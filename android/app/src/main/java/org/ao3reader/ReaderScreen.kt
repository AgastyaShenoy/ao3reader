package org.ao3reader

import android.os.Handler
import android.os.Looper
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import android.text.Editable
import android.text.TextWatcher
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import android.widget.BaseAdapter
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ListView
import android.widget.SeekBar
import android.widget.Switch
import android.widget.TextView
import org.json.JSONArray
import org.json.JSONObject

/** The reader: page view plus toolbar, status bar and slide-in panels. */
class ReaderScreen(private val a: MainActivity, val workId: String, opened: JSONObject) : FrameLayout(a), BookView {
    val meta: JSONObject = opened.getJSONObject("meta")
    private val chapters: List<JSONObject> = opened.getJSONArray("chapters").objects()
    private var bookmarks: JSONArray = opened.getJSONArray("bookmarks")
    private var highlights: List<JSONObject> = opened.optJSONArray("highlights")?.objects() ?: emptyList()
    /** This chapter's highlights (the page draws them). */
    var chapterHighlights: List<JSONObject> = emptyList()
        private set
    private var highlightsVersion = 0
    val highlightsKey get() = highlightsVersion
    /** The bar of things to do with a selection or a highlight. */
    private var bar: View? = null
    val selectionBarShown get() = bar != null
    val chapterCount get() = chapters.size
    var chapter = (opened.getInt("chapter") - 1).coerceIn(0, chapters.size - 1)
        private set
    var chapterJson: JSONObject? = null
        private set

    var hits: List<Hit> = emptyList()
        private set
    var currentHit: Hit? = null
        private set
    val hitsKey get() = "${hits.size}:${currentHit?.let { "${it.chapter}.${it.para}.${it.start}" }}"
    private var query = ""
    private var searchAll = true

    private val page = PageView(a, this)
    private var top = LinearLayout(a)
    private var bottom = LinearLayout(a)
    private var status = TextView(a)
    private var progressLine = View(a)
    private var panel: View? = null
    private var panelGravity = Gravity.BOTTOM
    /** Toolbar and status bar; hidden by default so the page uses the whole screen. */
    private var chrome = false
    private var insetTop = 0
    private var insetBottom = 0
    private val handler = Handler(Looper.getMainLooper())
    private val saveLater = Runnable { saveProgress() }
    private var dirty = false

    // Read aloud, with the phone's own text-to-speech voice.
    private var tts: TextToSpeech? = null
    private var ttsReady = false
    private var reading = false
    private var aloudButton: FrameLayout? = null
    /** The block being read aloud (an index into the page's blocks), or -1. */
    var aloudBlock = -1
        private set
    /** The word being said, as character positions in that block's text. */
    var aloudWord: IntRange? = null
        private set

    init {
        loadChapter(chapter, opened.getDouble("position").toFloat())
        rebuild()
    }

    override val view: View get() = this

    override fun setInsets(t: Int, b: Int, cutout: Int) {
        insetTop = t
        insetBottom = b
        if (page.safeTop != cutout) {
            page.safeTop = cutout
            page.relayout()
        }
        top.setPadding(a.dp(6), t + a.dp(6), a.dp(6), a.dp(6))
        bottom.setPadding(a.dp(6), a.dp(4), a.dp(6), b + a.dp(8))
    }

