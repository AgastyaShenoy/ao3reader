package org.ao3reader

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.os.Handler
import android.os.Looper
import android.view.GestureDetector
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.OverScroller
import android.widget.TextView
import org.json.JSONObject
import java.util.concurrent.LinkedBlockingDeque
import kotlin.math.max
import kotlin.math.min

/** The book open over the screens: laid-out text ([ReaderScreen]) or a PDF's pages ([PdfScreen]). */
interface BookView {
    val view: View
    /** Back button: closes a panel first. Returns false when the book itself should close. */
    fun back(): Boolean
    fun saveProgress()
    fun setInsets(t: Int, b: Int, cutout: Int)
    /** Recreates the chrome (after a theme change) while keeping the reading position. */
    fun rebuild()
    /** The book is closing: let go of what it holds. */
    fun close()
}

/** A PDF: its pages as they are, one under another at the screen's width. The Rust engine draws
 *  them on a background thread; tap the middle for the toolbar. */
class PdfScreen(private val a: MainActivity, val workId: String, opened: JSONObject) : FrameLayout(a), BookView {
    private val meta: JSONObject = opened.getJSONObject("meta")
    private val pages = PdfPages(a, this)
    private var top = LinearLayout(a)
    private var pageLabel = TextView(a)
    private var chrome = false
    private var insetTop = 0
    private var insetBottom = 0
    private val handler = Handler(Looper.getMainLooper())
    private val saveLater = Runnable { saveProgress() }
    private var dirty = false

    init {
        val sizes = Core.obj("pdf.open", json("id" to workId)).getJSONArray("sizes")
        pages.sizes = (0 until sizes.length()).map { val s = sizes.getJSONArray(it); s.getDouble(0).toFloat() to s.getDouble(1).toFloat() }
        pages.goTo(opened.optDouble("position", 0.0).toFloat())
        rebuild()
    }

    override val view: View get() = this

    override fun setInsets(t: Int, b: Int, cutout: Int) {
        insetTop = t
        insetBottom = b
        pages.safeTop = cutout
        top.setPadding(a.dp(6), t + a.dp(6), a.dp(6), a.dp(6))
    }

    override fun rebuild() {
        val p = Settings.pal
        removeAllViews()
        setBackgroundColor(p.bg)
        addView(pages, frame())
        pages.recolor()
        top = a.row().apply { setBackgroundColor(p.surface) }
        top.addView(a.iconButton(R.drawable.ic_back, p.text, "Back to library", large = true) { a.closeReader() })
        top.addView(a.label(meta.optString("title"), 18f, p.text, bold = true, lines = 1).apply { gravity = Gravity.CENTER_VERTICAL },
            LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply { leftMargin = a.dp(6) })
        pageLabel = a.label("", 15f, p.muted, lines = 1).apply { setPadding(a.dp(8), 0, a.dp(14), 0) }
        top.addView(pageLabel)
        addView(top, frame(h = ViewGroup.LayoutParams.WRAP_CONTENT, gravity = Gravity.TOP))
        setInsets(insetTop, insetBottom, pages.safeTop)
        top.gone(!chrome)
        a.immersive(!chrome)
        positionChanged()
    }

    fun toggleChrome() {
        chrome = !chrome
        if (chrome) Anim.enter(top.also { it.gone(false) }, dy = -a.dpf(20), ms = 170) else top.gone(true)
        a.immersive(!chrome)
    }

    /** Updates the page shown in the toolbar and schedules a progress save. */
    fun positionChanged() {
        val n = pages.sizes.size
        pageLabel.text = "Page ${min(pages.currentPage() + 1, max(n, 1))} of $n"
        pages.contentDescription = "${meta.optString("title")}, page ${pages.currentPage() + 1} of $n"
        dirty = true
        handler.removeCallbacks(saveLater)
        handler.postDelayed(saveLater, 3000)
    }

    override fun saveProgress() {
        if (!dirty) return
        dirty = false
        runCatching { Core.req("progress.save", json("chapter" to 0, "position" to pages.anchor().toDouble(), "completed" to pages.atEnd())) }
    }

