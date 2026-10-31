package org.ao3reader

import android.animation.Animator
import android.animation.AnimatorListenerAdapter
import android.animation.ValueAnimator
import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.RectF
import android.graphics.Shader
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
import android.view.ViewConfiguration
import android.widget.OverScroller
import org.json.JSONObject
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min

/** A drawable piece of a block: text, a rounded box, or a pill. */
private sealed class El {
    class Text(val layout: StaticLayout, val x: Float, val y: Float, val color: Int) : El()
    class Box(val r: RectF, val radius: Float, val fill: Int, val stroke: Int = 0, val strokeW: Float = 0f) : El()
    class Icon(val d: android.graphics.drawable.Drawable, val r: RectF) : El()
    /** A picture file drawn into `r`; decoded only when it's on screen. */
    class Picture(val path: String, val r: RectF) : El()
}

/** Block kinds that stand for a paragraph of the text (and so carry reading positions). */
private val BODY = setOf("para", "quote", "divider", "heading", "item", "pre", "center", "image", "row")

/** One measured block of a chapter; `lines` are text line bottoms in block coordinates. */
/** `say` is the text read aloud, from element `sayEl` (whose text starts `shift` characters in). */
private class Block(val kind: String, val para: Int, val els: List<El>, val h: Float, val lines: FloatArray, var y: Float = 0f, val say: String = "", val sayEl: Int = -1, val shift: Int = 0)

private class Slice(val block: Int, val from: Float, val to: Float)
private class Page(val slices: List<Slice>, val y: Float)

/** A search match: chapter, paragraph, UTF-16 start and end. */
class Hit(val chapter: Int, val para: Int, val start: Int, val end: Int)

/** Block kinds whose text can be selected and highlighted. */
private val SELECTABLE = setOf("para", "quote", "heading", "item", "center")

/** A place in the text: a block, and a character offset into its laid-out text. */
data class TextPos(val block: Int, val at: Int) : Comparable<TextPos> {
    override fun compareTo(other: TextPos) = compareValuesBy(this, other, { it.block }, { it.at })
}

/** Where a block's text was last drawn: the part on screen, and where the text starts. */
private class Spot(val block: Int, val clip: RectF, val ox: Float, val oy: Float)

/** Highlight colours: yellow, green, blue, pink (as on the desktop app). */
val HIGHLIGHTS = intArrayOf(0xFACC15, 0x4ADE80, 0x60A5FA, 0xF472B6)
val HIGHLIGHT_NAMES = listOf("Yellow", "Green", "Blue", "Pink")
fun highlightFill(i: Int): Int = alpha(Pal.opaque(HIGHLIGHTS[i.coerceIn(0, 3)]), if (Settings.pal.dark) 0.30f else 0.38f)

private fun utf8Bytes(c: Int) = when {
    c < 0x80 -> 1
    c < 0x800 -> 2
    c < 0x10000 -> 3
    else -> 4
}

/** Bytes of UTF-8 in the first `n` characters of `s` (the core counts positions in bytes). */
fun utf8Len(s: String, n: Int): Int {
    var b = 0
    var i = 0
    val end = n.coerceIn(0, s.length)
    while (i < end) {
        val c = s.codePointAt(i)
        b += utf8Bytes(c)
        i += Character.charCount(c)
    }
    return b
}

/** The character position in `s` at its `bytes`-th byte of UTF-8. */
fun utf16At(s: String, bytes: Int): Int {
    var b = 0
    var i = 0
    while (i < s.length && b < bytes) {
        val c = s.codePointAt(i)
        b += utf8Bytes(c)
        i += Character.charCount(c)
    }
    return i
}

// Layout's measurements leave out the space that justification adds between words, though its
// drawing has it, so positions in justified text are worked out here.

/** Layouts built justified. Layout can only say so itself from Android 15 on
 *  (`getJustificationMode`), and calling that on Android 14 crashes, so the page view keeps
 *  track (weakly: a layout is dropped with its chapter). */
private val justifiedLayouts: MutableSet<Layout> = java.util.Collections.newSetFromMap(java.util.WeakHashMap())

/** Whether `line` is stretched to the full width (all but a paragraph's last line). */
private fun justified(l: Layout, line: Int): Boolean {
    if (l !in justifiedLayouts) return false
    val end = l.getLineEnd(line)
    return end < l.text.length && l.text[end - 1] != '\n'
}

/** How much wider each space on `line` is drawn than it's measured. */
private fun spaceExtra(l: Layout, line: Int): Float {
    if (!justified(l, line)) return 0f
    val start = l.getLineStart(line)
    var end = l.getLineEnd(line)
    while (end > start && l.text[end - 1].isWhitespace()) end--
    val spaces = (start until end).count { l.text[it] == ' ' }
    if (spaces == 0) return 0f
    val natural = Layout.getDesiredWidth(l.text, start, end, l.paint)
    return ((l.width - natural) / spaces).coerceAtLeast(0f)
}

/** Where offset `o` is drawn on `line` (the line's right end for its last offset). */
private fun xAt(l: Layout, line: Int, o: Int): Float {
    val start = l.getLineStart(line)
    val end = l.getLineEnd(line)
    val extra = spaceExtra(l, line)
    if (o >= end && end < l.text.length) return if (extra > 0f) l.width.toFloat() else l.getLineRight(line)
    return l.getPrimaryHorizontal(o) + extra * (start until o.coerceAtMost(end)).count { l.text[it] == ' ' }
}

/** The offset on `line` nearest to `x`. */
private fun offsetAt(l: Layout, line: Int, x: Float): Int {
    val extra = spaceExtra(l, line)
    if (extra == 0f) return l.getOffsetForHorizontal(line, x)
    val start = l.getLineStart(line)
    val end = l.getLineVisibleEnd(line)
    var best = start
    var bestD = Float.MAX_VALUE
    var spaces = 0
    for (o in start..end) {
        val d = abs(l.getPrimaryHorizontal(o) + extra * spaces - x)
        if (d < bestD) {
            bestD = d
            best = o
        }
        if (o < end && l.text[o] == ' ') spaces++
    }
    return best
}

/** The shape of offsets `s` until `e`, line by line. */
private fun rangePath(l: Layout, s: Int, e: Int, path: android.graphics.Path) {
    path.reset()
    if (s >= e) return
    for (line in l.getLineForOffset(s)..l.getLineForOffset(e - 1)) {
        val a = max(s, l.getLineStart(line))
        val b = min(e, l.getLineEnd(line))
        path.addRect(xAt(l, line, a), l.getLineTop(line).toFloat(), xAt(l, line, b), l.getLineBottom(line).toFloat(), android.graphics.Path.Direction.CW)
    }
}