    /** Recreates the chrome (after a theme change) while keeping the reading position. */
    override fun rebuild() {
        val p = Settings.pal
        removeAllViews()
        setBackgroundColor(p.bg)
        page.paged = Settings.typo.optString("reading_mode") != "Scroll"
        addView(page, frame())
        page.relayout()

        top = a.row().apply { setBackgroundColor(p.surface) }
        // Big bars: they're reached for with a thumb while holding the phone or tablet.
        top.addView(a.iconButton(R.drawable.ic_back, p.text, "Back to library", large = true) { a.closeReader() })
        top.addView(a.iconButton(R.drawable.ic_list, p.text, "Contents and bookmarks", large = true) { openContents() })
        top.addView(a.label(meta.getString("title"), if (a.resources.configuration.screenWidthDp >= 600) 18f else 16f, p.text, bold = true, lines = 1).apply { gravity = Gravity.CENTER_VERTICAL },
            LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply { leftMargin = a.dp(6) })
        top.addView(a.iconButton(R.drawable.ic_search, p.text, "Search in this work", large = true) { openSearch() })
        top.addView(a.iconButton(R.drawable.ic_bookmark, p.text, "Bookmark this page", large = true) { addBookmark() })
        if (meta.optBoolean("ao3", true)) top.addView(a.iconButton(R.drawable.ic_heart, p.text, "Leave kudos on AO3", large = true) { kudos() })
        aloudButton = a.iconButton(if (reading) R.drawable.ic_stop else R.drawable.ic_volume, p.text, if (reading) "Stop reading aloud" else "Read aloud", reading, large = true) { toggleAloud() }
        top.addView(aloudButton)
        top.addView(a.iconButton(R.drawable.ic_text, p.text, "Text and theme", large = true) { openAppearance() })
        addView(top, frame(h = ViewGroup.LayoutParams.WRAP_CONTENT, gravity = Gravity.TOP))

        bottom = a.column().apply { setBackgroundColor(p.surface) }
        progressLine = View(a).apply { setBackgroundColor(p.accentMark) }
        val track = FrameLayout(a).apply { setBackgroundColor(p.border) }
        track.addView(progressLine, frame(0, a.dp(4)))
        bottom.addView(track, lp(h = a.dp(4)))
        val row = a.row().apply { setPadding(0, a.dp(4), 0, 0) }
        row.addView(a.iconButton(R.drawable.ic_prev, p.text, "Previous chapter", large = true) { if (chapter > 0) jumpChapter(chapter - 1, 0f) })
        status = a.label("", 15f, p.muted, lines = 2).apply { gravity = Gravity.CENTER; setLineSpacing(0f, 1.1f) }
        row.addView(status, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        row.addView(a.iconButton(R.drawable.ic_next, p.text, "Next chapter", large = true) { if (chapter + 1 < chapters.size) jumpChapter(chapter + 1, 0f) })
        bottom.addView(row)
        addView(bottom, frame(h = ViewGroup.LayoutParams.WRAP_CONTENT, gravity = Gravity.BOTTOM))
        bottom.gone(!Settings.typo.optBoolean("show_reading_progress_bar", true))

        setInsets(insetTop, insetBottom, page.safeTop)
        showChrome(chrome)
        positionChanged()
    }

    // ---- chapters and position ----------------------------------------------------------

    private fun loadChapter(i: Int, anchor: Float) {
        chapter = i
        chapterHighlights = highlights.filter { it.getInt("chapter") == i }
        chapterJson = Core.obj("work.chapter", json("index" to i))
        page.setChapter(anchor)
        dirty = true
    }

    fun goChapter(i: Int, anchor: Float) {
        if (i !in chapters.indices) return
        loadChapter(i, anchor)
        positionChanged()
    }

    /** A chapter change from the menus: the new text fades in. */
    private fun jumpChapter(i: Int, anchor: Float) {
        goChapter(i, anchor)
        Anim.enter(page, alpha = 0.15f, ms = 200)
    }

    fun nextChapter() {
        if (chapter + 1 < chapters.size) goChapter(chapter + 1, 0f)
    }

    fun prevChapter(toEnd: Boolean) {
        if (chapter > 0) goChapter(chapter - 1, if (toEnd) Float.MAX_VALUE else 0f)
    }

    /** Updates the status bar and schedules a progress save. */
    fun positionChanged() {
        if (chapterJson == null) return
        val anchor = page.anchor()
        val stats = runCatching { Core.obj("progress.stats", json("chapter" to chapter, "position" to anchor.toDouble())) }.getOrNull()
        val pct = stats?.optDouble("percentage") ?: 0.0
        // Measured from the user's own reading, or the speed set by hand.
        val wpm = stats?.optInt("wpm", 0)?.takeIf { it > 0 } ?: Settings.config.optInt("words_per_minute", 230)
        val done = chapter + 1 >= chapters.size && page.atEnd()
        val parts = mutableListOf("Chapter ${chapter + 1} of ${chapters.size}")
        if (page.paged && page.pageCount > 0) {
            parts += if (page.spread && page.page + 1 < page.pageCount) "Pages ${page.page + 1}–${page.page + 2} of ${page.pageCount}" else "Page ${page.page + 1} of ${page.pageCount}"
        }
        val second = if (done) "Finished" else if (stats != null) {
            "${pct.toInt()}%  ·  ${minutes(stats.getLong("words_left"), wpm)} left" +
                if (chapters.size > 1) "  ·  ${minutes(stats.getLong("chapter_words_left"), wpm)} in chapter" else ""
        } else ""
        status.text = parts.joinToString("  ·  ") + "\n" + second
        // TalkBack reads the page's text.
        page.contentDescription = page.visibleText()
        progressLine.post {
            val w = (progressLine.parent as? View)?.width ?: 0
            progressLine.layoutParams = FrameLayout.LayoutParams((w * pct / 100).toInt(), a.dp(4))
        }
        dirty = true
        handler.removeCallbacks(saveLater)
        handler.postDelayed(saveLater, 3000)
    }

    override fun saveProgress() {
        if (!dirty) return
        dirty = false
        val done = chapter + 1 >= chapters.size && page.atEnd()
        runCatching { Core.req("progress.save", json("chapter" to chapter, "position" to page.anchor().toDouble(), "completed" to done)) }
    }

    // ---- read aloud ----------------------------------------------------------------------

    private fun toggleAloud() = if (reading) stopAloud() else startAloud()

    private fun startAloud() {
        reading = true
        updateAloudButton()
        if (tts == null) {
            tts = TextToSpeech(a) { status ->
                if (status == TextToSpeech.SUCCESS) {
                    ttsReady = true
                    if (reading) sayFrom(page.firstSpeakable())
                } else {
                    // No voice to use: try again from scratch next time.
                    tts?.shutdown()
                    tts = null
                    a.toast("Read aloud needs a voice: Settings → Accessibility → Text-to-speech")
                    stopAloud()
                }
            }.apply { setOnUtteranceProgressListener(progress) }
        } else if (ttsReady) sayFrom(page.firstSpeakable())
    }

    private fun say(block: Int) {
        val text = page.speakText(block) ?: return stopAloud()
        aloudBlock = block
        aloudWord = null
        tts?.setSpeechRate(Settings.config.optDouble("speech_rate", 1.0).toFloat().coerceIn(0.5f, 2f))
        // Voices take a few thousand characters at a time: a long paragraph goes in pieces, broken
        // after a sentence or a space. Each piece's id carries where it starts ("b12@3990").
        val most = (TextToSpeech.getMaxSpeechInputLength() - 100).coerceAtLeast(500)
        var from = 0
        while (from < text.length) {
            var to = minOf(text.length, from + most)
            if (to < text.length) {
                val cut = text.lastIndexOf(". ", to).takeIf { it > from + most / 2 } ?: text.lastIndexOf(' ', to).takeIf { it > from }
                if (cut != null) to = cut + 1
            }
            tts?.speak(text.substring(from, to), if (from == 0) TextToSpeech.QUEUE_FLUSH else TextToSpeech.QUEUE_ADD, null, if (to >= text.length) "b$block" else "b$block@$from")
            from = to
        }
        page.followAloud()
        page.invalidate()
    }

    private fun sayFrom(block: Int?) = if (block == null) aloudNext(-1) else say(block)

    /** On to the next block worth reading, or into the next chapter. */
    private fun aloudNext(after: Int) {
        val next = page.nextSpeakable(after)
        when {
            next != null -> say(next)
            chapter + 1 < chapters.size -> {
                goChapter(chapter + 1, 0f)
                page.nextSpeakable(-1)?.let { say(it) } ?: stopAloud()
            }
            else -> stopAloud()
        }
    }

    fun stopAloud() {
        reading = false
        tts?.stop()
        aloudBlock = -1
        aloudWord = null
        updateAloudButton()
        page.invalidate()
    }

    /** The reader turned the page or scrolled away while reading aloud: carry on from there. */
    fun userMoved() {
        if (reading && ttsReady && !page.showing(aloudBlock)) sayFrom(page.firstSpeakable())
    }

    private fun updateAloudButton() {
        val b = aloudButton ?: return
        val p = Settings.pal
        (b.getChildAt(0) as? android.widget.ImageView)?.setImageResource(if (reading) R.drawable.ic_stop else R.drawable.ic_volume)
        val tip = if (reading) "Stop reading aloud" else "Read aloud"
        b.tooltipText = tip
        b.contentDescription = tip
        (b.background as? android.graphics.drawable.RippleDrawable)?.setDrawable(0, if (reading) rounded(p.selected, b.width / 2f) else null)
    }

    private val progress = object : UtteranceProgressListener() {
        override fun onStart(id: String?) {}
        override fun onDone(id: String?) {
            handler.post { if (reading && id == "b$aloudBlock") aloudNext(aloudBlock) }
        }
        @Deprecated("Deprecated in Java")
        override fun onError(id: String?) {
            handler.post { stopAloud() }
        }
        override fun onRangeStart(id: String?, start: Int, end: Int, frame: Int) {
            handler.post {
                // A later piece of a long paragraph says where in it the piece starts.
                val (block, offset) = id.orEmpty().removePrefix("b").split('@').let { it[0] to (it.getOrNull(1)?.toIntOrNull() ?: 0) }
                if (reading && block == "$aloudBlock") {
                    aloudWord = (start + offset) until (end + offset)
                    page.followAloud()
                    page.invalidate()
                }
            }
        }
    }

    /** The reader is closing: let the voice go. */
    override fun close() {
        reading = false
        tts?.stop()
        tts?.shutdown()
        tts = null
    }

    // ---- selection, highlights, notes and definitions ----------------------------------

    /** Shows what can be done with the selection (or the highlight `existing`), above `top`, or
     *  below `bottom` when there's no room above. */
    fun showSelectionBar(x: Float, top: Float, bottom: Float, existing: JSONObject?) {
        hideSelectionBar()
        val p = Settings.pal
        val row = a.row().apply {
            background = rounded(p.card, a.dpf(14), p.border, a.dp(1))
            elevation = a.dpf(6)
            setPadding(a.dp(8), a.dp(2), a.dp(2), a.dp(2))
        }
        HIGHLIGHTS.forEachIndexed { i, c ->
            val chosen = existing != null && existing.optInt("color") == i
            row.addView(View(a).apply {
                background = android.graphics.drawable.GradientDrawable().apply {
                    shape = android.graphics.drawable.GradientDrawable.OVAL
                    setColor(Pal.opaque(c))
                    if (chosen) setStroke(a.dp(2), p.text)
                }
                contentDescription = "${HIGHLIGHT_NAMES[i]} highlight"
                tooltipText = contentDescription
                setOnClickListener { saveHighlight(existing?.let { JSONObject(it.toString()).put("color", i) } ?: page.selectionHighlight(i)) }
            }, LinearLayout.LayoutParams(a.dp(26), a.dp(26)).apply { leftMargin = a.dp(5); rightMargin = a.dp(5) })
        }
        row.addView(View(a).apply { setBackgroundColor(p.border) }, LinearLayout.LayoutParams(a.dp(1), a.dp(24)).apply { leftMargin = a.dp(6); rightMargin = a.dp(2) })
        row.addView(a.iconButton(R.drawable.ic_edit, p.text, if (existing?.optString("note").isNullOrEmpty()) "Add a note" else "Edit the note") { editNote(existing) })
        row.addView(a.iconButton(R.drawable.ic_copy, p.text, "Copy") { copy(existing?.getString("text") ?: page.selectedText()) })
        if (existing == null && page.selectedText().split(Regex("\\s+")).size <= 3) row.addView(a.iconButton(R.drawable.ic_book, p.text, "Define") { define(page.selectedText()) })
        if (existing != null) row.addView(a.iconButton(R.drawable.ic_delete, p.text, "Remove highlight") {
            highlights = Core.arr("highlight.remove", json("id" to workId, "highlight" to existing.getString("id"))).objects()
            highlightsChanged()
        })
        row.measure(MeasureSpec.makeMeasureSpec(width, MeasureSpec.AT_MOST), MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED))
        val w = row.measuredWidth
        val h = row.measuredHeight
        val gap = a.dp(10)
        val y = if (top - h - gap > insetTop + a.dp(8)) top - h - gap else (bottom + gap).coerceAtMost((height - h - insetBottom - a.dp(8)).toFloat())
        val lp = frame(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, Gravity.TOP or Gravity.START).apply {
            leftMargin = (x - w / 2).toInt().coerceIn(a.dp(8), (width - w - a.dp(8)).coerceAtLeast(a.dp(8)))
            topMargin = y.toInt()
        }
        addView(row, lp)
        bar = row
        Anim.enter(row, dy = a.dpf(6), ms = 140)
    }