    override fun back(): Boolean = false

    override fun close() {
        handler.removeCallbacks(saveLater)
        pages.stop()
        runCatching { Core.req("pdf.close") }
    }
}

/** The pages, scrolled with a finger or by TalkBack. Each page is rendered whole at the view's
 *  width and kept in a small cache; a background thread with a deep stack does the rendering. */
class PdfPages(context: Context, private val screen: PdfScreen) : View(context) {
    var sizes: List<Pair<Float, Float>> = emptyList()
    var safeTop = 0
    private var scrollPos = 0f
    private var pending: Float? = null
    private val gap get() = context.dpf(10)
    private val scroller = OverScroller(context)
    private val paint = Paint(Paint.FILTER_BITMAP_FLAG)
    private val boxPaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private var look = 0

    // Page tops and heights at the current width and zoom.
    @Volatile private var scale = 1f
    /** 1 fits the widest page to the screen; up to 4 (pinch, or double-tap for 2). */
    private var zoom = 1f
    /** How far a zoomed page is panned sideways. */
    private var panX = 0f
    /** A pinch under way: drawn scaled about (pinchX, pinchY) until the fingers lift. */
    private var pinch = 1f
    private var pinchX = 0f
    private var pinchY = 0f
    private var widest = 1f
    private var tops = FloatArray(0)
    private var heights = FloatArray(0)
    private var totalH = 0f
    private var laidFor = -1

    private val main = Handler(Looper.getMainLooper())
    // Room for four pages at the most a page is rendered at (MAX_PAGE_BYTES), so the pages on
    // screen and the next one never push each other out; zoomed pages are bigger and fewer.
    private val bitmaps = object : android.util.LruCache<Int, Bitmap>(4 * MAX_PAGE_BYTES) {
        override fun sizeOf(key: Int, value: Bitmap) = value.byteCount
    }
    /** Pages waiting to be rendered (most wanted first) and those being rendered. */
    private val queue = LinkedBlockingDeque<Int>()
    private val busy = java.util.Collections.synchronizedSet(HashSet<Int>())
    @Volatile private var generation = 0
    @Volatile private var running = true
    private val worker = Thread(null, ::renderLoop, "pdf-render", 16L shl 20).apply { isDaemon = true; start() }

    fun stop() {
        running = false
        worker.interrupt()
    }

    /** The theme changed: pages are drawn again in its colours. */
    fun recolor() {
        val p = Settings.pal
        val want = when {
            p.dark -> 1
            p.id == "WarmSepia" -> 2
            else -> 0
        }
        if (want != look) {
            look = want
            generation++
            bitmaps.evictAll()
        }
        invalidate()
    }

    private fun layoutPages() {
        if (width == 0 || sizes.isEmpty() || laidFor == width) return
        val anchor = pending ?: if (laidFor > 0) anchor() else 0f
        laidFor = width
        widest = sizes.maxOf { it.first }.coerceAtLeast(1f)
        scale = (width - 2 * gap) / widest * zoom
        tops = FloatArray(sizes.size)
        heights = FloatArray(sizes.size)
        var y = safeTop + gap
        for ((i, s) in sizes.withIndex()) {
            tops[i] = y
            heights[i] = s.second * scale
            y += heights[i] + gap
        }
        totalH = y
        generation++
        bitmaps.evictAll()
        pending = null
        scrollPos = yFor(anchor)
        clamp()
    }

    override fun onSizeChanged(w: Int, h: Int, ow: Int, oh: Int) {
        if (ow > 0 && pending == null) pending = anchor()
        laidFor = -1
        layoutPages()
    }

    private fun maxScroll() = max(0f, totalH - height)

    /** How far a zoomed page can pan: the widest page plus its margins, less the screen. */
    private fun maxPan() = max(0f, widest * scale + 2 * gap - width)

    private fun clamp() {
        scrollPos = scrollPos.coerceIn(0f, maxScroll())
        panX = panX.coerceIn(0f, maxPan())
    }