private fun inWord(c: Char) = c.isLetterOrDigit() || c == '\'' || c == '’' || c == '-'

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
    private val wordPath = android.graphics.Path()
    /** How much wider spaces are drawn ("space between words"); 1 for normal. */
    private var spaceScale = 1f

    /** Reading ruler: the line it's on (paged mode), and the text lines on screen last drawn. */
    private var ruler = 0
    private val rulerLines = mutableListOf<RectF>()
    private var rulerDrag = false
    private val picturePaint = Paint(Paint.FILTER_BITMAP_FLAG)

    /** Decoded pictures at the size they're drawn, within a small memory budget. */
    private val pictures = object : android.util.LruCache<String, Bitmap>(16 * 1024 * 1024) {
        override fun sizeOf(key: String, value: Bitmap) = value.byteCount
    }

    /** Pixel size of a picture file, or null if it can't be read. */
    private fun pictureSize(path: String): Pair<Int, Int>? {
        val o = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(path, o)
        return if (o.outWidth > 0 && o.outHeight > 0) o.outWidth to o.outHeight else null
    }

    private fun picture(path: String, w: Int, h: Int): Bitmap? {
        val key = "$path@$w"
        pictures.get(key)?.let { return it }
        val size = pictureSize(path) ?: return null
        // Decode at the nearest power-of-two reduction that still covers the drawn size.
        var sample = 1
        while (size.first / (sample * 2) >= w && size.second / (sample * 2) >= h) sample *= 2
        val bmp = BitmapFactory.decodeFile(path, BitmapFactory.Options().apply { inSampleSize = sample }) ?: return null
        pictures.put(key, bmp)
        return bmp
    }

    // Swipe page turns: the page follows the finger, then settles on the next page or back.
    /** Horizontal offset of the current page while it is dragged or settling. */
    private var dragX = 0f
    private var dragging = false
    private var downX = 0f
    private var downY = 0f
    private var flingVx = 0f
    private var settle: ValueAnimator? = null
    private val slop = ViewConfiguration.get(context).scaledTouchSlop
    private val shadowPaint = Paint()
    private var shadowFor = 0

    /** Space kept clear at the top for a camera cutout. */
    var safeTop = 0

    // Selecting text: a long press picks a word, dragging (or the handles) moves its ends.
    private val spots = mutableListOf<Spot>()
    private var selA: TextPos? = null
    private var selB: TextPos? = null
    /** The finger that long-pressed is still down, extending the selection. */
    private var selecting = false
    /** A handle being dragged: 1 the start, 2 the end. */
    private var handle = 0
    private var grabDy = 0f
    /** The word first pressed: dragging selects from it. */
    private var anchorWord: Pair<TextPos, TextPos>? = null
    /** The rest of this touch is used up (it closed the selection). */
    private var swallow = false
    private val handleAt = arrayOfNulls<android.graphics.PointF>(2)

    private val margin get() = context.dp(if (width > context.dp(700)) 48 else 22).toFloat()
    private val headH get() = safeTop + context.dpf(34)
    private val footH get() = context.dpf(30)
    private val pageH get() = (height - headH - footH).coerceAtLeast(context.dpf(120))

    val pageCount get() = pages.size
    val spread get() = dual

    fun setChapter(anchor: Float) {
        clearSelection()
        key = ""
        pendingAnchor = anchor
        page = 0
        scrollPos = 0f
        requestLayoutBlocks()
    }

    fun relayout() {
        clearSelection()
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
        // Bundled easy-to-read fonts.
        "Atkinson" -> resources.getFont(R.font.atkinson)
        "OpenDyslexic" -> resources.getFont(R.font.opendyslexic)
        else -> Typeface.SANS_SERIF
    }

    private fun ensureLayout() {
        val t = Settings.typo
        val wide = width >= context.dp(840)
        val layoutMode = t.optString("page_layout")
        dual = paged && wide && layoutMode != "Single"
        val gutter = context.dpf(56)
        colW = if (dual) (width - 2 * margin - gutter) / 2 else width - 2 * margin
        val k = "${reader.chapter}|$width|$height|$paged|$dual|$t|${reader.hitsKey}|${reader.highlightsKey}"
        if (k == key) return
        key = k
        build(t)
        pendingAnchor?.let { goTo(it) }
        pendingAnchor = null
        reader.positionChanged()
    }

    private fun spannable(text: String, spans: org.json.JSONArray, shift: Int, hits: List<Hit>, para: Int, marks: Boolean = false): SpannableString {
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
        if (spaceScale > 1.01f) {
            for (i in s.indices) if (s[i] == ' ' || s[i] == '\u00A0') s.setSpan(android.text.style.ScaleXSpan(spaceScale), i, i + 1, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
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
        if (marks) {
            val raw = text.substring(shift)
            for (h in reader.chapterHighlights) {
                val st = h.getJSONArray("start")
                val en = h.getJSONArray("end")
                if (para < st.getInt(0) || para > en.getInt(0)) continue
                val a = shift + if (para == st.getInt(0)) utf16At(raw, st.getInt(1)) else 0
                val b = shift + if (para == en.getInt(0)) utf16At(raw, en.getInt(1)) else raw.length
                if (a < b) s.setSpan(BackgroundColorSpan(highlightFill(h.optInt("color"))), a, b, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
            }
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
        return b.build().also { if (justify) justifiedLayouts += it }
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
        val body = TextPaint(Paint.ANTI_ALIAS_FLAG).apply {
            typeface = tf
            textSize = size
            color = p.text
            // Extra space between letters, in ems.
            letterSpacing = t.optDouble("letter_spacing", 0.0).toFloat().coerceIn(0f, 0.5f)
        }
        // Extra space between words: wider spaces (see `spannable`), which line breaking measures.
        spaceScale = 1f + t.optDouble("word_spacing", 0.0).toFloat().coerceIn(0f, 2f) * size / body.measureText(" ").coerceAtLeast(1f)
        val small = TextPaint(body).apply { textSize = size * 0.9f }
        val w = colW.toInt()
        val pad = context.dpf(16)
        val hits = reader.hits
        val out = mutableListOf<Block>()

        if (reader.chapter == 0) out += preface(w)
        if (reader.chapterCount > 1 && !ch.optBoolean("pictures_only")) {
            val title = ch.optString("title").trim()
            if (title.isNotEmpty()) {
                val tp = TextPaint(body).apply { textSize = size * 1.45f; typeface = Typeface.create(tf, Typeface.BOLD) }
                val l = staticLayout(title, tp, w, 1.25f, Layout.Alignment.ALIGN_CENTER, false)
                out += Block("title", 0, listOf(El.Text(l, 0f, context.dpf(6), p.text)), l.height + context.dpf(30), bottoms(l, context.dpf(6)), say = title, sayEl = 0)
            }
        }

        val blocksJson = ch.getJSONArray("blocks")
        val density = resources.displayMetrics.density
        // Pictures are never taller than a page (or, when scrolling, a tall screenful).
        val maxPictureH = if (paged) pageH - context.dpf(8) else colW * 1.4f
        var tableCols = 1
        for (i in 0 until blocksJson.length()) {
            val b = blocksJson.getJSONObject(i)
            val kind = b.getString("kind")
            val para = b.getInt("para")
            val text = b.getString("text")
            val nextKind = blocksJson.optJSONObject(i + 1)?.optString("kind")
            when (kind) {
                "heading" -> {
                    val level = b.optInt("level", 2).coerceIn(1, 6)
                    val scale = floatArrayOf(1.4f, 1.22f, 1.08f)[min(level - 1, 2)]
                    val hp = TextPaint(body).apply { textSize = size * scale; typeface = Typeface.create(tf, Typeface.BOLD) }
                    val first = out.lastOrNull()?.kind !in BODY
                    val top = if (first) 0f else gap * 0.6f + size * 0.6f
                    val tl = staticLayout(spannable(text, b.getJSONArray("spans"), 0, hits, para, marks = true), hp, w, 1.3f, Layout.Alignment.ALIGN_NORMAL, false)
                    out += Block(kind, para, listOf(El.Text(tl, 0f, top, p.text)), top + tl.height + gap * 0.7f, bottoms(tl, top), say = text, sayEl = 0)
                }
                "item" -> {
                    val depth = b.optInt("depth", 1).coerceIn(1, 8)
                    val left = context.dpf(24) * depth
                    val tl = staticLayout(spannable(text, b.getJSONArray("spans"), 0, hits, para, marks = true), body, (w - left).toInt(), mult, Layout.Alignment.ALIGN_NORMAL, false)
                    val mp = TextPaint(body).apply { color = p.muted }
                    val marker = staticLayout(b.optString("marker", "•"), mp, left.toInt(), mult, Layout.Alignment.ALIGN_OPPOSITE, false)
                    val more = nextKind == "item"
                    out += Block(kind, para, listOf(
                        El.Text(marker, -context.dpf(8), 0f, p.muted),
                        El.Text(tl, left, 0f, p.text),
                    ), tl.height + if (more) gap * 0.35f else gap, bottoms(tl, 0f), say = text, sayEl = 1)
                }
                "pre" -> {
                    // Columns in preformatted text stay lined up: no extra spacing.
                    val mono = TextPaint(body).apply { typeface = Typeface.MONOSPACE; textSize = size * 0.82f; letterSpacing = 0f }
                    val inset = context.dpf(14)
                    val tl = staticLayout(text, mono, (w - 2 * inset).toInt(), 1.4f, Layout.Alignment.ALIGN_NORMAL, false)
                    val h = tl.height + 2 * context.dpf(10)
                    out += Block(kind, para, listOf(
                        El.Box(RectF(0f, 0f, w.toFloat(), h), context.dpf(6), p.quoteBg, p.border, context.dpf(1)),
                        El.Text(tl, inset, context.dpf(10), p.text),
                    ), h + gap, bottoms(tl, context.dpf(10)))
                }
                "center" -> {
                    val tl = staticLayout(spannable(text, b.getJSONArray("spans"), 0, hits, para, marks = true), body, w, mult, Layout.Alignment.ALIGN_CENTER, false)
                    out += Block(kind, para, listOf(El.Text(tl, 0f, 0f, p.text)), tl.height + gap, bottoms(tl, 0f), say = text, sayEl = 0)
                }
                "image" -> {
                    val path = b.optString("path").takeIf { !b.isNull("path") && it.isNotEmpty() }
                    val size2 = path?.let { pictureSize(it) }
                    if (path != null && size2 != null) {
                        // Small pictures keep their size; large ones shrink to the column and page.
                        var dw = min(size2.first * density, w.toFloat())
                        var dh = size2.second * dw / size2.first
                        if (dh > maxPictureH) {
                            dw *= maxPictureH / dh
                            dh = maxPictureH
                        }
                        val x0 = (w - dw) / 2
                        out += Block(kind, para, listOf(El.Picture(path, RectF(x0, 0f, x0 + dw, dh))), dh + gap, FloatArray(0))
                    } else if (text.isNotEmpty()) {
                        val mp = TextPaint(body).apply { color = p.muted }
                        val tl = staticLayout("[Picture: $text]", mp, w, mult, Layout.Alignment.ALIGN_CENTER, false)
                        out += Block(kind, para, listOf(El.Text(tl, 0f, 0f, p.muted)), tl.height + gap, bottoms(tl, 0f))
                    }
                }
                "row" -> {
                    // Columns are shared by the whole table: as many as its widest row.
                    if (out.lastOrNull()?.kind != "row") {
                        tableCols = 1
                        var j = i
                        while (j < blocksJson.length() && blocksJson.getJSONObject(j).getString("kind") == "row") {
                            tableCols = maxOf(tableCols, blocksJson.getJSONObject(j).getJSONArray("cells").length())
                            j++
                        }
                    }
                    val header = b.optBoolean("header")
                    val cells = b.getJSONArray("cells")
                    val colWidth = w.toFloat() / tableCols
                    val cp = TextPaint(small).apply { if (header) typeface = Typeface.create(tf, Typeface.BOLD) }
                    val padX = context.dpf(8)
                    val padY = context.dpf(7)
                    val layouts = (0 until cells.length()).map { ci ->
                        val c = cells.getJSONObject(ci)
                        staticLayout(spannable(c.getString("text"), c.getJSONArray("spans"), 0, emptyList(), para), cp, (colWidth - 2 * padX).toInt(), 1.3f, Layout.Alignment.ALIGN_NORMAL, false)
                    }
                    val rowH = (layouts.maxOfOrNull { it.height } ?: 0) + 2 * padY
                    val line = context.dpf(1)
                    val els = mutableListOf<El>()
                    if (header) els += El.Box(RectF(0f, 0f, w.toFloat(), rowH), 0f, alpha(p.text, 0.07f))
                    if (out.lastOrNull()?.kind != "row") els += El.Box(RectF(0f, 0f, w.toFloat(), line), 0f, p.border)
                    els += El.Box(RectF(0f, rowH - line, w.toFloat(), rowH), 0f, p.border)
                    for (ci in 0..tableCols) els += El.Box(RectF(min(ci * colWidth, w - line), 0f, min(ci * colWidth, w - line) + line, rowH), 0f, p.border)
                    layouts.forEachIndexed { ci, l -> els += El.Text(l, ci * colWidth + padX, padY, p.text) }
                    out += Block(kind, para, els, rowH + if (nextKind != "row") gap else 0f, FloatArray(0))
                }
                "summary", "notes", "endnotes" -> {
                    if (kind == "summary" && !t.optBoolean("show_chapter_summary", true)) continue
                    if (kind == "notes" && !t.optBoolean("show_chapter_pre_notes", true)) continue
                    if (kind == "endnotes" && !t.optBoolean("show_chapter_post_notes", true)) continue
                    val top = if (kind == "endnotes") context.dpf(12) else 0f
                    val labelText = mapOf("summary" to "Chapter summary", "notes" to "Notes", "endnotes" to "End notes")[kind]!!
                    val lp = TextPaint(Paint.ANTI_ALIAS_FLAG).apply { textSize = 12.5f * sd; color = p.accentText; typeface = Typeface.DEFAULT_BOLD }
                    val label = staticLayout(labelText, lp, w, 1.2f, Layout.Alignment.ALIGN_NORMAL, false)
                    val tl = staticLayout(spannable(text, b.getJSONArray("spans"), 0, emptyList(), para), small, (w - 2 * pad).toInt(), min(mult, 1.7f), Layout.Alignment.ALIGN_NORMAL, false)
                    val textY = top + pad + label.height + context.dpf(6)
                    val h = textY + tl.height + pad
                    out += Block(kind, para, listOf(
                        El.Box(RectF(0f, top, w.toFloat(), h), context.dpf(10), p.notesBg, p.border, context.dpf(1)),
                        El.Text(label, pad, top + pad, p.accentText),
                        El.Text(tl, pad, textY, alpha(p.text, 0.9f)),
                    ), h + context.dpf(18), bottoms(tl, textY), say = text, sayEl = 2)
                }
                "divider" -> {
                    val dp = TextPaint(body).apply { color = p.muted }
                    val l = staticLayout("⁂", dp, w, 1f, Layout.Alignment.ALIGN_CENTER, false)
                    out += Block(kind, para, listOf(El.Text(l, 0f, size * 0.6f, p.muted)), size * 2.4f + gap, FloatArray(0))
                }
                "quote" -> {
                    val inset = context.dpf(20)
                    val tl = staticLayout(spannable(text, b.getJSONArray("spans"), 0, hits, para, marks = true), body, (w - inset - context.dpf(12)).toInt(), mult, Layout.Alignment.ALIGN_NORMAL, justify)
                    val h = tl.height + context.dpf(20)
                    out += Block(kind, para, listOf(
                        El.Box(RectF(0f, 0f, w.toFloat(), h), context.dpf(6), p.quoteBg),
                        El.Box(RectF(0f, 0f, context.dpf(3), h), context.dpf(1.5f), alpha(p.text, 0.22f)),
                        El.Text(tl, inset, context.dpf(10), p.text),
                    ), h + gap, bottoms(tl, context.dpf(10)), say = text, sayEl = 2)
                }
                else -> {
                    val prefix = if (indent) "  " else ""
                    val tl = staticLayout(spannable(prefix + text, b.getJSONArray("spans"), prefix.length, hits, para, marks = true), body, w, mult, Layout.Alignment.ALIGN_NORMAL, justify)
                    out += Block("para", para, listOf(El.Text(tl, 0f, 0f, p.text)), tl.height + gap, bottoms(tl, 0f), say = text, sayEl = 0, shift = prefix.length)
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
        // AO3's markers (rating, category, warnings) as a coloured dot on a quiet pill. With
        // `ships`, relationships: romantic ones ("A/B") with a heart on a rose tint, platonic
        // ones ("A & B") with a person sign on an outline.
        fun pills(items: List<Pair<String, Int?>>, ships: Boolean = false) {
            var x = 0f
            val h = context.dpf(24)
            val dot = context.dpf(7)
            val ic = context.dpf(12)
            for ((label, marker) in items) {
                val love = ships && '/' in label
                val tp = TextPaint(Paint.ANTI_ALIAS_FLAG).apply { textSize = 12.5f * sd; color = p.text; typeface = Settings.face(love) }
                val lead = if (marker != null) context.dpf(22) else if (ships) context.dpf(26) else context.dpf(10)
                val l = staticLayout(label, tp, (w - context.dp(36)), 1f, Layout.Alignment.ALIGN_NORMAL, false)
                val pw = l.getLineWidth(0) + lead + context.dpf(10)
                if (x > 0 && x + pw > w) { x = 0f; y += h + context.dpf(6) }
                val r = RectF(x, y, x + pw, y + h)
                when {
                    marker != null -> {
                        els += El.Box(r, h / 2, alpha(p.text, 0.07f))
                        val cx = x + context.dpf(12)
                        els += El.Box(RectF(cx - dot / 2, y + (h - dot) / 2, cx + dot / 2, y + (h + dot) / 2), dot / 2, marker)
                    }
                    love -> els += El.Box(r, h / 2, alpha(p.romance, 0.14f), alpha(p.romance, 0.45f), context.dpf(1))
                    else -> els += El.Box(r, h / 2, 0, p.border, context.dpf(1))
                }
                if (ships) {
                    val d = context.getDrawable(if (love) R.drawable.ic_heart_fill else R.drawable.ic_person)!!.mutate()
                    d.setTint(if (love) p.romance else p.muted)
                    els += El.Icon(d, RectF(x + context.dpf(9), y + (h - ic) / 2, x + context.dpf(9) + ic, y + (h + ic) / 2))
                }
                els += El.Text(l, x + lead, y + (h - l.height) / 2, p.text)
                x += pw + context.dpf(6)
            }
            if (items.isNotEmpty()) y += h
        }
        text(m.getString("title"), 24f, p.text, bold = true)
        y += context.dpf(4)
        text("by " + m.getJSONArray("authors").strings().joinToString(", "), 14f, p.muted)
        y += context.dpf(12)
        val badges = mutableListOf<Pair<String, Int?>>()
        // Books from files have no AO3 rating.
        if (m.optBoolean("ao3", true)) badges += m.getString("rating") to Pal.opaque(m.getInt("rating_color"))
        m.getJSONArray("categories").objects().forEach { badges += it.getString("name") to Pal.opaque(it.getInt("color")) }
        m.getJSONArray("warnings").strings().forEach { badges += it to if ("No Archive Warnings" in it) p.success else WARNING_RED }
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
            pills(rels.map { it to null }, ships = true)
            y += context.dpf(8)
        }
        tagLine("Characters", "characters")
        tagLine("Tags", "tags")
        val summary = m.getString("summary").trim()
        if (summary.isNotEmpty()) {
            y += context.dpf(6)
            val pad = context.dpf(14)
            val lp = TextPaint(Paint.ANTI_ALIAS_FLAG).apply { textSize = 12.5f * sd; color = p.accentText; typeface = Typeface.DEFAULT_BOLD }
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
            els += El.Text(label, pad, y + pad, p.accentText)
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
            in BODY -> b.para + ((y - b.y) / max(b.h, 1f)).coerceIn(0f, 0.999f)
            "endnotes", "end" -> paraCount.toFloat()
            else -> 0f
        }
    }

    private fun yFor(anchor: Float): Float {
        if (anchor <= 0f) return 0f
        val idx = anchor.toInt()
        if (idx >= paraCount) return blocks.firstOrNull { it.kind == "endnotes" || it.kind == "end" }?.y ?: totalH
        val b = blocks.firstOrNull { it.para == idx && it.kind in BODY } ?: return 0f
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

    fun turn(forward: Boolean, byUser: Boolean = true) {
        clearSelection()
        val step = if (dual) 2 else 1
        ruler = 0
        if (forward) {
            if (page + step < pages.size) page += step else { reader.nextChapter(); if (byUser) reader.userMoved(); return }
        } else {
            if (page > 0) page = max(0, page - step) else { reader.prevChapter(toEnd = true); if (byUser) reader.userMoved(); return }
        }
        invalidate()
        reader.positionChanged()
        if (byUser) reader.userMoved()
    }

    private fun scrollBy(dy: Float) {
        val maxScroll = max(0f, totalH + headH - height)
        scrollPos = (scrollPos + dy).coerceIn(0f, maxScroll)
        invalidate()
        reader.positionChanged()
        reader.userMoved()
    }

    // ---- read aloud and screen readers --------------------------------------------------

    /** Blocks on screen now, in order. */
    private fun visibleBlocks(): List<Int> =
        if (paged) (page until min(page + step, pages.size)).flatMap { pages[it].slices.map { s -> s.block } }.distinct()
        else blocks.indices.filter { val top = headH - scrollPos + blocks[it].y; top + blocks[it].h > 0 && top < height }

    fun firstSpeakable(): Int? = visibleBlocks().firstOrNull { blocks[it].say.isNotEmpty() }
    fun nextSpeakable(after: Int): Int? = (after + 1 until blocks.size).firstOrNull { blocks[it].say.isNotEmpty() }
    fun speakText(i: Int): String? = blocks.getOrNull(i)?.say?.takeIf { it.isNotEmpty() }
    fun showing(i: Int): Boolean = i in visibleBlocks()

    /** The text on screen, for TalkBack. */
    fun visibleText(): String = visibleBlocks().mapNotNull { blocks[it].say.takeIf(String::isNotEmpty) }.joinToString("\n")

    /** Keeps the word being read aloud on screen: turns the page, or scrolls. */
    fun followAloud() {
        val b = blocks.getOrNull(reader.aloudBlock) ?: return
        val el = b.els.getOrNull(b.sayEl) as? El.Text ?: return
        val w = reader.aloudWord
        val line = if (w != null) el.layout.getLineForOffset((w.first + b.shift).coerceIn(0, el.layout.text.length)) else 0
        val y = b.y + el.y + el.layout.getLineBottom(line)
        if (paged) {
            val last = min(page + step - 1, pages.size - 1)
            if (last + 1 < pages.size && y > pages[last + 1].y + 1f) turn(true, byUser = false)
        } else {
            val onScreen = headH - scrollPos + y
            if (onScreen > height * 0.8f || onScreen < headH) {
                scrollPos = (y - height * 0.35f).coerceIn(0f, max(0f, totalH + headH - height))
                invalidate()
                reader.positionChanged()
            }
        }
    }

    override fun onInitializeAccessibilityNodeInfo(info: android.view.accessibility.AccessibilityNodeInfo) {
        super.onInitializeAccessibilityNodeInfo(info)
        // TalkBack's scroll gestures turn pages.
        info.isScrollable = true
        info.addAction(android.view.accessibility.AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_FORWARD)
        info.addAction(android.view.accessibility.AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_BACKWARD)
    }

    override fun performAccessibilityAction(action: Int, args: android.os.Bundle?): Boolean = when (action) {
        android.view.accessibility.AccessibilityNodeInfo.ACTION_SCROLL_FORWARD -> { if (paged) turn(true) else scrollBy(height * 0.8f); true }
        android.view.accessibility.AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD -> { if (paged) turn(false) else scrollBy(-height * 0.8f); true }
        else -> super.performAccessibilityAction(action, args)
    }

    private val gestures = GestureDetector(context, object : GestureDetector.SimpleOnGestureListener() {
        override fun onDown(e: MotionEvent): Boolean {
            if (!scroller.isFinished) scroller.forceFinished(true)
            // A new touch finishes a page turn that is still settling.
            settle?.end()
            return true
        }

        override fun onLongPress(e: MotionEvent) {
            if (rulerDrag || dragging) return
            val pos = hitText(e.x, e.y, exact = true) ?: return
            val (a, b) = wordAt(pos) ?: return
            selA = a
            selB = b
            anchorWord = a to b
            selecting = true
            performHapticFeedback(android.view.HapticFeedbackConstants.LONG_PRESS)
            parent?.requestDisallowInterceptTouchEvent(true)
            invalidate()
        }

        override fun onSingleTapUp(e: MotionEvent): Boolean {
            if (rulerDrag) return true
            val tapped = hitText(e.x, e.y, exact = true)?.let { pos -> highlightAt(pos)?.let { pos to it } }
            if (tapped != null) {
                posRect(tapped.first)?.let { r -> reader.showSelectionBar(r.left, r.top, r.bottom, tapped.second) }
                return true
            }
            val x = e.x / width
            if (!paged) {
                val endBlock = blocks.lastOrNull()
                if (endBlock != null && e.y + scrollPos - headH >= endBlock.y) { reader.nextChapter(); return true }
                reader.toggleChrome()
                return true
            }
            val y = e.y / height
            // Bands along the top and bottom (where the bars appear) and a wide middle show the
            // bars; the strips down the sides turn pages.
            when {
                y < 0.2f || y > 0.8f -> reader.toggleChrome()
                x < 0.17f -> turn(false)
                x > 0.83f -> turn(true)
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
                // Acted on when the finger lifts (see release).
                if (abs(vx) > abs(vy)) flingVx = vx
            } else {
                val maxScroll = max(0f, totalH + headH - height).toInt()
                scroller.fling(0, scrollPos.toInt(), 0, -vy.toInt(), 0, 0, 0, maxScroll)
                postInvalidateOnAnimation()
            }
            return true
        }
    })

    override fun onTouchEvent(event: MotionEvent): Boolean {
        if (selectionTouch(event)) return true
        if (paged && event.actionMasked == MotionEvent.ACTION_DOWN) {
            downX = event.x
            downY = event.y
            dragging = false
            flingVx = 0f
        }
        val handled = gestures.onTouchEvent(event)
        if (!paged) return handled || super.onTouchEvent(event)
        when (event.actionMasked) {
            MotionEvent.ACTION_MOVE -> {
                val dx = event.x - downX
                // With the reading ruler on, dragging up or down moves it.
                if (Settings.typo.optBoolean("reading_ruler") && !dragging && (rulerDrag || (abs(event.y - downY) > slop && abs(event.y - downY) > abs(dx)))) {
                    rulerDrag = true
                    rulerLines.indexOfFirst { event.y < it.bottom + context.dpf(4) }.takeIf { it >= 0 }?.let { ruler = it; invalidate() }
                    return true
                }
                if (!dragging && abs(dx) > slop && abs(dx) > abs(event.y - downY)) {
                    dragging = true
                    parent?.requestDisallowInterceptTouchEvent(true)
                }
                if (dragging && Anim.on) {
                    // Resist when there is nothing to turn to in that direction.
                    dragX = if (canTurn(dx < 0)) dx else dx * 0.3f
                    invalidate()
                }
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> if (dragging) {
                dragging = false
                release(event.x - downX, event.actionMasked == MotionEvent.ACTION_UP)
            } else rulerDrag = false
        }
        return true
    }

    private val step get() = if (dual) 2 else 1

    /** Whether a turn in that direction leads anywhere (another page or chapter). */
    private fun canTurn(forward: Boolean): Boolean =
        if (forward) page + step < pages.size || reader.chapter + 1 < reader.chapterCount else page > 0 || reader.chapter > 0

    private fun release(dx: Float, up: Boolean) {
        val w = width.toFloat().coerceAtLeast(1f)
        val fast = abs(flingVx) > context.dpf(300)
        val forward = if (fast) flingVx < 0 else dx < 0
        val commit = up && canTurn(forward) && ((fast && (dx < 0) == forward) || abs(dx) > w * 0.22f)
        if (!Anim.on) {
            dragX = 0f
            if (commit) turn(forward)
            return
        }
        if (!commit) return animateDrag(0f, 170) {}
        val within = if (forward) page + step < pages.size else page > 0
        val ms = (230 * (1f - abs(dragX) / w)).toLong().coerceIn(90, 230)
        if (within) {
            // The neighbouring page is already drawn beside this one: slide it into place.
            animateDrag(if (forward) -w else w, ms) {
                dragX = 0f
                turn(forward)
            }
        } else {
            // Next or previous chapter: slide this page away, then the new chapter in.
            animateDrag(if (forward) -w else w, (ms * 0.7f).toLong()) {
                dragX = if (forward) w else -w
                turn(forward)
                animateDrag(0f, 200) {}
            }
        }
    }

    private fun animateDrag(to: Float, ms: Long, end: () -> Unit) {
        settle = ValueAnimator.ofFloat(dragX, to).apply {
            duration = ms
            interpolator = Anim.decel
            addUpdateListener { dragX = it.animatedValue as Float; invalidate() }
            addListener(object : AnimatorListenerAdapter() {
                override fun onAnimationEnd(animation: Animator) {
                    if (settle === animation) settle = null
                    dragX = to
                    end()
                    invalidate()
                }
            })
            start()
        }
    }

    override fun computeScroll() {
        if (!paged && scroller.computeScrollOffset()) {
            scrollPos = scroller.currY.toFloat()
            reader.positionChanged()
            postInvalidateOnAnimation()
        }
    }

    // ---- selecting and highlights -----------------------------------------------------

    private fun textOf(bi: Int): StaticLayout? = blocks.getOrNull(bi)?.let { it.els.getOrNull(it.sayEl) as? El.Text }?.layout

    /** The selection, start first. */
    private fun selRange(): Pair<TextPos, TextPos>? {
        val a = selA ?: return null
        val b = selB ?: return null
        if (a == b) return null
        return if (a < b) a to b else b to a
    }

    /** The text position under (x, y): on text exactly (`exact`), or on the nearest line. */
    private fun hitText(x: Float, y: Float, exact: Boolean): TextPos? {
        val spot = spots.firstOrNull { s ->
            val l = textOf(s.block)
            s.clip.contains(x, y) && l != null && (!exact || (y - s.oy <= l.height && x >= s.ox - context.dpf(4) && x <= s.ox + l.width + context.dpf(4)))
        } ?: if (exact) return null else spots.minByOrNull { s ->
            abs(y - y.coerceIn(s.clip.top, s.clip.bottom)) + abs(x - x.coerceIn(s.clip.left, s.clip.right)) * 0.25f
        } ?: return null
        val l = textOf(spot.block) ?: return null
        val ly = (y.coerceIn(spot.clip.top, spot.clip.bottom - 1f) - spot.oy).coerceIn(0f, l.height - 1f)
        val line = l.getLineForVertical(ly.toInt())
        return TextPos(spot.block, offsetAt(l, line, x - spot.ox).coerceAtLeast(blocks[spot.block].shift))
    }

    private fun wordAt(p: TextPos): Pair<TextPos, TextPos>? {
        val t = textOf(p.block)?.text ?: return null
        var s = p.at.coerceIn(0, t.length)
        var e = s
        while (s > 0 && inWord(t[s - 1])) s--
        while (e < t.length && inWord(t[e])) e++
        return if (s < e) TextPos(p.block, s) to TextPos(p.block, e) else null
    }

    /** `p` moved to the start (`end`: the end) of the word it's inside, without the spaces around. */
    private fun snap(p: TextPos, end: Boolean): TextPos {
        val t = textOf(p.block)?.text ?: return p
        var at = p.at.coerceIn(0, t.length)
        if (end) {
            while (at > 0 && t[at - 1].isWhitespace()) at--
            if (at > 0 && inWord(t[at - 1])) while (at < t.length && inWord(t[at])) at++
        } else {
            while (at < t.length && t[at].isWhitespace()) at++
            if (at < t.length && inWord(t[at])) while (at > 0 && inWord(t[at - 1])) at--
        }
        return TextPos(p.block, at.coerceAtLeast(blocks[p.block].shift))
    }

    /** Where `p` is on screen: a zero-width rectangle as tall as its line (for the `end` of a
     *  range, the line it ends, not the next one). */
    private fun posRect(p: TextPos, end: Boolean = false): RectF? {
        val l = textOf(p.block) ?: return null
        val at = p.at.coerceIn(0, l.text.length)
        val line = l.getLineForOffset(if (end && at > 0) at - 1 else at)
        for (s in spots) {
            if (s.block != p.block) continue
            val top = s.oy + l.getLineTop(line)
            val bottom = s.oy + l.getLineBottom(line)
            if (bottom > s.clip.top + 1 && top < s.clip.bottom - 1) {
                val x = s.ox + xAt(l, line, at)
                return RectF(x, top, x, bottom)
            }
        }
        return null
    }

    /** The highlight at `p`, if there is one. */
    private fun highlightAt(p: TextPos): JSONObject? {
        val blk = blocks.getOrNull(p.block) ?: return null
        val at = utf8Len(blk.say, p.at - blk.shift)
        return reader.chapterHighlights.firstOrNull { h ->
            val st = h.getJSONArray("start")
            val en = h.getJSONArray("end")
            val afterStart = blk.para > st.getInt(0) || (blk.para == st.getInt(0) && at >= st.getInt(1))
            val beforeEnd = blk.para < en.getInt(0) || (blk.para == en.getInt(0) && at <= en.getInt(1))
            afterStart && beforeEnd
        }
    }

    fun selectedText(): String {
        val (a, b) = selRange() ?: return ""
        return (a.block..b.block).mapNotNull { bi ->
            val blk = blocks[bi]
            val t = textOf(bi)?.text?.takeIf { blk.kind in SELECTABLE } ?: return@mapNotNull null
            val s = if (bi == a.block) a.at else blk.shift
            val e = if (bi == b.block) b.at else t.length
            if (s < e) t.subSequence(s.coerceIn(0, t.length), e.coerceIn(0, t.length)).toString().trim() else null
        }.joinToString("\n")
    }

    /** The selection as a highlight for the core (not saved yet). */
    fun selectionHighlight(color: Int): JSONObject? {
        val (a, b) = selRange() ?: return null
        fun end(p: TextPos): org.json.JSONArray {
            val blk = blocks[p.block]
            return org.json.JSONArray(listOf(blk.para, utf8Len(blk.say, p.at - blk.shift)))
        }
        return JSONObject().put("chapter", reader.chapter).put("start", end(a)).put("end", end(b)).put("color", color).put("note", "").put("text", selectedText())
    }

    fun clearSelection() {
        val had = selA != null
        selA = null
        selB = null
        selecting = false
        handle = 0
        reader.hideSelectionBar()
        if (had) invalidate()
    }

    /** Selection bar for the selection, above it (or below, near the top of the screen). */
    private fun showBar() {
        val (a, b) = selRange() ?: return
        val r0 = posRect(a) ?: return
        val r1 = posRect(b, end = true) ?: r0
        reader.showSelectionBar(r0.left, r0.top, r1.bottom + context.dpf(22), null)
    }

    /** Touches that select: a long press then dragging, the handles, and closing a selection. */
    private fun selectionTouch(e: MotionEvent): Boolean {
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                swallow = false
                if (selRange() != null) {
                    val near = context.dpf(28)
                    val i = handleAt.indexOfFirst { it != null && abs(it.x - e.x) < near && abs(it.y - e.y) < near }
                    if (i >= 0) {
                        handle = i + 1
                        val (a, b) = selRange()!!
                        val r = if (i == 0) posRect(a) else posRect(b, end = true)
                        grabDy = if (r != null) e.y - r.centerY() else 0f
                        reader.hideSelectionBar()
                        parent?.requestDisallowInterceptTouchEvent(true)
                        return true
                    }
                }
                if (selRange() != null || reader.selectionBarShown) {
                    clearSelection()
                    swallow = true
                    return true
                }
            }
            MotionEvent.ACTION_MOVE -> {
                if (handle != 0) {
                    val pos = hitText(e.x, e.y - grabDy, exact = false) ?: return true
                    val (a, b) = selRange() ?: return true
                    if (handle == 1) { selA = pos; selB = b } else { selA = a; selB = pos }
                    if (selA == selB) selB = TextPos(pos.block, pos.at + 1)
                    invalidate()
                    return true
                }
                if (selecting) {
                    val pos = hitText(e.x, e.y, exact = false) ?: return true
                    val (wa, wb) = anchorWord ?: return true
                    // From the word first pressed, to the word under the finger.
                    val word = wordAt(pos)
                    if (pos < wa) { selA = wb; selB = word?.first ?: pos } else { selA = wa; selB = word?.second ?: pos }
                    invalidate()
                    return true
                }
                return swallow
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                if (handle != 0 || selecting) {
                    handle = 0
                    selecting = false
                    selRange()?.let { (a, b) ->
                        selA = snap(a, end = false)
                        selB = snap(b, end = true)
                    }
                    if (selRange() == null) clearSelection() else { invalidate(); post { showBar() } }
                    return true
                }
                if (swallow) {
                    swallow = false
                    return true
                }
            }
        }
        return false
    }

    /** The selection's two handles, under its ends. */
    private fun drawHandles(c: Canvas) {
        handleAt[0] = null
        handleAt[1] = null
        val (a, b) = selRange() ?: return
        val r = context.dpf(8)
        boxPaint.style = Paint.Style.FILL
        boxPaint.color = Settings.pal.accentMark
        listOf(a, b).forEachIndexed { i, pos ->
            val rect = posRect(pos, end = i == 1) ?: return@forEachIndexed
            val cx = rect.left
            val cy = rect.bottom + r
            c.drawRect(cx - context.dpf(1), rect.top, cx + context.dpf(1), cy, boxPaint)
            c.drawCircle(cx, cy, r, boxPaint)
            handleAt[i] = android.graphics.PointF(cx, cy)
        }
    }

    // ---- drawing ------------------------------------------------------------------------

    private fun drawBlock(c: Canvas, b: Block, x: Float, y: Float, bi: Int = -1, clip: RectF? = null) {
        if (clip != null && bi >= 0 && b.kind in SELECTABLE) (b.els.getOrNull(b.sayEl) as? El.Text)?.let { spots += Spot(bi, clip, x + it.x, y + it.y) }
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
            is El.Icon -> {
                el.d.setBounds(el.r.left.toInt(), el.r.top.toInt(), el.r.right.toInt(), el.r.bottom.toInt())
                el.d.draw(c)
            }
            is El.Picture -> {
                val bmp = picture(el.path, el.r.width().toInt(), el.r.height().toInt())
                if (bmp != null) c.drawBitmap(bmp, null, el.r, picturePaint)
                else {
                    boxPaint.style = Paint.Style.STROKE
                    boxPaint.strokeWidth = context.dpf(1)
                    boxPaint.color = Settings.pal.border
                    c.drawRoundRect(el.r, context.dpf(6), context.dpf(6), boxPaint)
                }
            }
            is El.Text -> {
                c.save()
                c.translate(el.x, el.y)
                val w = reader.aloudWord
                if (w != null && b === blocks.getOrNull(reader.aloudBlock) && el === b.els.getOrNull(b.sayEl)) {
                    val n = el.layout.text.length
                    rangePath(el.layout, (w.first + b.shift).coerceIn(0, n), (w.last + 1 + b.shift).coerceIn(0, n), wordPath)
                    boxPaint.style = Paint.Style.FILL
                    boxPaint.color = alpha(Settings.pal.accentMark, 0.3f)
                    c.drawPath(wordPath, boxPaint)
                }
                val sel = selRange()
                if (sel != null && bi in sel.first.block..sel.second.block && el === b.els.getOrNull(b.sayEl)) {
                    val n = el.layout.text.length
                    val s = if (bi == sel.first.block) sel.first.at else b.shift
                    val e = if (bi == sel.second.block) sel.second.at else n
                    if (s < e) {
                        rangePath(el.layout, s.coerceIn(0, n), e.coerceIn(0, n), wordPath)
                        boxPaint.style = Paint.Style.FILL
                        boxPaint.color = alpha(Settings.pal.accentMark, 0.32f)
                        c.drawPath(wordPath, boxPaint)
                    }
                }
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
        spots.clear()
        if (blocks.isEmpty()) return
        headPaint.textSize = context.dpf(12)
        headPaint.color = alpha(p.muted, 0.85f)
        headPaint.textAlign = Paint.Align.CENTER
        if (paged) {
            val w = width.toFloat()
            drawSpread(c, page, dragX)
            if (dragX < 0 && page + step < pages.size) drawSpread(c, page + step, dragX + w)
            if (dragX > 0 && page > 0) drawSpread(c, max(0, page - step), dragX - w)
            if (dragX != 0f) {
                // A soft shadow along the seam between the two pages.
                val seam = if (dragX < 0) w + dragX else dragX
                val sw = context.dpf(14)
                val dark = alpha(0xFF000000.toInt(), if (p.dark) 0.35f else 0.16f)
                // One gradient per theme, moved into place, instead of a new one every frame.
                if (shadowFor != dark) {
                    shadowFor = dark
                    shadowPaint.shader = LinearGradient(0f, 0f, sw, 0f, dark, 0, Shader.TileMode.CLAMP)
                }
                c.save()
                c.translate(seam, 0f)
                c.drawRect(0f, 0f, sw, height.toFloat(), shadowPaint)
                c.restore()
            }
        } else {
            val x = (width - colW) / 2
            val top = headH - scrollPos
            rulerLines.clear()
            for ((bi, b) in blocks.withIndex()) {
                if (top + b.y + b.h < 0 || top + b.y > height) continue
                val clip = RectF(x - context.dpf(8), max(top + b.y, 0f), x + colW + context.dpf(8), min(top + b.y + b.h, height.toFloat()))
                drawBlock(c, b, x, top + b.y, bi, clip)
                lineRects(b, x, top + b.y, 0f, height.toFloat())
            }
        }
        headPaint.textAlign = Paint.Align.LEFT
        val t = Settings.typo
        // A soft colour over the page, then the reading ruler.
        if (t.has("tint") && !t.isNull("tint")) c.drawColor(alpha(Pal.opaque(t.optInt("tint")), t.optDouble("tint_strength", 0.18).toFloat().coerceIn(0f, 0.5f)))
        if (t.optBoolean("reading_ruler") && dragX == 0f) drawRuler(c)
        if (dragX == 0f) drawHandles(c)
    }

    /** Screen rectangles of block `b`'s text lines (drawn at `x`, `y`) within `top`..`bottom`. */
    private fun lineRects(b: Block, x: Float, y: Float, top: Float, bottom: Float) {
        var prev = -1f
        for ((i, bot) in b.lines.withIndex()) {
            val lineTop = if (i > 0) prev else bot - (if (b.lines.size > 1) b.lines[1] - b.lines[0] else bot)
            prev = bot
            if (y + bot > top + 1 && y + lineTop < bottom - 1) rulerLines += RectF(x - context.dpf(6), y + lineTop, x + colW + context.dpf(6), y + bot)
        }
    }

    /** Highlights one line and dims the rest. Scrolling keeps the band still and moves the text. */
    private fun drawRuler(c: Canvas) {
        if (rulerLines.isEmpty()) return
        val idx = if (paged) ruler.coerceIn(0, rulerLines.size - 1)
        else rulerLines.indices.minByOrNull { abs(rulerLines[it].centerY() - height * 0.38f) } ?: 0
        ruler = idx
        val l = rulerLines[idx]
        val band = RectF(l.left, l.top - context.dpf(3), l.right, l.bottom + context.dpf(3))
        boxPaint.style = Paint.Style.FILL
        boxPaint.color = alpha(Settings.pal.bg, 0.62f)
        c.drawRect(0f, 0f, width.toFloat(), band.top, boxPaint)
        c.drawRect(0f, band.bottom, width.toFloat(), height.toFloat(), boxPaint)
        c.drawRect(0f, band.top, band.left, band.bottom, boxPaint)
        c.drawRect(band.right, band.top, width.toFloat(), band.bottom, boxPaint)
        boxPaint.color = Settings.pal.accentMark
        c.drawRoundRect(RectF(band.left - context.dpf(2), band.top + context.dpf(2), band.left + context.dpf(1), band.bottom - context.dpf(2)), context.dpf(1.5f), context.dpf(1.5f), boxPaint)
    }

    /** One page (or two-page spread) starting at `first`, shifted right by `dx`. */
    private fun drawSpread(c: Canvas, first: Int, dx: Float) {
        val p = Settings.pal
        val gutter = context.dpf(56)
        val total = if (dual) colW * 2 + gutter else colW
        val x0 = (width - total) / 2 + dx
        if (dx != 0f) {
            boxPaint.style = Paint.Style.FILL
            boxPaint.color = p.bg
            c.drawRect(dx, 0f, dx + width, height.toFloat(), boxPaint)
        }
        // Focus mode: no running heads or page numbers, just the text.
        val focus = Settings.typo.optBoolean("focus_mode")
        if (first == page) rulerLines.clear()
        for (k in 0 until if (dual) 2 else 1) {
            val pi = first + k
            if (pi >= pages.size) break
            val px = x0 + k * (colW + gutter)
            val head = if (dual && k == 0) reader.meta.getString("title") else reader.chapterJson?.optString("title") ?: ""
            if (!focus) c.drawText(android.text.TextUtils.ellipsize(head, headPaint, colW, android.text.TextUtils.TruncateAt.END).toString(), px + colW / 2, safeTop + context.dpf(22), headPaint)
            var y = headH
            for (s in pages[pi].slices) {
                val h = s.to - s.from
                c.save()
                c.clipRect(px - context.dpf(8), y, px + colW + context.dpf(8), y + h)
                drawBlock(c, blocks[s.block], px, y - s.from, s.block, if (first == page && dx == 0f) RectF(px - context.dpf(8), y, px + colW + context.dpf(8), y + h) else null)
                c.restore()
                if (first == page) lineRects(blocks[s.block], px, y - s.from, y, y + h)
                y += h
            }
            if (!focus) c.drawText("${pi + 1}", px + colW / 2, height - context.dpf(10), headPaint)
        }
        if (dual) {
            boxPaint.style = Paint.Style.FILL
            boxPaint.color = p.border
            val mx = x0 + colW + gutter / 2
            c.drawRect(mx, headH, mx + context.dpf(1), headH + pageH, boxPaint)
        }
    }
}