    fun hideSelectionBar() {
        bar?.let { removeView(it) }
        bar = null
    }

    private fun highlightsChanged() {
        chapterHighlights = highlights.filter { it.getInt("chapter") == chapter }
        highlightsVersion++
        hideSelectionBar()
        page.relayout()
    }

    private fun saveHighlight(h: JSONObject?) {
        if (h == null) return
        highlights = Core.arr("highlight.save", json("id" to workId, "highlight" to h)).objects()
        highlightsChanged()
    }

    /** A note on a highlight (made yellow if the passage wasn't highlighted yet). */
    private fun editNote(existing: JSONObject?) {
        val h = existing?.let { JSONObject(it.toString()) } ?: page.selectionHighlight(0) ?: return
        hideSelectionBar()
        val p = Settings.pal
        val box = a.column()
        // The passage, with a bar of its highlight colour beside it.
        val quote = a.row().apply { gravity = Gravity.TOP }
        quote.addView(View(a).apply { background = rounded(Pal.opaque(HIGHLIGHTS[h.optInt("color").coerceIn(0, 3)]), a.dpf(1.5f)) },
            LinearLayout.LayoutParams(a.dp(3), ViewGroup.LayoutParams.MATCH_PARENT).apply { rightMargin = a.dp(10) })
        quote.addView(a.label(h.optString("text"), 13.5f, alpha(p.text, 0.8f), lines = 3).apply { setTypeface(typeface, android.graphics.Typeface.ITALIC) }, LinearLayout.LayoutParams(0, -2, 1f))
        box.addView(quote, lp(bottom = a.dp(12)))
        val input = a.input("Your note").apply {
            isSingleLine = false
            minLines = 3
            gravity = Gravity.TOP or Gravity.START
            inputType = android.text.InputType.TYPE_CLASS_TEXT or android.text.InputType.TYPE_TEXT_FLAG_MULTI_LINE or android.text.InputType.TYPE_TEXT_FLAG_CAP_SENTENCES
            setText(h.optString("note"))
        }
        box.addView(input, lp())
        a.dialog(if (h.optString("note").isEmpty()) "Add a note" else "Edit the note", body = box, ok = "Save") {
            h.put("note", input.text.toString().trim())
            saveHighlight(h)
            a.toast("Note saved")
            true
        }
        input.requestFocus()
    }