    /** Zooms to `z` (1 to 4), keeping the point under (fx, fy) where it is. */
    private fun setZoom(z: Float, fx: Float, fy: Float) {
        val nz = z.coerceIn(1f, 4f)
        if (kotlin.math.abs(nz - zoom) < 0.01f) return
        val ratio = nz / zoom
        val docY = scrollPos + fy
        val docX = panX + fx
        zoom = nz
        laidFor = -1
        pending = null
        layoutPages()
        scrollPos = docY * ratio - fy
        panX = docX * ratio - fx
        clamp()
        invalidate()
        screen.positionChanged()
    }

    fun currentPage(): Int {
        if (tops.isEmpty()) return 0
        val mid = scrollPos + height / 3f
        val i = tops.indexOfLast { it <= mid }
        return i.coerceIn(0, tops.size - 1)
    }

    /** Page + fraction at the top of the screen. */
    fun anchor(): Float {
        if (tops.isEmpty()) return pending ?: 0f
        if (scrollPos >= maxScroll() - 1f && maxScroll() > 0) return sizes.size.toFloat()
        val y = scrollPos + safeTop + gap
        val i = tops.indexOfLast { it <= y }.coerceAtLeast(0)
        return i + ((y - tops[i]) / heights[i].coerceAtLeast(1f)).coerceIn(0f, 0.999f)
    }

    private fun yFor(anchor: Float): Float {
        if (tops.isEmpty()) return 0f
        val i = anchor.toInt().coerceIn(0, tops.size - 1)
        return tops[i] + (anchor - i).coerceIn(0f, 1f) * heights[i] - safeTop - gap
    }

    fun atEnd(): Boolean = anchor() >= sizes.size - 0.05f

    fun goTo(anchor: Float) {
        if (tops.isEmpty()) {
            pending = anchor
            return
        }
        scrollPos = yFor(anchor)
        clamp()
        invalidate()
    }

    private fun scrollBy(dy: Float) {
        scrollPos += dy
        clamp()
        invalidate()
        screen.positionChanged()
    }

    // ---- rendering ----------------------------------------------------------------------

    private fun renderLoop() {
        while (running) {
            val page = try { queue.takeFirst() } catch (e: InterruptedException) { return }
            val gen = generation
            val (pw, ph) = sizes.getOrNull(page) ?: continue
            // A big screen in landscape would want a page bitmap of 30 MB or more: render at most
            // MAX_PAGE_BYTES (twice that when zoomed in) and let drawing scale it up.
            val most = if (zoom > 1.01f) 2 * MAX_PAGE_BYTES else MAX_PAGE_BYTES
            val s = minOf(scale, kotlin.math.sqrt(most / (4f * pw * ph).coerceAtLeast(1f)))
            val w = (pw * s).toInt()
            val h = (ph * s).toInt()
            if (w <= 0 || h <= 0) continue
            busy.add(page)
            val px = Core.renderPdf(page, s, 0, 0, w, h, look)
            val bmp = px?.let { Bitmap.createBitmap(it, w, h, Bitmap.Config.ARGB_8888) }
            main.post {
                busy.remove(page)
                if (bmp != null && gen == generation) {
                    bitmaps.put(page, bmp)
                    invalidate()
                }
            }
        }
    }

    override fun onDraw(c: Canvas) {
        layoutPages()
        val p = Settings.pal
        c.drawColor(p.bg)
        if (tops.isEmpty()) return
        val top = scrollPos
        val bottom = scrollPos + height
        queue.clear()
        if (pinch != 1f) {
            c.save()
            c.scale(pinch, pinch, pinchX, pinchY)
        }
        // Zoomed wider than the screen, pages line up on the widest and pan together.
        val wide = widest * scale + 2 * gap > width
        // The pages on screen, then the next one down.
        var last = -1
        for (i in tops.indices) {
            if (tops[i] + heights[i] < top) continue
            if (tops[i] > bottom) break
            last = i
            val w = sizes[i].first * scale
            val x = if (wide) gap + (widest * scale - w) / 2 - panX else (width - w) / 2
            val r = RectF(x, tops[i] - top, x + w, tops[i] - top + heights[i])
            val bmp = bitmaps.get(i)
            if (bmp != null) c.drawBitmap(bmp, null, r, paint) else {
                boxPaint.style = Paint.Style.FILL
                boxPaint.color = if (look == 1) 0xFF262626.toInt() else if (look == 2) 0xFFF7EED6.toInt() else 0xFFFFFFFF.toInt()
                c.drawRect(r, boxPaint)
            }
            boxPaint.style = Paint.Style.STROKE
            boxPaint.strokeWidth = context.dpf(1)
            boxPaint.color = p.border
            c.drawRect(r, boxPaint)
            if (bmp == null && i !in busy) queue.offerLast(i)
        }
        val next = last + 1
        if (next in tops.indices && bitmaps.get(next) == null && next !in busy) queue.offerLast(next)
        if (pinch != 1f) c.restore()
    }

