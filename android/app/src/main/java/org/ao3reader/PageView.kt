package org.ao3reader

import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.graphics.Typeface
import android.text.Layout
import android.text.SpannableString
import android.text.Spanned
import android.text.StaticLayout
import android.text.TextPaint
import android.text.style.BackgroundColorSpan
import android.text.style.StrikethroughSpan
import android.text.style.StyleSpan
import android.view.GestureDetector
import android.view.MotionEvent
import android.view.View
import android.widget.OverScroller
import org.json.JSONObject
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min

/** A drawable piece of a block: text, a rounded box, or a pill. */
private sealed class El {
    class Text(val layout: StaticLayout, val x: Float, val y: Float, val color: Int) : El()
    class Box(val r: RectF, val radius: Float, val fill: Int, val stroke: Int = 0, val strokeW: Float = 0f) : El()
}

/** One measured block of a chapter; `lines` are text line bottoms in block coordinates. */
private class Block(val kind: String, val para: Int, val els: List<El>, val h: Float, val lines: FloatArray, var y: Float = 0f)

private class Slice(val block: Int, val from: Float, val to: Float)
private class Page(val slices: List<Slice>, val y: Float)

/** A search match: chapter, paragraph, UTF-16 start and end. */
class Hit(val chapter: Int, val para: Int, val start: Int, val end: Int)

/**
 * Paged or scrolling chapter view. Every block is laid out with the platform's StaticLayout
 * once per (chapter, size, typography); pages are cut at real line boundaries, and positions
 * are paragraph anchors (paragraph index + fraction) that survive any re-layout.
 */
class PageView(context: Context, private val reader: ReaderScreen) : View(context) {
    private var blocks = listOf<Block>()
    private var pages = listOf<Page>()
    private var totalH = 0f
    private var paraCount = 0
    private var colW = 0f
    private var dual = false
    private var key = ""

    var paged = true
    var page = 0
        private set
    private var scrollPos = 0f
    private var pendingAnchor: Float? = 0f

    private val scroller = OverScroller(context)
    private val headPaint = TextPaint(Paint.ANTI_ALIAS_FLAG)
    private val boxPaint = Paint(Paint.ANTI_ALIAS_FLAG)

    /** Space kept clear at the top for a camera cutout. */
    var safeTop = 0

    private val margin get() = context.dp(if (width > context.dp(700)) 48 else 22).toFloat()
    private val headH get() = safeTop + context.dpf(34)
    private val footH get() = context.dpf(30)
    private val pageH get() = (height - headH - footH).coerceAtLeast(context.dpf(120))

    val pageCount get() = pages.size
    val spread get() = dual

    fun setChapter(anchor: Float) {
        key = ""
        pendingAnchor = anchor
        page = 0
        scrollPos = 0f
        requestLayoutBlocks()
    }

    fun relayout() {
        pendingAnchor = anchor()
        key = ""
        requestLayoutBlocks()
    }

    private fun requestLayoutBlocks() {
        if (width > 0 && height > 0) ensureLayout()
        invalidate()
    }

    override fun onSizeChanged(w: Int, h: Int, ow: Int, oh: Int) {
        if (ow > 0 && pendingAnchor == null) pendingAnchor = anchor()
        key = ""
        ensureLayout()
    }

    // ---- layout -----------------------------------------------------------------------

    private fun typeface(t: JSONObject): Typeface = when (t.opt("font_family")) {
        "Serif" -> Typeface.SERIF
        "Monospace" -> Typeface.MONOSPACE
        else -> Typeface.SANS_SERIF
    }

    private fun ensureLayout() {
        val t = Settings.typo
        val wide = width >= context.dp(840)
        val layoutMode = t.optString("page_layout")
        dual = paged && wide && layoutMode != "Single"
        val gutter = context.dpf(56)
        colW = if (dual) (width - 2 * margin - gutter) / 2 else width - 2 * margin
        val k = "${reader.chapter}|$width|$height|$paged|$dual|$t|${reader.hitsKey}"
        if (k == key) return
        key = k
        build(t)
        pendingAnchor?.let { goTo(it) }
        pendingAnchor = null
        reader.positionChanged()
    }