    private fun copy(text: String) {
        if (text.isEmpty()) return
        a.getSystemService(android.content.ClipboardManager::class.java)?.setPrimaryClip(android.content.ClipData.newPlainText("Passage", text))
        // Android 13 and later show their own notice.
        if (android.os.Build.VERSION.SDK_INT < 33) a.toast("Copied")
        page.clearSelection()
    }

    /** Wiktionary's meanings of a word, in a panel at the bottom. */
    private fun define(text: String) {
        val p = Settings.pal
        page.clearSelection()
        val word = text.trim().trim { !it.isLetterOrDigit() && it != '-' && it != '\'' }
        val box = a.column().apply { setPadding(a.dp(20), a.dp(16), a.dp(20), insetBottom + a.dp(14)) }
        val head = a.row()
        head.addView(a.label(word, 21f, p.text, bold = true, lines = 1), LinearLayout.LayoutParams(0, -2, 1f))
        head.addView(a.iconButton(R.drawable.ic_close, p.text, "Close") { closePanel() })
        box.addView(head)
        val body = a.column()
        body.addView(a.label("Looking it up…", 14f, p.muted), lp(top = a.dp(8)))
        val cap = (height * 0.6f).toInt()
        val scroll = object : android.widget.ScrollView(a) {
            override fun onMeasure(w: Int, h: Int) = super.onMeasure(w, MeasureSpec.makeMeasureSpec(cap, MeasureSpec.AT_MOST))
        }
        scroll.addView(body)
        box.addView(scroll, lp())
        box.addView(a.label("From Wiktionary", 11.5f, p.muted), lp(top = a.dp(10)))
        showPanel(box, Gravity.BOTTOM, height = ViewGroup.LayoutParams.WRAP_CONTENT)
        Core.background("define", json("word" to word)) { r ->
            body.removeAllViews()
            r.onSuccess { res ->
                for (m in (res as JSONObject).getJSONArray("meanings").objects()) {
                    body.addView(a.label(m.getString("part"), 13f, p.accentText, bold = true), lp(top = a.dp(12), bottom = a.dp(4)))
                    m.getJSONArray("definitions").strings().take(6).forEachIndexed { i, d ->
                        val line = a.row().apply { gravity = Gravity.TOP }
                        line.addView(a.label("${i + 1}.", 14f, p.muted), LinearLayout.LayoutParams(a.dp(22), -2))
                        line.addView(a.label(d, 14.5f, p.text).apply { setLineSpacing(0f, 1.15f) }, LinearLayout.LayoutParams(0, -2, 1f))
                        body.addView(line, lp(bottom = a.dp(6)))
                    }
                }
            }.onFailure { body.addView(a.label(it.message ?: "Couldn't look it up.", 14f, p.muted), lp(top = a.dp(8))) }
        }
    }