    // ---- touch ----------------------------------------------------------------------------

    private val gestures = GestureDetector(context, object : GestureDetector.SimpleOnGestureListener() {
        override fun onDown(e: MotionEvent): Boolean {
            if (!scroller.isFinished) scroller.forceFinished(true)
            return true
        }

        // Confirmed, so a double-tap (zoom) doesn't also show and hide the bars.
        override fun onSingleTapConfirmed(e: MotionEvent): Boolean {
            screen.toggleChrome()
            return true
        }

        override fun onScroll(e1: MotionEvent?, e2: MotionEvent, dx: Float, dy: Float): Boolean {
            if (zoom > 1.01f) panX += dx
            scrollBy(dy)
            return true
        }

        override fun onDoubleTap(e: MotionEvent): Boolean {
            // Double-tap zooms in to twice the size, or back to the page's width.
            setZoom(if (zoom > 1.01f) 1f else 2f, e.x, e.y)
            return true
        }

        override fun onFling(e1: MotionEvent?, e2: MotionEvent, vx: Float, vy: Float): Boolean {
            scroller.fling(0, scrollPos.toInt(), 0, -vy.toInt(), 0, 0, 0, maxScroll().toInt())
            postInvalidateOnAnimation()
            return true
        }
    })

    private val pinches = android.view.ScaleGestureDetector(context, object : android.view.ScaleGestureDetector.SimpleOnScaleGestureListener() {
        override fun onScale(d: android.view.ScaleGestureDetector): Boolean {
            pinch = (pinch * d.scaleFactor).coerceIn(1f / zoom, 4f / zoom)
            pinchX = d.focusX
            pinchY = d.focusY
            invalidate()
            return true
        }

        override fun onScaleEnd(d: android.view.ScaleGestureDetector) {
            val z = zoom * pinch
            pinch = 1f
            setZoom(z, pinchX, pinchY)
            invalidate()
        }
    })

    override fun onTouchEvent(event: MotionEvent): Boolean {
        pinches.onTouchEvent(event)
        if (pinches.isInProgress) return true
        return gestures.onTouchEvent(event) || super.onTouchEvent(event)
    }

    override fun computeScroll() {
        if (scroller.computeScrollOffset()) {
            scrollPos = scroller.currY.toFloat()
            clamp()
            screen.positionChanged()
            postInvalidateOnAnimation()
        }
    }

    override fun onInitializeAccessibilityNodeInfo(info: AccessibilityNodeInfo) {
        super.onInitializeAccessibilityNodeInfo(info)
        // TalkBack's scroll gestures move a screen at a time.
        info.isScrollable = true
        info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_FORWARD)
        info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_BACKWARD)
    }

    override fun performAccessibilityAction(action: Int, args: android.os.Bundle?): Boolean = when (action) {
        AccessibilityNodeInfo.ACTION_SCROLL_FORWARD -> { scrollBy(height * 0.85f); true }
        AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD -> { scrollBy(-height * 0.85f); true }
        else -> super.performAccessibilityAction(action, args)
    }
}

/** The most one rendered PDF page takes (ARGB, 4 bytes a pixel). */
private const val MAX_PAGE_BYTES = 12 * 1024 * 1024