    private fun spannable(text: String, spans: org.json.JSONArray, shift: Int, hits: List<Hit>, para: Int): SpannableString {
        val s = SpannableString(text)
        for (i in 0 until spans.length()) {
            val sp = spans.getJSONArray(i)
            val a = sp.getInt(0) + shift
            val b = sp.getInt(1) + shift
            if (a >= b || b > s.length) continue
            val style = when (sp.getString(2)) {
                "b" -> StyleSpan(Typeface.BOLD)
                "s" -> StrikethroughSpan()
                else -> StyleSpan(Typeface.ITALIC)
            }
            s.setSpan(style, a, b, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
        }
        val p = Settings.pal
        for (h in hits) {
            if (h.chapter != reader.chapter || h.para != para) continue
            val a = h.start + shift
            val b = h.end + shift
            if (a < 0 || b > s.length || a >= b) continue
            val current = reader.currentHit === h
            s.setSpan(BackgroundColorSpan(alpha(p.accent, if (current) 0.55f else 0.22f)), a, b, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
        }
        return s
    }

    private fun staticLayout(text: CharSequence, paint: TextPaint, w: Int, lineMult: Float, align: Layout.Alignment, justify: Boolean): StaticLayout {
        val natural = paint.fontSpacing
        val add = (paint.textSize * lineMult - natural).coerceAtLeast(0f)
        val b = StaticLayout.Builder.obtain(text, 0, text.length, paint, w.coerceAtLeast(1))
            .setAlignment(align)
            .setLineSpacing(add, 1f)
            .setIncludePad(false)
        if (justify) b.setJustificationMode(Layout.JUSTIFICATION_MODE_INTER_WORD)
        return b.build()
    }

    private fun bottoms(l: StaticLayout, offset: Float): FloatArray = FloatArray(l.lineCount) { l.getLineBottom(it) + offset }

    private fun build(t: JSONObject) {
        val p = Settings.pal
        val ch = reader.chapterJson ?: return
        val sd = resources.displayMetrics.scaledDensity
        val size = t.optDouble("font_size", 18.0).toFloat() * sd
        val mult = t.optDouble("line_spacing_multiplier", 1.6).toFloat()
        val gap = t.optDouble("paragraph_spacing", 14.0).toFloat() * resources.displayMetrics.density
        val justify = t.optString("text_alignment") == "Justified"
        val indent = t.optBoolean("indent_paragraphs")
        val tf = typeface(t)
        val body = TextPaint(Paint.ANTI_ALIAS_FLAG).apply { typeface = tf; textSize = size; color = p.text }
        val small = TextPaint(body).apply { textSize = size * 0.9f }
        val w = colW.toInt()
        val pad = context.dpf(16)
        val hits = reader.hits
        val out = mutableListOf<Block>()

        if (reader.chapter == 0) out += preface(w)
        if (reader.chapterCount > 1) {
            val title = ch.optString("title").trim()
            if (title.isNotEmpty()) {
                val tp = TextPaint(body).apply { textSize = size * 1.45f; typeface = Typeface.create(tf, Typeface.BOLD) }
                val l = staticLayout(title, tp, w, 1.25f, Layout.Alignment.ALIGN_CENTER, false)
                out += Block("title", 0, listOf(El.Text(l, 0f, context.dpf(6), p.text)), l.height + context.dpf(30), bottoms(l, context.dpf(6)))
            }
        }

        val blocksJson = ch.getJSONArray("blocks")
        for (i in 0 until blocksJson.length()) {
            val b = blocksJson.getJSONObject(i)
            val kind = b.getString("kind")
            val para = b.getInt("para")
            val text = b.getString("text")
            when (kind) {
                "summary", "notes", "endnotes" -> {
                    if (kind == "summary" && !t.optBoolean("show_chapter_summary", true)) continue
                    if (kind == "notes" && !t.optBoolean("show_chapter_pre_notes", true)) continue
                    if (kind == "endnotes" && !t.optBoolean("show_chapter_post_notes", true)) continue
                    val top = if (kind == "endnotes") context.dpf(12) else 0f
                    val labelText = mapOf("summary" to "Chapter summary", "notes" to "Notes", "endnotes" to "End notes")[kind]!!
                    val lp = TextPaint(Paint.ANTI_ALIAS_FLAG).apply { textSize = 12.5f * sd; color = p.accent; typeface = Typeface.DEFAULT_BOLD }
                    val label = staticLayout(labelText, lp, w, 1.2f, Layout.Alignment.ALIGN_NORMAL, false)
                    val tl = staticLayout(spannable(text, b.getJSONArray("spans"), 0, emptyList(), para), small, (w - 2 * pad).toInt(), min(mult, 1.7f), Layout.Alignment.ALIGN_NORMAL, false)
                    val textY = top + pad + label.height + context.dpf(6)
                    val h = textY + tl.height + pad
                    out += Block(kind, para, listOf(
                        El.Box(RectF(0f, top, w.toFloat(), h), context.dpf(10), p.notesBg, p.border, context.dpf(1)),
                        El.Text(label, pad, top + pad, p.accent),
                        El.Text(tl, pad, textY, alpha(p.text, 0.9f)),
                    ), h + context.dpf(18), bottoms(tl, textY))
                }
                "divider" -> {
                    val dp = TextPaint(body).apply { color = p.muted }
                    val l = staticLayout("⁂", dp, w, 1f, Layout.Alignment.ALIGN_CENTER, false)
                    out += Block(kind, para, listOf(El.Text(l, 0f, size * 0.6f, p.muted)), size * 2.4f + gap, FloatArray(0))
                }
                "quote" -> {
                    val inset = context.dpf(20)
                    val tl = staticLayout(spannable(text, b.getJSONArray("spans"), 0, hits, para), body, (w - inset - context.dpf(12)).toInt(), mult, Layout.Alignment.ALIGN_NORMAL, justify)
                    val h = tl.height + context.dpf(20)
                    out += Block(kind, para, listOf(
                        El.Box(RectF(0f, 0f, w.toFloat(), h), context.dpf(6), p.quoteBg),
                        El.Box(RectF(0f, 0f, context.dpf(3), h), context.dpf(1.5f), alpha(p.accent, 0.7f)),
                        El.Text(tl, inset, context.dpf(10), p.text),
                    ), h + gap, bottoms(tl, context.dpf(10)))
                }
                else -> {
                    val prefix = if (indent) "  " else ""
                    val tl = staticLayout(spannable(prefix + text, b.getJSONArray("spans"), prefix.length, hits, para), body, w, mult, Layout.Alignment.ALIGN_NORMAL, justify)
                    out += Block("para", para, listOf(El.Text(tl, 0f, 0f, p.text)), tl.height + gap, bottoms(tl, 0f))
                }
            }
        }
        out += Block("end", ch.getInt("paragraphs"), emptyList(), context.dpf(110), FloatArray(0))

        var y = 0f
        for (b in out) {
            b.y = y
            y += b.h
        }
        blocks = out
        totalH = y
        paraCount = ch.getInt("paragraphs")
        pages = if (paged) paginate(out, pageH) else emptyList()
    }

    /** Work header shown before chapter 1. */
    private fun preface(w: Int): Block {
        val p = Settings.pal
        val m = reader.meta
        val sd = resources.displayMetrics.scaledDensity
        val els = mutableListOf<El>()
        var y = context.dpf(4)
        fun text(s: String, sp: Float, color: Int, bold: Boolean = false, lines: Int = 0) {
            val tp = TextPaint(Paint.ANTI_ALIAS_FLAG).apply { textSize = sp * sd; this.color = color; if (bold) typeface = Typeface.DEFAULT_BOLD }
            var l = staticLayout(s, tp, w, 1.3f, Layout.Alignment.ALIGN_NORMAL, false)
            if (lines > 0 && l.lineCount > lines) {
                val cut = l.getLineEnd(lines - 1)
                l = staticLayout(s.substring(0, (cut - 1).coerceAtLeast(0)).trimEnd() + "…", tp, w, 1.3f, Layout.Alignment.ALIGN_NORMAL, false)
            }
            els += El.Text(l, 0f, y, color)
            y += l.height
        }
        fun pills(items: List<Triple<String, Int, Boolean>>) {
            val tp = TextPaint(Paint.ANTI_ALIAS_FLAG).apply { textSize = 12.5f * sd }
            var x = 0f
            val h = context.dpf(24)
            for ((label, color, outlined) in items) {
                tp.color = if (outlined) p.text else p.onAccent
                val l = staticLayout(label, TextPaint(tp), (w - context.dp(20)), 1f, Layout.Alignment.ALIGN_NORMAL, false)
                val pw = l.getLineWidth(0) + context.dpf(20)
                if (x > 0 && x + pw > w) { x = 0f; y += h + context.dpf(6) }
                els += if (outlined) El.Box(RectF(x, y, x + pw, y + h), h / 2, 0, color, context.dpf(1)) else El.Box(RectF(x, y, x + pw, y + h), h / 2, color)
                els += El.Text(l, x + context.dpf(10), y + (h - l.height) / 2, tp.color)
                x += pw + context.dpf(6)
            }
            if (items.isNotEmpty()) y += h
        }
        text(m.getString("title"), 24f, p.text, bold = true)
        y += context.dpf(4)
        text("by " + m.getJSONArray("authors").strings().joinToString(", "), 14f, p.muted)
        y += context.dpf(12)
        val badges = mutableListOf(Triple(m.getString("rating"), Pal.opaque(m.getInt("rating_color")), false))
        m.getJSONArray("categories").objects().forEach { badges += Triple(it.getString("name"), Pal.opaque(it.getInt("color")), false) }
        m.getJSONArray("warnings").strings().forEach { badges += Triple(it, if ("No Archive Warnings" in it) 0xFF228B22.toInt() else 0xFFB42828.toInt(), false) }
        pills(badges)
        y += context.dpf(10)
        fun tagLine(label: String, key: String) {
            val v = m.getJSONArray(key).strings()
            if (v.isEmpty()) return
            text("$label  " + v.joinToString(" · "), 13f, p.muted, lines = 3)
            y += context.dpf(6)
        }
        tagLine("Fandoms", "fandoms")
        val rels = m.getJSONArray("relationships").strings().take(8)
        if (rels.isNotEmpty()) {
            pills(rels.map { Triple(it, p.accent, true) })
            y += context.dpf(8)
        }
        tagLine("Characters", "characters")
        tagLine("Tags", "tags")
        val summary = m.getString("summary").trim()
        if (summary.isNotEmpty()) {
            y += context.dpf(6)
            val pad = context.dpf(14)
            val lp = TextPaint(Paint.ANTI_ALIAS_FLAG).apply { textSize = 12.5f * sd; color = p.accent; typeface = Typeface.DEFAULT_BOLD }
            val label = staticLayout("Summary", lp, w, 1.2f, Layout.Alignment.ALIGN_NORMAL, false)
            val sp = TextPaint(Paint.ANTI_ALIAS_FLAG).apply { textSize = 14f * sd; color = p.text }
            val maxLines = if (paged) ((pageH * 0.45f) / (14f * sd * 1.45f)).toInt().coerceAtLeast(3) else Int.MAX_VALUE
            var sl = staticLayout(summary, sp, (w - 2 * pad).toInt(), 1.45f, Layout.Alignment.ALIGN_NORMAL, false)
            if (sl.lineCount > maxLines) {
                val cut = sl.getLineEnd(maxLines - 1)
                sl = staticLayout(summary.substring(0, (cut - 1).coerceAtLeast(0)).trimEnd() + "…", sp, (w - 2 * pad).toInt(), 1.45f, Layout.Alignment.ALIGN_NORMAL, false)
            }
            val h = pad + label.height + context.dpf(6) + sl.height + pad
            els += El.Box(RectF(0f, y, w.toFloat(), y + h), context.dpf(10), p.notesBg, p.border, context.dpf(1))
            els += El.Text(label, pad, y + pad, p.accent)
            els += El.Text(sl, pad, y + pad + label.height + context.dpf(6), p.text)
            y += h + context.dpf(12)
        }
        val stats = mutableListOf("${thousands(m.getLong("words"))} words", "Chapters ${m.getInt("chapters")}/${m.opt("chapters_total")?.takeIf { it != JSONObject.NULL } ?: "?"}")
        if (m.getLong("kudos") > 0) stats += "${thousands(m.getLong("kudos"))} kudos"
        text(stats.joinToString("  ·  "), 12.5f, p.muted)
        y += context.dpf(14)
        els += El.Box(RectF(0f, y, w.toFloat(), y + context.dpf(1)), 0f, p.border)
        y += context.dpf(28)
        return Block("preface", 0, els, y, FloatArray(0))
    }

    private fun paginate(bs: List<Block>, ph: Float): List<Page> {
        val out = mutableListOf<Page>()
        var cur = mutableListOf<Slice>()
        var curY = 0f
        var used = 0f
        fun flush(nextY: Float) {
            out += Page(cur, curY)
            cur = mutableListOf()
            curY = nextY
            used = 0f
        }
        bs.forEachIndexed { bi, b ->
            var from = 0f
            while (true) {
                val remaining = ph - used
                val rest = b.h - from
                if (rest <= remaining + 0.5f) {
                    cur += Slice(bi, from, b.h)
                    used += rest
                    break
                }
                val textEnd = b.lines.lastOrNull() ?: -1f
                if (textEnd >= 0 && textEnd - from <= remaining + 0.5f) {
                    // Only the spacing under the text overflows: keep the text, start a new page.
                    cur += Slice(bi, from, min(b.h, from + remaining))
                    flush(b.y + b.h)
                    break
                }
                val cut = b.lines.filter { it > from + 0.5f && it - from <= remaining + 0.5f }.maxOrNull()
                when {
                    cut != null -> {
                        cur += Slice(bi, from, cut)
                        flush(b.y + cut)
                        from = cut
                    }
                    used > 0f -> flush(b.y + from)
                    else -> {
                        val to = min(from + ph, b.h)
                        cur += Slice(bi, from, to)
                        flush(b.y + to)
                        from = to
                        if (from >= b.h - 0.5f) break
                    }
                }
            }
        }
        if (cur.isNotEmpty() || out.isEmpty()) out += Page(cur, curY)
        return out
    }

    // ---- positions ----------------------------------------------------------------------

    private fun anchorAt(y: Float): Float {
        val b = blocks.lastOrNull { it.y <= y + 0.5f } ?: return 0f
        return when (b.kind) {
            "para", "quote", "divider" -> b.para + ((y - b.y) / max(b.h, 1f)).coerceIn(0f, 0.999f)
            "endnotes", "end" -> paraCount.toFloat()
            else -> 0f
        }
    }

    private fun yFor(anchor: Float): Float {
        if (anchor <= 0f) return 0f
        val idx = anchor.toInt()
        if (idx >= paraCount) return blocks.firstOrNull { it.kind == "endnotes" || it.kind == "end" }?.y ?: totalH
        val b = blocks.firstOrNull { it.para == idx && it.kind in setOf("para", "quote", "divider") } ?: return 0f
        return b.y + b.h * (anchor - idx)
    }

    fun anchor(): Float {
        if (blocks.isEmpty()) return pendingAnchor ?: 0f
        return if (paged) {
            val last = min(page + (if (dual) 1 else 0), pages.size - 1)
            if (last >= pages.size - 1) paraCount.toFloat() else anchorAt(pages.getOrNull(page)?.y ?: 0f)
        } else {
            val maxScroll = max(0f, totalH + headH - height)
            if (scrollPos >= maxScroll - 1f && maxScroll > 0) paraCount.toFloat() else anchorAt(scrollPos)
        }
    }

    fun atEnd(): Boolean = anchor() >= paraCount - 0.05f

    fun goTo(anchor: Float) {
        if (blocks.isEmpty()) {
            pendingAnchor = anchor
            return
        }
        val y = if (anchor >= Float.MAX_VALUE / 2) totalH else yFor(anchor)
        if (paged) {
            page = pages.indexOfLast { it.y <= y + 0.5f }.coerceAtLeast(0)
            if (dual) page -= page % 2
        } else {
            scrollPos = y.coerceIn(0f, max(0f, totalH + headH - height))
        }
        invalidate()
    }

    // ---- navigation ---------------------------------------------------------------------

    fun turn(forward: Boolean) {
        val step = if (dual) 2 else 1
        if (forward) {
            if (page + step < pages.size) page += step else { reader.nextChapter(); return }
        } else {
            if (page > 0) page = max(0, page - step) else { reader.prevChapter(toEnd = true); return }
        }
        invalidate()
        reader.positionChanged()
    }

    private fun scrollBy(dy: Float) {
        val maxScroll = max(0f, totalH + headH - height)
        scrollPos = (scrollPos + dy).coerceIn(0f, maxScroll)
        invalidate()
        reader.positionChanged()
    }

    private val gestures = GestureDetector(context, object : GestureDetector.SimpleOnGestureListener() {
        override fun onDown(e: MotionEvent): Boolean {
            if (!scroller.isFinished) scroller.forceFinished(true)
            return true
        }

        override fun onSingleTapUp(e: MotionEvent): Boolean {
            val x = e.x / width
            if (!paged) {
                val endBlock = blocks.lastOrNull()
                if (endBlock != null && e.y + scrollPos - headH >= endBlock.y) { reader.nextChapter(); return true }
                reader.toggleChrome()
                return true
            }
            when {
                x < 0.28f -> turn(false)
                x > 0.72f -> turn(true)
                else -> reader.toggleChrome()
            }
            return true
        }

        override fun onScroll(e1: MotionEvent?, e2: MotionEvent, dx: Float, dy: Float): Boolean {
            if (!paged) scrollBy(dy)
            return true
        }

        override fun onFling(e1: MotionEvent?, e2: MotionEvent, vx: Float, vy: Float): Boolean {
            if (paged) {
                if (abs(vx) > abs(vy) && abs(vx) > context.dpf(300)) turn(vx < 0)
            } else {
                val maxScroll = max(0f, totalH + headH - height).toInt()
                scroller.fling(0, scrollPos.toInt(), 0, -vy.toInt(), 0, 0, 0, maxScroll)
                postInvalidateOnAnimation()
            }
            return true
        }
    })

    override fun onTouchEvent(event: MotionEvent): Boolean = gestures.onTouchEvent(event) || super.onTouchEvent(event)

    override fun computeScroll() {
        if (!paged && scroller.computeScrollOffset()) {
            scrollPos = scroller.currY.toFloat()
            reader.positionChanged()
            postInvalidateOnAnimation()
        }
    }

    // ---- drawing ------------------------------------------------------------------------

    private fun drawBlock(c: Canvas, b: Block, x: Float, y: Float) {
        c.save()
        c.translate(x, y)
        for (el in b.els) when (el) {
            is El.Box -> {
                if (el.fill != 0) {
                    boxPaint.style = Paint.Style.FILL
                    boxPaint.color = el.fill
                    c.drawRoundRect(el.r, el.radius, el.radius, boxPaint)
                }
                if (el.strokeW > 0) {
                    boxPaint.style = Paint.Style.STROKE
                    boxPaint.strokeWidth = el.strokeW
                    boxPaint.color = el.stroke
                    c.drawRoundRect(el.r, el.radius, el.radius, boxPaint)
                }
            }
            is El.Text -> {
                c.save()
                c.translate(el.x, el.y)
                el.layout.paint.color = el.color
                el.layout.draw(c)
                c.restore()
            }
        }
        if (b.kind == "end") {
            val p = Settings.pal
            headPaint.textSize = context.dpf(13)
            headPaint.color = p.muted
            headPaint.textAlign = Paint.Align.CENTER
            val last = reader.chapter + 1 >= reader.chapterCount
            val msg = if (last) "The end" else if (paged) "End of chapter" else "End of chapter · tap for the next one"
            c.drawText(msg, colW / 2, context.dpf(48), headPaint)
            headPaint.textAlign = Paint.Align.LEFT
        }
        c.restore()
    }

    override fun onDraw(c: Canvas) {
        val p = Settings.pal
        c.drawColor(p.bg)
        if (blocks.isEmpty()) return
        headPaint.textSize = context.dpf(12)
        headPaint.color = alpha(p.muted, 0.85f)
        headPaint.textAlign = Paint.Align.CENTER
        if (paged) {
            val gutter = context.dpf(56)
            val total = if (dual) colW * 2 + gutter else colW
            val x0 = (width - total) / 2
            for (k in 0 until if (dual) 2 else 1) {
                val pi = page + k
                if (pi >= pages.size) break
                val px = x0 + k * (colW + gutter)
                val head = if (dual && k == 0) reader.meta.getString("title") else reader.chapterJson?.optString("title") ?: ""
                c.drawText(android.text.TextUtils.ellipsize(head, headPaint, colW, android.text.TextUtils.TruncateAt.END).toString(), px + colW / 2, safeTop + context.dpf(22), headPaint)
                var y = headH
                for (s in pages[pi].slices) {
                    val h = s.to - s.from
                    c.save()
                    c.clipRect(px - context.dpf(8), y, px + colW + context.dpf(8), y + h)
                    drawBlock(c, blocks[s.block], px, y - s.from)
                    c.restore()
                    y += h
                }
                c.drawText("${pi + 1}", px + colW / 2, height - context.dpf(10), headPaint)
            }
            if (dual) {
                boxPaint.style = Paint.Style.FILL
                boxPaint.color = p.border
                val mx = x0 + colW + gutter / 2
                c.drawRect(mx, headH, mx + context.dpf(1), headH + pageH, boxPaint)
            }
        } else {
            val x = (width - colW) / 2
            val top = headH - scrollPos
            for (b in blocks) {
                if (top + b.y + b.h < 0 || top + b.y > height) continue
                drawBlock(c, b, x, top + b.y)
            }
        }
        headPaint.textAlign = Paint.Align.LEFT
    }
}