    // ---- chrome and panels --------------------------------------------------------------

    fun toggleChrome() {
        if (panel != null) return closePanel()
        hideSelectionBar()
        chrome = !chrome
        showChrome(chrome, animate = true)
    }

    private fun showChrome(on: Boolean, animate: Boolean = false) {
        slide(top, on, fromTop = true, animate)
        slide(bottom, on && Settings.typo.optBoolean("show_reading_progress_bar", true), fromTop = false, animate)
        a.immersive(!on)
    }

    /** Shows or hides a toolbar, sliding it from or to its edge of the screen. */
    private fun slide(v: View, show: Boolean, fromTop: Boolean, animate: Boolean) {
        val away = { (if (fromTop) -1f else 1f) * v.height.coerceAtLeast(a.dp(56)) * 0.7f }
        if (!animate || !Anim.on) {
            v.animate().cancel()
            v.alpha = 1f
            v.translationY = 0f
            v.gone(!show)
        } else if (show) {
            v.alpha = 0f
            v.gone(false)
            v.post { Anim.enter(v, dy = away(), ms = 190) }
        } else if (v.visibility == View.VISIBLE) {
            Anim.exit(v, dy = away(), ms = 150) {
                v.gone(true)
                v.alpha = 1f
                v.translationY = 0f
            }
        }
    }

    /** Back button: closes a panel first. Returns false when the reader itself should close. */
    override fun back(): Boolean {
        if (bar != null) {
            page.clearSelection()
            return true
        }
        if (panel != null) {
            closePanel()
            return true
        }
        return false
    }

    private fun closePanel() {
        val pnl = panel ?: return
        panel = null
        (a.getSystemService(InputMethodManager::class.java))?.hideSoftInputFromWindow(windowToken, 0)
        val content = (pnl as ViewGroup).getChildAt(0)
        val (dx, dy) = panelOffset(content)
        if (content != null) Anim.exit(content, dx * 0.5f, dy * 0.5f, ms = 150) {}
        Anim.exit(pnl, ms = 170) { removeView(pnl) }
    }

    /** Where a panel slides in from: its own edge of the screen. */
    private fun panelOffset(v: View?): Pair<Float, Float> {
        val w = (v?.width ?: 0).coerceAtLeast(a.dp(200)).toFloat()
        val h = (v?.height ?: 0).coerceAtLeast(a.dp(160)).toFloat()
        return when (panelGravity) {
            Gravity.START -> -w to 0f
            Gravity.TOP -> 0f to -h
            else -> 0f to h
        }
    }

    private fun showPanel(content: View, gravity: Int, width: Int = ViewGroup.LayoutParams.MATCH_PARENT, height: Int = ViewGroup.LayoutParams.MATCH_PARENT) {
        closePanel()
        val p = Settings.pal
        val scrim = FrameLayout(a).apply {
            setBackgroundColor(alpha(0xFF000000.toInt(), 0.35f))
            setOnClickListener { closePanel() }
        }
        content.isClickable = true
        content.background = rounded(p.surface, a.dpf(14), p.border, a.dp(1))
        scrim.addView(content, frame(width, height, gravity))
        panel = scrim
        panelGravity = gravity
        addView(scrim, frame())
        if (Anim.on) {
            content.alpha = 0f
            Anim.enter(scrim, ms = 160)
            content.post {
                val (dx, dy) = panelOffset(content)
                Anim.enter(content, dx, dy, ms = 220)
            }
        }
    }

