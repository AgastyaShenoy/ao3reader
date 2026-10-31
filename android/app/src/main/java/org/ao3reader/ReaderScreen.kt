package org.ao3reader

import android.app.AlertDialog
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
class ReaderScreen(private val a: MainActivity, val workId: String, opened: JSONObject) : FrameLayout(a) {
    val meta: JSONObject = opened.getJSONObject("meta")
    private val chapters: List<JSONObject> = opened.getJSONArray("chapters").objects()
    private var bookmarks: JSONArray = opened.getJSONArray("bookmarks")
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

    fun setInsets(t: Int, b: Int, cutout: Int = page.safeTop) {
        insetTop = t
        insetBottom = b
        if (page.safeTop != cutout) {
            page.safeTop = cutout
            page.relayout()
        }
        top.setPadding(a.dp(4), t + a.dp(4), a.dp(4), a.dp(4))
        bottom.setPadding(a.dp(4), a.dp(2), a.dp(4), b + a.dp(4))
    }

    /** Recreates the chrome (after a theme change) while keeping the reading position. */
    fun rebuild() {
        val p = Settings.pal
        removeAllViews()
        setBackgroundColor(p.bg)
        page.paged = Settings.typo.optString("reading_mode") != "Scroll"
        addView(page, frame())
        page.relayout()

        top = a.row().apply { setBackgroundColor(p.surface) }
        top.addView(a.iconButton(R.drawable.ic_back, p.text, "Back to library") { a.closeReader() })
        top.addView(a.iconButton(R.drawable.ic_list, p.text, "Contents and bookmarks") { openContents() })
        top.addView(a.label(meta.getString("title"), 15f, p.text, bold = true, lines = 1).apply { gravity = Gravity.CENTER_VERTICAL },
            LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply { leftMargin = a.dp(4) })
        top.addView(a.iconButton(R.drawable.ic_search, p.text, "Search in this work") { openSearch() })
        top.addView(a.iconButton(R.drawable.ic_bookmark, p.text, "Bookmark this page") { addBookmark() })
        if (meta.optBoolean("ao3", true)) top.addView(a.iconButton(R.drawable.ic_heart, p.text, "Leave kudos on AO3") { kudos() })
        aloudButton = a.iconButton(if (reading) R.drawable.ic_stop else R.drawable.ic_volume, p.text, if (reading) "Stop reading aloud" else "Read aloud", reading) { toggleAloud() }
        top.addView(aloudButton)
        top.addView(a.iconButton(R.drawable.ic_text, p.text, "Text and theme") { openAppearance() })
        addView(top, frame(h = ViewGroup.LayoutParams.WRAP_CONTENT, gravity = Gravity.TOP))

        bottom = a.column().apply { setBackgroundColor(p.surface) }
        progressLine = View(a).apply { setBackgroundColor(p.accentMark) }
        val track = FrameLayout(a).apply { setBackgroundColor(p.border) }
        track.addView(progressLine, frame(0, a.dp(3)))
        bottom.addView(track, lp(h = a.dp(3)))
        val row = a.row()
        row.addView(a.iconButton(R.drawable.ic_prev, p.text, "Previous chapter") { if (chapter > 0) jumpChapter(chapter - 1, 0f) })
        status = a.label("", 12.5f, p.muted, lines = 2).apply { gravity = Gravity.CENTER }
        row.addView(status, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        row.addView(a.iconButton(R.drawable.ic_next, p.text, "Next chapter") { if (chapter + 1 < chapters.size) jumpChapter(chapter + 1, 0f) })
        bottom.addView(row)
        addView(bottom, frame(h = ViewGroup.LayoutParams.WRAP_CONTENT, gravity = Gravity.BOTTOM))
        bottom.gone(!Settings.typo.optBoolean("show_reading_progress_bar", true))

        setInsets(insetTop, insetBottom)
        showChrome(chrome)
        positionChanged()
    }

    // ---- chapters and position ----------------------------------------------------------

    private fun loadChapter(i: Int, anchor: Float) {
        chapter = i
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
            progressLine.layoutParams = FrameLayout.LayoutParams((w * pct / 100).toInt(), a.dp(3))
        }
        dirty = true
        handler.removeCallbacks(saveLater)
        handler.postDelayed(saveLater, 3000)
    }

    fun saveProgress() {
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
        tts?.speak(text, TextToSpeech.QUEUE_FLUSH, null, "b$block")
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
        (b.background as? android.graphics.drawable.RippleDrawable)?.setDrawable(0, if (reading) rounded(p.selected, a.dpf(22)) else null)
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
                if (reading && id == "b$aloudBlock") {
                    aloudWord = start until end
                    page.followAloud()
                    page.invalidate()
                }
            }
        }
    }

    /** The reader is closing: let the voice go. */
    fun close() {
        reading = false
        tts?.stop()
        tts?.shutdown()
        tts = null
    }

    // ---- chrome and panels --------------------------------------------------------------

    fun toggleChrome() {
        if (panel != null) return closePanel()
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
    fun back(): Boolean {
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
        fun fill() {
            tabsHolder.removeAllViews()
            tabsHolder.addView(a.segmented(listOf("Chapters", "Bookmarks (${bookmarks.length()})"), tab) { tab = it; fill() })
            if (tab == 0) {
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
        AlertDialog.Builder(a)
            .setTitle("Bookmark this page")
            .setView(FrameLayout(a).apply { setPadding(a.dp(20), a.dp(8), a.dp(20), 0); addView(input) })
            .setPositiveButton("Add") { _, _ ->
                bookmarks = Core.arr("bookmark.add", json("id" to workId, "chapter" to chapter + 1, "para" to page.anchor().toInt(), "note" to input.text.toString()))
                a.toast("Bookmark added")
            }
            .setNegativeButton("Cancel", null)
            .show()
    }

    private fun kudos() {
        a.toast("Sending kudos…")
        Core.background("kudos", json("id" to workId)) { r ->
            r.onSuccess { a.toast(it.toString()) }.onFailure { a.toast("Couldn't leave kudos: ${it.message}") }
        }
    }
}