    private fun openContents() {
        val p = Settings.pal
        val box = a.column().apply { setPadding(a.dp(12), insetTop + a.dp(12), a.dp(12), insetBottom + a.dp(12)) }
        var tab = 0
        val list = ListView(a).apply { divider = null; selector = rounded(0, 0f) }
        val tabsHolder = FrameLayout(a)
        // AO3 works have their comments too: the current chapter's, a page at a time.
        val ao3 = meta.optBoolean("ao3", false)
        val footer = a.column()
        val comments = mutableListOf<JSONObject>()
        var cPage = 0
        var cNext = false
        var cLoading = false
        var cTried = false
        var cError: String? = null
        lateinit var fill: () -> Unit
        fun loadComments(page: Int) {
            cLoading = true
            cTried = true
            Core.background("ao3.comments", json("chapter" to chapter, "page" to page)) { r ->
                cLoading = false
                r.onSuccess {
                    val o = it as JSONObject
                    if (page <= 1) comments.clear()
                    comments.addAll(o.getJSONArray("comments").objects())
                    cPage = o.getInt("page")
                    cNext = o.getBoolean("has_next")
                    cError = null
                }.onFailure { cError = it.message }
                if (tab == 3) fill()
            }
        }
        fun commentsFooter() {
            footer.removeAllViews()
            if (!Core.obj("session").getBoolean("signed_in")) {
                footer.addView(a.label("Sign in to AO3 (Settings › Sources › Accounts) to leave a comment.", 12.5f, p.muted), lp(top = a.dp(8)))
                return
            }
            val draft = a.input("Write a comment…").apply {
                inputType = android.text.InputType.TYPE_CLASS_TEXT or android.text.InputType.TYPE_TEXT_FLAG_MULTI_LINE or android.text.InputType.TYPE_TEXT_FLAG_CAP_SENTENCES
                maxLines = 4
            }
            footer.addView(draft, lp(top = a.dp(8)))
            lateinit var post: android.widget.TextView
            post = a.button("Post", Kind.Primary) {
                val text = draft.text.toString().trim()
                if (text.isEmpty()) return@button
                post.isEnabled = false
                post.text = "Posting…"
                Core.background("ao3.comment", json("chapter" to chapter, "text" to text)) { r ->
                    post.isEnabled = true
                    post.text = "Post"
                    r.onSuccess { draft.setText(""); a.toast("Comment posted"); loadComments(1) }.onFailure { a.toast(it.message ?: "Couldn't post the comment") }
                }
            }
            footer.addView(post, lp(-2, top = a.dp(8)))
        }
        fill = fun() {
            tabsHolder.removeAllViews()
            val names = listOf("Chapters", "Bookmarks", "Highlights") + if (ao3) listOf("Comments") else emptyList()
            tabsHolder.addView(a.segmented(names, tab, counts = listOf(0, bookmarks.length(), highlights.size, 0).take(names.size)) { tab = it; fill() })
            footer.gone(tab != 3)
            if (tab == 3) {
                // Asked once when the tab opens; a failure shows, it isn't retried on its own.
                if (!cTried) loadComments(1)
                val rows = comments.toList()
                val message = when {
                    rows.isNotEmpty() -> null
                    cLoading -> "Loading comments from AO3…"
                    cError != null -> cError
                    else -> "No comments yet."
                }
                list.adapter = object : BaseAdapter() {
                    override fun getCount() = if (message != null) 1 else rows.size + if (cNext) 1 else 0
                    override fun getItem(i: Int) = i
                    override fun getItemId(i: Int) = i.toLong()
                    override fun getView(i: Int, c: View?, parent: ViewGroup?): View {
                        if (message != null) return a.label(message, 14f, p.muted).apply { setPadding(a.dp(12), a.dp(20), a.dp(12), a.dp(20)); gravity = Gravity.CENTER }
                        if (i == rows.size) return a.button(if (cLoading) "Loading…" else "Load more") { if (!cLoading) { loadComments(cPage + 1); fill() } }
                        val m = rows[i]
                        val t = a.column().apply { setPadding(a.dp(12) + a.dp(16) * m.optInt("depth").coerceAtMost(4), a.dp(10), a.dp(12), a.dp(10)) }
                        val head = a.row()
                        head.addView(a.label(m.optString("author").ifBlank { "Someone" }, 13.5f, p.text, bold = true, lines = 1))
                        val whenText = listOf(m.optString("chapter"), m.optString("date")).filter { it.isNotBlank() }.joinToString("  ·  ")
                        head.addView(a.label(whenText, 12f, p.muted, lines = 1).apply { setPadding(a.dp(8), 0, 0, 0) })
                        t.addView(head)
                        t.addView(a.label(m.getJSONArray("paragraphs").strings().joinToString("\n\n"), 14f, p.text).apply { setLineSpacing(0f, 1.15f) }, lp(top = a.dp(4)))
                        return t
                    }
                }
            } else if (tab == 0) {
                list.adapter = object : BaseAdapter() {
                    override fun getCount() = chapters.size
                    override fun getItem(i: Int) = chapters[i]
                    override fun getItemId(i: Int) = i.toLong()
                    override fun getView(i: Int, c: View?, parent: ViewGroup?): View {
                        val r = a.row().apply { setPadding(a.dp(12), a.dp(12), a.dp(12), a.dp(12)) }
                        if (i == chapter) r.background = rounded(p.selected, a.dpf(8))
                        r.addView(a.label(chapters[i].getString("title").ifBlank { "Chapter ${i + 1}" }, 14.5f, p.text, bold = i == chapter, lines = 2),
                            LinearLayout.LayoutParams(0, -2, 1f))
                        r.addView(a.label("${thousands(chapters[i].getLong("words"))} words", 12f, p.muted))
                        r.setOnClickListener { closePanel(); jumpChapter(i, 0f) }
                        return r
                    }
                }
                list.setSelection((chapter - 2).coerceAtLeast(0))
            } else if (tab == 2) {
                val hs = highlights
                list.adapter = object : BaseAdapter() {
                    override fun getCount() = hs.size
                    override fun getItem(i: Int) = hs[i]
                    override fun getItemId(i: Int) = i.toLong()
                    override fun getView(i: Int, c: View?, parent: ViewGroup?): View {
                        val h = hs[i]
                        val r = a.row().apply { setPadding(a.dp(10), a.dp(10), a.dp(4), a.dp(10)); gravity = Gravity.TOP }
                        r.addView(View(a).apply { background = rounded(Pal.opaque(HIGHLIGHTS[h.optInt("color").coerceIn(0, 3)]), a.dpf(2)) },
                            LinearLayout.LayoutParams(a.dp(4), ViewGroup.LayoutParams.MATCH_PARENT).apply { rightMargin = a.dp(10) })
                        val t = a.column()
                        val n = h.getInt("chapter")
                        t.addView(a.label(chapters.getOrNull(n)?.getString("title")?.ifBlank { null } ?: "Chapter ${n + 1}", 12.5f, p.muted, bold = true, lines = 1))
                        t.addView(a.label(h.optString("text"), 13.5f, p.text, lines = 3), lp(top = a.dp(2)))
                        if (h.optString("note").isNotEmpty()) t.addView(a.label(h.getString("note"), 13f, alpha(p.text, 0.75f), lines = 2).apply { setTypeface(typeface, android.graphics.Typeface.ITALIC) }, lp(top = a.dp(4)))
                        r.addView(t, LinearLayout.LayoutParams(0, -2, 1f))
                        r.addView(a.iconButton(R.drawable.ic_delete, p.muted, "Remove highlight") {
                            highlights = Core.arr("highlight.remove", json("id" to workId, "highlight" to h.getString("id"))).objects()
                            highlightsChanged()
                            fill()
                        })
                        r.setOnClickListener { closePanel(); jumpChapter(n, h.getJSONArray("start").getInt(0).toFloat()) }
                        return r
                    }
                }
                if (hs.isEmpty()) a.toast("No highlights yet. Press and hold on a word to start one.")
            } else {
                val bms = bookmarks.objects()
                list.adapter = object : BaseAdapter() {
                    override fun getCount() = bms.size
                    override fun getItem(i: Int) = bms[i]
                    override fun getItemId(i: Int) = i.toLong()
                    override fun getView(i: Int, c: View?, parent: ViewGroup?): View {
                        val b = bms[i]
                        val r = a.row().apply { setPadding(a.dp(12), a.dp(10), a.dp(4), a.dp(10)) }
                        val t = a.column()
                        t.addView(a.label("Chapter ${b.getInt("chapter")}", 12.5f, p.accentText, bold = true))
                        t.addView(a.label(b.getString("note").ifBlank { b.getString("excerpt") }, 13.5f, p.text, lines = 2))
                        r.addView(t, LinearLayout.LayoutParams(0, -2, 1f))
                        r.addView(a.iconButton(R.drawable.ic_delete, p.muted, "Delete bookmark") {
                            bookmarks = Core.arr("bookmark.remove", json("id" to workId, "bookmark" to b.getString("id")))
                            fill()
                        })
                        r.setOnClickListener { closePanel(); jumpChapter(b.getInt("chapter") - 1, b.getInt("para").toFloat()) }
                        return r
                    }
                }
                if (bms.isEmpty()) a.toast("No bookmarks yet. Use the bookmark button to add one.")
            }
        }
        box.addView(tabsHolder, lp(bottom = a.dp(8)))
        box.addView(list, LinearLayout.LayoutParams(-1, 0, 1f))
        if (ao3) {
            commentsFooter()
            box.addView(footer, lp())
        }
        fill()
        showPanel(box, Gravity.START, (width * 0.86f).toInt().coerceAtMost(a.dp(420)))
    }

    private fun openAppearance() {
        val p = Settings.pal
        val t = Settings.typo
        val box = a.column().apply { setPadding(a.dp(18), a.dp(16), a.dp(18), insetBottom + a.dp(16)) }
        fun changed() {
            Settings.save()
            page.relayout()
            positionChanged()
        }
        box.addView(a.label("Theme", 12.5f, p.muted, bold = true))
        val swatches = a.row()
        Settings.themes.forEach { th ->
            val sel = th.id == p.id
            val slot = if (th.dark) "dark_theme" else "light_theme" 
            swatches.addView(View(a).apply {
                background = rounded(th.bg, a.dpf(18), if (sel) p.text else p.border, a.dp(if (sel) 2 else 1))
                contentDescription = th.name
                setOnClickListener {
                    Settings.config.put(if (Settings.config.optBoolean("follow_system")) slot else "theme", th.id)
                    Settings.save()
                    closePanel()
                    a.rebuild()
                }
            }, LinearLayout.LayoutParams(a.dp(36), a.dp(36)).apply { rightMargin = a.dp(8) })
        }
        box.addView(swatches, lp(top = a.dp(8), bottom = a.dp(14)))

        val size = a.row()
        val sizeLabel = a.label("${t.optInt("font_size")}", 15f, p.text).apply { gravity = Gravity.CENTER }
        size.addView(a.button("A−") {
            t.put("font_size", (t.optDouble("font_size") - 1).coerceAtLeast(12.0)); sizeLabel.text = "${t.optInt("font_size")}"; changed()
        })
        size.addView(sizeLabel, LinearLayout.LayoutParams(0, -2, 1f))
        size.addView(a.button("A+") {
            t.put("font_size", (t.optDouble("font_size") + 1).coerceAtMost(36.0)); sizeLabel.text = "${t.optInt("font_size")}"; changed()
        })
        box.addView(a.label("Text size", 12.5f, p.muted, bold = true))
        box.addView(size, lp(top = a.dp(6), bottom = a.dp(12)))

        box.addView(a.label("Line height", 12.5f, p.muted, bold = true))
        box.addView(SeekBar(a).apply {
            max = 28
            progress = ((t.optDouble("line_spacing_multiplier") - 1.2) / 0.05).toInt()
            progressTintList = android.content.res.ColorStateList.valueOf(p.accentMark)
            thumbTintList = android.content.res.ColorStateList.valueOf(p.accentMark)
            setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
                override fun onProgressChanged(s: SeekBar?, v: Int, u: Boolean) { t.put("line_spacing_multiplier", 1.2 + v * 0.05) }
                override fun onStartTrackingTouch(s: SeekBar?) {}
                override fun onStopTrackingTouch(s: SeekBar?) = changed()
            })
        }, lp(top = a.dp(4), bottom = a.dp(12)))

        val fonts = listOf("SansSerif" to "Sans", "Serif" to "Serif", "Monospace" to "Mono")
        box.addView(a.label("Font", 12.5f, p.muted, bold = true))
        box.addView(a.segmented(fonts.map { it.second }, fonts.indexOfFirst { it.first == t.opt("font_family") }) { i ->
            t.put("font_family", fonts[i].first); changed(); openAppearance()
        }, lp(top = a.dp(6), bottom = a.dp(12)))

        box.addView(a.label("Alignment", 12.5f, p.muted, bold = true))
        box.addView(a.segmented(listOf("Left", "Justified"), if (t.optString("text_alignment") == "Justified") 1 else 0) { i ->
            t.put("text_alignment", if (i == 1) "Justified" else "Left"); changed(); openAppearance()
        }, lp(top = a.dp(6), bottom = a.dp(12)))

        box.addView(a.label("Layout", 12.5f, p.muted, bold = true))
        box.addView(a.segmented(listOf("Pages", "Scroll"), if (page.paged) 0 else 1) { i ->
            t.put("reading_mode", if (i == 0) "Paged" else "Scroll")
            Settings.save()
            closePanel()
            rebuild()
        }, lp(top = a.dp(6)))
        showPanel(box, Gravity.BOTTOM, height = ViewGroup.LayoutParams.WRAP_CONTENT)
    }

    private fun openSearch() {
        val p = Settings.pal
        val box = a.column().apply { setPadding(a.dp(14), insetTop + a.dp(10), a.dp(14), a.dp(10)) }
        val input = a.input("Find in this work")
        input.setText(query)
        input.imeOptions = EditorInfo.IME_ACTION_SEARCH
        box.addView(input)
        val row = a.row()
        val all = Switch(a).apply { isChecked = searchAll; text = "All chapters"; setTextColor(p.text) }
        row.addView(all)
        val count = a.label("", 13f, p.muted).apply { gravity = Gravity.END }
        row.addView(count, LinearLayout.LayoutParams(0, -2, 1f).apply { rightMargin = a.dp(6) })
        fun show() {
            count.text = when {
                hits.isEmpty() && query.trim().length >= 2 -> "No matches"
                hits.isEmpty() -> ""
                else -> "${hits.indexOf(currentHit) + 1} of ${hits.size}${if (hits.size >= 2000) "+" else ""}"
            }
            count.setTextColor(if (hits.isEmpty() && query.length >= 2) p.dangerText else p.muted)
        }
        fun goto(h: Hit) {
            currentHit = h
            val text = Core.obj("work.chapter", json("index" to h.chapter)).getJSONArray("blocks").objects().firstOrNull { it.getInt("para") == h.para }?.getString("text") ?: ""
            val frac = if (text.isEmpty()) 0f else (h.start.toFloat() / text.length * 0.9f).coerceAtMost(0.9f)
            if (h.chapter != chapter) loadChapter(h.chapter, h.para + frac) else { page.relayout(); page.goTo(h.para + frac) }
            positionChanged()
            show()
        }
        fun run() {
            query = input.text.toString()
            searchAll = all.isChecked
            val res = runCatching { Core.arr("search", json("query" to query, "all" to searchAll, "chapter" to chapter)) }.getOrNull() ?: JSONArray()
            hits = (0 until res.length()).map { val m = res.getJSONArray(it); Hit(m.getInt(0), m.getInt(1), m.getInt(2), m.getInt(3)) }
            currentHit = null
            val anchor = page.anchor()
            val first = hits.firstOrNull { it.chapter != chapter || it.para >= anchor.toInt() } ?: hits.firstOrNull()
            if (first != null) goto(first) else { page.relayout(); show() }
        }
        row.addView(a.iconButton(R.drawable.ic_up, p.text, "Previous match") { if (hits.isNotEmpty()) goto(hits[(hits.indexOf(currentHit) - 1 + hits.size) % hits.size]) })
        row.addView(a.iconButton(R.drawable.ic_down, p.text, "Next match") { if (hits.isNotEmpty()) goto(hits[(hits.indexOf(currentHit) + 1) % hits.size]) })
        row.addView(a.iconButton(R.drawable.ic_close, p.text, "Close search") {
            hits = emptyList(); currentHit = null; query = ""
            closePanel()
            page.relayout()
        })
        box.addView(row, lp(top = a.dp(6)))
        input.setOnEditorActionListener { _, _, _ -> run(); true }
        all.setOnCheckedChangeListener { _, _ -> run() }
        show()
        showPanel(box, Gravity.TOP, height = ViewGroup.LayoutParams.WRAP_CONTENT)
        // The search panel keeps the page visible: no dimming behind it.
        (panel as? FrameLayout)?.setBackgroundColor(0)
        input.requestFocus()
        (a.getSystemService(InputMethodManager::class.java))?.showSoftInput(input, 0)
    }

    private fun addBookmark() {
        val input = a.input("Add a note (optional)")
        a.dialog("Bookmark this page", body = input, ok = "Add") {
            bookmarks = Core.arr("bookmark.add", json("id" to workId, "chapter" to chapter + 1, "para" to page.anchor().toInt(), "note" to input.text.toString()))
            a.toast("Bookmark added")
            true
        }
    }

    private fun kudos() {
        a.toast("Sending kudos…")
        Core.background("kudos", json("id" to workId)) { r ->
            r.onSuccess { a.toast(it.toString()) }.onFailure { a.toast("Couldn't leave kudos: ${it.message}") }
        }
    }
}
