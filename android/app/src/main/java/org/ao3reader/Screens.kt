package org.ao3reader

import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.view.Gravity
import android.view.View
import android.view.inputmethod.EditorInfo
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.ScrollView
import android.widget.SeekBar
import android.widget.Switch
import java.io.File

private fun scroll(a: MainActivity, title: String, sub: String? = null, body: (LinearLayout) -> Unit): View {
    val col = a.column().apply { setPadding(a.dp(16), 0, a.dp(16), a.dp(24)) }
    val outer = a.column()
    outer.addView(a.header(title, sub))
    body(col)
    outer.addView(ScrollView(a).apply { addView(col) }, LinearLayout.LayoutParams(-1, 0, 1f))
    return outer
}

/** "45 minutes", "3 h 20 min". */
private fun readingTime(minutes: Int): String = when {
    minutes <= 1 -> "a minute"
    minutes < 60 -> "$minutes minutes"
    minutes % 60 == 0 -> "${minutes / 60} h"
    else -> "${minutes / 60} h ${minutes % 60} min"
}

private fun section(a: MainActivity, parent: LinearLayout, title: String) {
    parent.addView(a.label(title, 13.5f, Settings.pal.text, bold = true), lp(top = a.dp(18), bottom = a.dp(8)))
}

// ---- Import ---------------------------------------------------------------------------

fun importScreen(a: MainActivity): View = scroll(a, "Import") { col ->
    val p = Settings.pal
    val st = a.imports
    val card = a.card()
    card.addView(a.label("Add a work you already have as a file. It goes into your library and reads like any downloaded work, offline.", 14f, p.muted).apply { setLineSpacing(0f, 1.15f) })
    for ((name, what) in listOf(
        "EPUB books" to "Including the EPUB files from AO3's Download button.",
        "PDFs" to "Shown page by page, as they were made.",
        "AO3 web pages" to "AO3's HTML downloads, or a work's page saved from a browser.",
        "Flax Reader works" to "Works saved by this app (.json), including from AO3 Reader.",
    )) {
        val r = a.row().apply { gravity = Gravity.TOP }
        r.addView(a.icon(R.drawable.ic_check, p.success, 18), LinearLayout.LayoutParams(a.dp(18), a.dp(18)).apply { topMargin = a.dp(2); rightMargin = a.dp(12) })
        val t = a.column()
        t.addView(a.label(name, 15f, p.text, bold = true))
        t.addView(a.label(what, 13f, p.muted))
        r.addView(t)
        card.addView(r, lp(top = a.dp(12)))
    }
    val pick = a.row()
    pick.addView(a.button("Choose a file…", Kind.Primary, R.drawable.ic_import) { if (!st.busy) import(a) })
    if (st.busy) {
        pick.addView(ProgressBar(a), LinearLayout.LayoutParams(a.dp(22), a.dp(22)).apply { leftMargin = a.dp(14) })
        pick.addView(a.label("Importing…", 13.5f, p.muted).apply { setPadding(a.dp(8), 0, 0, 0) })
    }
    card.addView(pick, lp(top = a.dp(16)))
    col.addView(card, lp(top = a.dp(4)))

    st.error?.let { col.addView(a.notice("Couldn't import that file", it), lp(top = a.dp(12))) }
    if (st.error == null) st.done?.let { m ->
        val c = a.card()
        val r = a.row()
        val t = a.column()
        t.addView(a.label("Imported", 12.5f, p.success, bold = true))
        t.addView(a.label(m.getString("title"), 15f, p.text, bold = true, lines = 2))
        r.addView(t, LinearLayout.LayoutParams(0, -2, 1f))
        r.addView(a.button("Open", Kind.Tonal, R.drawable.ic_book) { a.openWork(m.getString("id")) })
        c.addView(r)
        col.addView(c, lp(top = a.dp(12)))
    }

    val export = a.card()
    export.addView(a.label("Save a work as a file", 15f, p.text))
    export.addView(a.label("Use ⋮ on a work in your Library to export it as a web page, Markdown or plain text.", 13f, p.muted), lp(top = a.dp(2)))
    col.addView(export, lp(top = a.dp(18)))
}

/** Imports an EPUB, an AO3 .html download or a saved .json work picked with the system file picker. */
private fun import(a: MainActivity) {
    val st = a.imports
    a.onDocument = { _, data ->
        val uri = data?.data
        if (uri != null) {
            val tmp = File(a.cacheDir, "import.bin")
            a.contentResolver.openInputStream(uri)?.use { input -> tmp.outputStream().use { input.copyTo(it) } }
            st.busy = true
            st.error = null
            a.rebuild()
            Core.background("import", json("path" to tmp.absolutePath, "name" to a.displayName(uri))) { r ->
                st.busy = false
                tmp.delete()
                r.onSuccess {
                    st.done = (it as org.json.JSONObject).getJSONObject("meta")
                    a.toast("Imported “${st.done?.optString("title")}”")
                }.onFailure { st.error = it.message }
                if (a.currentTab == MainActivity.IMPORT) a.rebuild()
            }
        }
    }
    @Suppress("DEPRECATION")
    a.startActivityForResult(Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
        addCategory(Intent.CATEGORY_OPENABLE)
        type = "*/*"
        putExtra(Intent.EXTRA_MIME_TYPES, arrayOf("application/epub+zip", "application/pdf", "text/html", "application/json", "application/octet-stream", "application/zip"))
    }, 3)
}

// ---- Account ---------------------------------------------------------------------------

fun accountScreen(a: MainActivity): View = scroll(a, "Account") { col ->
    val p = Settings.pal
    val s = Core.obj("session")
    val signedIn = s.getBoolean("signed_in")
    val status = a.card()
    status.addView(a.label(when {
        signedIn -> "Signed in as ${s.optString("username").ifEmpty { "an AO3 user" }}"
        s.getBoolean("stale") -> "Sign-in expired"
        else -> "Reading as a guest"
    }, 16f, p.text, bold = true))
    status.addView(a.label(when {
        signedIn -> "Works restricted to registered users will download."
        s.getBoolean("stale") -> "This sign-in came from an older version that didn't keep the session. Please sign in again."
        else -> "Public works download without an account."
    }, 13f, p.muted), lp(top = a.dp(4)))
    if (signedIn || s.getBoolean("stale")) status.addView(a.button("Sign out") {
        Core.req("logout")
        a.toast("Signed out")
        a.rebuild()
    }, lp(-2, top = a.dp(10)))
    col.addView(status, lp(top = a.dp(4)))

    if (!signedIn) {
        val form = a.card()
        form.addView(a.label("Sign in to AO3", 15f, p.text, bold = true))
        val user = a.input("Username or email")
        val pass = a.input("Password", password = true)
        form.addView(user, lp(top = a.dp(12)))
        form.addView(pass, lp(top = a.dp(8)))
        val msg = a.label("", 13f, p.dangerText)
        fun login() {
            if (user.text.isBlank() || pass.text.isEmpty()) return
            msg.setTextColor(p.muted)
            msg.text = "Signing in…"
            Core.background("login", json("user" to user.text.toString().trim(), "pass" to pass.text.toString())) { r ->
                r.onSuccess { a.toast("Signed in to AO3"); a.rebuild() }.onFailure { msg.setTextColor(p.dangerText); msg.text = it.message }
            }
        }
        pass.imeOptions = EditorInfo.IME_ACTION_DONE
        pass.setOnEditorActionListener { _, _, _ -> login(); true }
        form.addView(a.button("Sign in", Kind.Primary) { login() }, lp(-2, top = a.dp(12)))
        form.addView(msg, lp(top = a.dp(8)))

        form.addView(a.label("Or use a session cookie", 13.5f, p.text, bold = true), lp(top = a.dp(16)))
        val cookie = a.input("Value of the _otw_remember_me cookie", password = true)
        form.addView(cookie, lp(top = a.dp(8)))
        form.addView(a.button("Apply cookie") {
            if (cookie.text.isBlank()) return@button
            msg.setTextColor(p.muted)
            msg.text = "Checking the cookie with AO3…"
            Core.background("cookie", json("value" to cookie.text.toString())) { r ->
                r.onSuccess { a.toast("Signed in to AO3"); a.rebuild() }.onFailure { msg.setTextColor(p.dangerText); msg.text = it.message }
            }
        }, lp(-2, top = a.dp(8)))
        col.addView(form, lp(top = a.dp(12)))
    }

    val privacy = a.card()
    privacy.addView(a.label("Privacy", 12.5f, p.muted, bold = true))
    privacy.addView(a.label("Your password is sent only to archiveofourown.org and is never saved. After signing in, AO3's session cookie is kept in this app's private storage so you stay signed in; signing out removes it.", 13f, p.muted), lp(top = a.dp(4)))
    col.addView(privacy, lp(top = a.dp(12)))
}

// ---- Settings --------------------------------------------------------------------------

private fun settingRow(a: MainActivity, title: String, sub: String?, control: View?): View {
    val p = Settings.pal
    val r = a.row().apply {
        background = rounded(p.card, a.dpf(10), p.border, a.dp(1))
        setPadding(a.dp(16), a.dp(12), a.dp(12), a.dp(12))
        minimumHeight = a.dp(56)
    }
    val text = a.column()
    text.addView(a.label(title, 15f, p.text))
    if (sub != null) text.addView(a.label(sub, 12.5f, p.muted))
    r.addView(text, LinearLayout.LayoutParams(0, -2, 1f))
    if (control != null) r.addView(control)
    return r
}

private fun toggle(a: MainActivity, parent: LinearLayout, title: String, sub: String?, key: String, inTypo: Boolean = true, then: (() -> Unit)? = null) {
    val obj = if (inTypo) Settings.typo else Settings.config
    val sw = Switch(a).apply {
        isChecked = obj.optBoolean(key)
        contentDescription = title
        setOnCheckedChangeListener { _, v -> obj.put(key, v); Settings.save(); then?.invoke() }
        val p = Settings.pal
        thumbTintList = android.content.res.ColorStateList(arrayOf(intArrayOf(android.R.attr.state_checked), intArrayOf()), intArrayOf(p.onAccent, p.muted))
        trackTintList = android.content.res.ColorStateList(arrayOf(intArrayOf(android.R.attr.state_checked), intArrayOf()), intArrayOf(p.accent, p.border))
    }
    parent.addView(settingRow(a, title, sub, sw), lp(bottom = a.dp(6)))
}

private fun slider(a: MainActivity, parent: LinearLayout, title: String, key: String, min: Float, max: Float, step: Float, inTypo: Boolean = true, whole: Boolean = !inTypo, fmt: (Float) -> String) {
    val p = Settings.pal
    val t = if (inTypo) Settings.typo else Settings.config
    val box = a.column().apply {
        background = rounded(p.card, a.dpf(10), p.border, a.dp(1))
        setPadding(a.dp(16), a.dp(12), a.dp(16), a.dp(8))
    }
    val head = a.row()
    head.addView(a.label(title, 15f, p.text), LinearLayout.LayoutParams(0, -2, 1f))
    val value = a.label(fmt(t.optDouble(key).toFloat()), 13f, p.muted)
    head.addView(value)
    box.addView(head)
    val steps = ((max - min) / step).toInt()
    box.addView(SeekBar(a).apply {
        this.max = steps
        contentDescription = title
        progress = Math.round((t.optDouble(key, min.toDouble()).toFloat() - min) / step).coerceIn(0, steps)
        progressTintList = android.content.res.ColorStateList.valueOf(p.accentMark)
        thumbTintList = android.content.res.ColorStateList.valueOf(p.accentMark)
        setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
            override fun onProgressChanged(s: SeekBar?, v: Int, user: Boolean) {
                val f = min + v * step
                if (whole) t.put(key, f.toInt()) else t.put(key, f.toDouble())
                value.text = fmt(f)
            }
            override fun onStartTrackingTouch(s: SeekBar?) {}
            override fun onStopTrackingTouch(s: SeekBar?) = Settings.save()
        })
    }, lp(top = a.dp(4)))
    parent.addView(box, lp(bottom = a.dp(6)))
}

/** Picks a "Your colours" colour with red, green and blue sliders, then makes it the theme. */
private fun pickColour(a: MainActivity, title: String, key: String) {
    val p = Settings.pal
    var c = Settings.config.optInt(key) and 0xFFFFFF
    val box = a.column()
    val swatch = View(a)
    val hex = a.label("", 14f, p.text)
    fun show() {
        swatch.background = rounded(Pal.opaque(c), a.dpf(10), p.border, a.dp(1))
        hex.text = "#%06X".format(c)
    }
    box.addView(swatch, lp(h = a.dp(56), bottom = a.dp(8)))
    box.addView(hex, lp(bottom = a.dp(6)))
    for ((name, shift) in listOf("Red" to 16, "Green" to 8, "Blue" to 0)) {
        box.addView(a.label(name, 12.5f, p.muted))
        box.addView(SeekBar(a).apply {
            max = 255
            progress = (c shr shift) and 0xFF
            contentDescription = name
            setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
                override fun onProgressChanged(s: SeekBar?, v: Int, u: Boolean) {
                    c = (c and (0xFF shl shift).inv()) or (v shl shift)
                    show()
                }
                override fun onStartTrackingTouch(s: SeekBar?) {}
                override fun onStopTrackingTouch(s: SeekBar?) {}
            })
        }, lp(bottom = a.dp(6)))
    }
    show()
    a.dialog(title, body = box, ok = "Use") {
        Settings.config.put(key, c)
        Settings.save()
        Settings.reloadThemes()
        // "Your colours" becomes the theme (with Match system, for the mode it suits).
        val custom = Settings.themes.firstOrNull { it.id == "Custom" }
        if (Settings.config.optBoolean("follow_system") && custom != null) Settings.config.put(if (custom.dark) "dark_theme" else "light_theme", "Custom")
        else Settings.config.put("theme", "Custom")
        Settings.save()
        a.rebuild()
        true
    }
}

const val DISCORD_INVITE = "https://discord.gg/3xuJY8UXKu"
const val GITHUB = "https://github.com/AgastyaShenoy/flax-reader"

/** Settings' categories, most used first: icon, name, what's in it. */
private val CATEGORIES = listOf(
    Triple(R.drawable.ic_palette, "Appearance", "Colour themes, and your own colours"),
    Triple(R.drawable.ic_text, "Text", "Font, size and spacing of text in books"),
    Triple(R.drawable.ic_book, "Reading", "Pages or scrolling, chapters and reading speed"),
    Triple(R.drawable.ic_accessibility, "Accessibility", "Reading aids, easier text and reading aloud"),
    Triple(R.drawable.ic_import, "Library & files", "Bring in books you have, and storage"),
    Triple(R.drawable.ic_globe, "Sources", "AO3, Project Gutenberg and your catalogues"),
    Triple(R.drawable.ic_info, "About", "Flax Reader, its community, privacy and credits"),
)

private fun version(a: MainActivity): String = a.packageManager.getPackageInfo(a.packageName, 0).versionName ?: ""

fun settingsScreen(a: MainActivity): View {
    val page = a.settingsPage
    if (page !in CATEGORIES.indices) return settingsList(a)
    return settingsPage(a, CATEGORIES[page].second) { col ->
        when (page) {
            0 -> appearancePage(a, col)
            1 -> textPage(a, col)
            2 -> readingPage(a, col)
            3 -> accessibilityPage(a, col)
            4 -> libraryPage(a, col)
            5 -> sourcesPage(a, col)
            else -> aboutPage(a, col)
        }
    }
}

/** The categories, each opening its own page. */
private fun settingsList(a: MainActivity): View = scroll(a, "Settings") { col ->
    val p = Settings.pal
    val card = a.column().apply {
        background = rounded(p.card, a.dpf(14), p.border, a.dp(1))
        clipToOutline = true
    }
    CATEGORIES.forEachIndexed { i, (icon, name, what) ->
        if (i > 0) card.addView(View(a).apply { setBackgroundColor(p.border) }, lp(h = a.dp(1)).apply { leftMargin = a.dp(66) })
        val r = a.row().apply {
            setPadding(a.dp(14), a.dp(13), a.dp(10), a.dp(13))
            background = android.graphics.drawable.RippleDrawable(android.content.res.ColorStateList.valueOf(alpha(p.text, 0.12f)), null, android.graphics.drawable.ColorDrawable(0xFFFFFFFF.toInt()))
            contentDescription = "$name. $what"
            setOnClickListener { a.openSettings(i) }
        }
        val iconBox = android.widget.FrameLayout(a).apply { background = rounded(p.selected, a.dpf(11)) }
        iconBox.addView(a.icon(icon, p.text, 22), frame(a.dp(22), a.dp(22), Gravity.CENTER))
        r.addView(iconBox, LinearLayout.LayoutParams(a.dp(40), a.dp(40)).apply { rightMargin = a.dp(14) })
        val t = a.column()
        t.addView(a.label(name, 15.5f, p.text, lines = 1))
        t.addView(a.label(what, 12.5f, p.muted, lines = 2), lp(top = a.dp(1)))
        r.addView(t, LinearLayout.LayoutParams(0, -2, 1f))
        r.addView(a.icon(R.drawable.ic_next, p.muted, 20))
        card.addView(r)
    }
    col.addView(card, lp(top = a.dp(4)))
    col.addView(a.label("Flax Reader ${version(a)}", 12.5f, p.muted).apply { gravity = Gravity.CENTER }, lp(top = a.dp(18)))
}

/** One category's page: back to the list, its name, then its settings. */
private fun settingsPage(a: MainActivity, title: String, body: (LinearLayout) -> Unit): View {
    val p = Settings.pal
    val col = a.column().apply { setPadding(a.dp(16), 0, a.dp(16), a.dp(24)) }
    val outer = a.column()
    val head = a.row().apply { setPadding(a.dp(6), a.dp(12), a.dp(16), a.dp(2)) }
    head.addView(a.iconButton(R.drawable.ic_back, p.text, "Back to Settings") { a.openSettings(-1) })
    head.addView(a.label(title, 22f, p.text, bold = true, lines = 1).apply { setPadding(a.dp(6), 0, 0, 0) })
    outer.addView(head)
    body(col)
    outer.addView(ScrollView(a).apply { addView(col) }, LinearLayout.LayoutParams(-1, 0, 1f))
    return outer
}

private fun appearancePage(a: MainActivity, col: LinearLayout) {
    val p = Settings.pal
    section(a, col, "Theme")
    val follow = Settings.config.optBoolean("follow_system")
    toggle(a, col, "Match system", if (follow) "Pick a light theme and a dark theme; the phone's dark mode chooses between them." else "Switch between a light and a dark theme with the phone's dark mode.", "follow_system", inTypo = false) {
        // Start from the theme in use for whichever mode it suits.
        if (Settings.config.optBoolean("follow_system")) {
            val cur = Settings.themes.firstOrNull { it.id == Settings.config.optString("theme") }
            if (cur != null) Settings.config.put(if (cur.dark) "dark_theme" else "light_theme", cur.id)
            Settings.save()
        }
        a.rebuild()
    }
    val themes = a.column()
    Settings.themes.chunked(2).forEach { pair ->
        val r = a.row()
        pair.forEach { th ->
            // Following the system, each card can be the light or the dark theme.
            val slot = if (th.dark) "dark_theme" else "light_theme"
            val sel = if (follow) Settings.config.optString(slot) == th.id else th.id == Settings.pal.id
            val c = a.column().apply {
                background = rounded(th.bg, a.dpf(12), if (sel) p.text else p.border, a.dp(if (sel) 2 else 1))
                setPadding(a.dp(14), a.dp(12), a.dp(14), a.dp(12))
                contentDescription = th.name + if (sel) ", chosen" else ""
                setOnClickListener {
                    Settings.config.put(if (follow) slot else "theme", th.id)
                    Settings.save()
                    a.rebuild()
                }
            }
            c.addView(a.label(th.name, 15f, th.text, bold = true))
            c.addView(a.label(if (follow && sel) (if (th.dark) "Dark mode" else "Light mode") else "The quick brown fox", 12.5f, th.muted, lines = 1))
            c.addView(View(a).apply { background = rounded(th.accent, a.dpf(3)) }, LinearLayout.LayoutParams(a.dp(40), a.dp(6)).apply { topMargin = a.dp(10) })
            r.addView(c, LinearLayout.LayoutParams(0, -2, 1f).apply { rightMargin = a.dp(6); leftMargin = a.dp(6) })
        }
        themes.addView(r, lp(bottom = a.dp(10)))
    }
    col.addView(themes)

    section(a, col, "Your colours")
    for ((key, title) in listOf("custom_bg" to "Page colour", "custom_text" to "Text colour")) {
        val v = Settings.config.optInt(key)
        val sw = View(a).apply { background = rounded(Pal.opaque(v), a.dpf(8), p.border, a.dp(1)) }
        val ctl = a.row()
        ctl.addView(sw, LinearLayout.LayoutParams(a.dp(30), a.dp(30)).apply { rightMargin = a.dp(10) })
        ctl.addView(a.button("Choose…") { pickColour(a, title, key) })
        col.addView(settingRow(a, title, "#%06X".format(v and 0xFFFFFF), ctl), lp(bottom = a.dp(6)))
    }
    val ratio = contrast(Settings.config.optInt("custom_bg"), Settings.config.optInt("custom_text"))
    col.addView(a.label(
        if (ratio >= 4.5) "Contrast %.1f:1, easy to read.".format(ratio) else "Contrast %.1f:1 is too low to read, so the text is adjusted until it reaches 4.5:1.".format(ratio),
        12.5f, if (ratio >= 4.5) p.muted else p.warningText,
    ).apply { setPadding(a.dp(4), 0, a.dp(4), 0) }, lp(bottom = a.dp(6)))
}

private fun readingPage(a: MainActivity, col: LinearLayout) {
    val p = Settings.pal
    val t = Settings.typo
    section(a, col, "Pages")
    val pagedNow = t.optString("reading_mode") != "Scroll"
    col.addView(settingRow(a, "Layout", "Turn pages, or scroll through each chapter.",
        a.segmented(listOf("Pages", "Scroll"), if (pagedNow) 0 else 1) { i -> t.put("reading_mode", if (i == 0) "Paged" else "Scroll"); Settings.save(); a.rebuild() }), lp(bottom = a.dp(6)))
    toggle(a, col, "Progress bar", "Chapter, page and time left under the text.", "show_reading_progress_bar")
    toggle(a, col, "Animations", "Page turns when you swipe, and screen and menu transitions. Tapping the sides always turns instantly.", "animations", inTypo = false)

    section(a, col, "Chapter content")
    toggle(a, col, "Chapter summaries", null, "show_chapter_summary")
    toggle(a, col, "Author notes before chapters", null, "show_chapter_pre_notes")
    toggle(a, col, "Author notes after chapters", null, "show_chapter_post_notes")

    section(a, col, "Reading speed")
    val speed = Core.obj("speed")
    val manual = Settings.config.optInt("words_per_minute", 230)
    val learned = if (speed.isNull("measured")) null else speed.getInt("measured")
    val speedSub = if (learned != null) "About $learned words a minute, learned from ${readingTime(speed.getInt("minutes"))} of your reading. Used for the time left in a work."
    else "Learned from how fast you turn pages, after a few minutes of reading. Until then, $manual words a minute is used."
    val auto = Settings.config.optBoolean("auto_reading_speed", true)
    val autoSwitch = Switch(a).apply {
        isChecked = auto
        thumbTintList = android.content.res.ColorStateList(arrayOf(intArrayOf(android.R.attr.state_checked), intArrayOf()), intArrayOf(p.onAccent, p.muted))
        trackTintList = android.content.res.ColorStateList(arrayOf(intArrayOf(android.R.attr.state_checked), intArrayOf()), intArrayOf(p.accent, p.border))
        setOnCheckedChangeListener { _, v -> Settings.config.put("auto_reading_speed", v); Settings.save(); a.rebuild() }
    }
    col.addView(settingRow(a, "Work out my reading speed", speedSub, autoSwitch), lp(bottom = a.dp(6)))
    if (!auto) slider(a, col, "Reading speed", "words_per_minute", 120f, 500f, 10f, inTypo = false) { "${it.toInt()} wpm" }
    else if (speed.optBoolean("has_data")) col.addView(settingRow(a, "Start over", "Forget the reading measured so far, e.g. if someone else has been reading.", a.button("Reset") {
        Core.req("speed.reset")
        a.rebuild()
    }), lp(bottom = a.dp(6)))
}

private fun textPage(a: MainActivity, col: LinearLayout) {
    val p = Settings.pal
    val t = Settings.typo
    section(a, col, "Font")
    val fonts = listOf("SansSerif" to "Sans", "Serif" to "Serif", "Monospace" to "Mono")
    val curFont = fonts.indexOfFirst { it.first == t.opt("font_family") }.coerceAtLeast(0)
    col.addView(settingRow(a, "Font", "Easy-to-read fonts are in Accessibility.", a.segmented(fonts.map { it.second }, curFont) { i ->
        t.put("font_family", fonts[i].first); Settings.save(); a.rebuild()
    }.apply { layoutParams = LinearLayout.LayoutParams(a.dp(210), -2) }), lp(bottom = a.dp(6)))
    slider(a, col, "Text size", "font_size", 12f, 36f, 1f) { "${it.toInt()}" }

    section(a, col, "Spacing and alignment")
    slider(a, col, "Line height", "line_spacing_multiplier", 1.2f, 2.6f, 0.05f) { "%.2f×".format(it) }
    slider(a, col, "Space between paragraphs", "paragraph_spacing", 4f, 32f, 1f) { "${it.toInt()}" }
    val justified = t.optString("text_alignment") == "Justified"
    col.addView(settingRow(a, "Alignment", null, a.segmented(listOf("Left", "Justified"), if (justified) 1 else 0) { i ->
        t.put("text_alignment", if (i == 1) "Justified" else "Left"); Settings.save(); a.rebuild()
    }.apply { layoutParams = LinearLayout.LayoutParams(a.dp(190), -2) }), lp(bottom = a.dp(6)))
    toggle(a, col, "Indent first line of paragraphs", null, "indent_paragraphs")

    // The app's own interface font; most people never change it.
    section(a, col, "Interface font")
    val faces = listOf<Pair<String?, String>>(null to "Default", "serif" to "Serif", "monospace" to "Mono", "sans-serif-condensed" to "Narrow")
    val curFace = faces.indexOfFirst { it.first == Settings.uiFamily }.coerceAtLeast(0)
    val faceBox = a.column().apply {
        background = rounded(p.card, a.dpf(10), p.border, a.dp(1))
        setPadding(a.dp(16), a.dp(12), a.dp(12), a.dp(12))
    }
    faceBox.addView(a.label("Menus, buttons and labels. Default keeps the phone's own font.", 12.5f, p.muted))
    faceBox.addView(a.segmented(faces.map { it.second }, curFace) { i ->
        t.put("ui_font", faces[i].first ?: org.json.JSONObject.NULL); Settings.save(); a.rebuild()
    }, lp(top = a.dp(10)))
    col.addView(faceBox, lp(bottom = a.dp(6)))
}

private fun accessibilityPage(a: MainActivity, col: LinearLayout) {
    val p = Settings.pal
    val t = Settings.typo
    section(a, col, "Easier text")
    val easy = listOf("SansSerif" to "Default", "Atkinson" to "Atkinson", "OpenDyslexic" to "OpenDyslexic")
    val easyBox = a.column().apply {
        background = rounded(p.card, a.dpf(10), p.border, a.dp(1))
        setPadding(a.dp(16), a.dp(12), a.dp(12), a.dp(12))
    }
    easyBox.addView(a.label("Easy-to-read font", 15f, p.text))
    easyBox.addView(a.label("Atkinson Hyperlegible keeps similar letters distinct; OpenDyslexic weights the bottom of each letter.", 12.5f, p.muted))
    easyBox.addView(a.segmented(easy.map { it.second }, easy.indexOfFirst { it.first == t.opt("font_family") }) { i ->
        t.put("font_family", easy[i].first); Settings.save(); a.rebuild()
    }, lp(top = a.dp(10)))
    col.addView(easyBox, lp(bottom = a.dp(6)))
    val pct = { v: Float -> if (v < 0.005f) "Normal" else "+${Math.round(v * 100)}%" }
    slider(a, col, "Space between letters", "letter_spacing", 0f, 0.3f, 0.01f, fmt = pct)
    slider(a, col, "Space between words", "word_spacing", 0f, 1f, 0.05f, fmt = pct)

    section(a, col, "Reading aids")
    toggle(a, col, "Reading ruler", "Highlights one line and dims the rest. Drag up or down on the page to move it.", "reading_ruler")
    toggle(a, col, "Bold word starts", "Bolds the first letters of each word, to lead the eye along the line.", "bold_word_starts")
    toggle(a, col, "Focus mode", "Hides the running heads and page numbers: just the text.", "focus_mode")

    section(a, col, "Colour")
    // Page tint: a coloured overlay.
    val tintBox = a.column().apply {
        background = rounded(p.card, a.dpf(10), p.border, a.dp(1))
        setPadding(a.dp(16), a.dp(12), a.dp(12), a.dp(12))
    }
    tintBox.addView(a.label("Page tint", 15f, p.text))
    tintBox.addView(a.label("A soft colour over the page, like a coloured overlay.", 12.5f, p.muted))
    val tints = listOf(0xFFE680 to "Yellow", 0xFFB37A to "Peach", 0xF59AB4 to "Rose", 0x7FB2F0 to "Blue", 0x8ED39A to "Green", 0x9A9A9A to "Grey")
    val current = if (t.isNull("tint") || !t.has("tint")) null else t.optInt("tint")
    val swatches = a.row()
    swatches.addView(a.button("None", if (current == null) Kind.Secondary else Kind.Ghost) { t.put("tint", org.json.JSONObject.NULL); Settings.save(); a.rebuild() })
    for ((rgb, name) in tints) {
        swatches.addView(View(a).apply {
            background = rounded(Pal.opaque(rgb), a.dpf(17), if (current == rgb) p.text else 0, a.dp(if (current == rgb) 3 else 0))
            contentDescription = "$name tint" + if (current == rgb) ", chosen" else ""
            setOnClickListener { t.put("tint", rgb); Settings.save(); a.rebuild() }
        }, LinearLayout.LayoutParams(a.dp(34), a.dp(34)).apply { leftMargin = a.dp(8) })
    }
    tintBox.addView(a.pillScroller(swatches), lp(top = a.dp(10)))
    col.addView(tintBox, lp(bottom = a.dp(6)))
    if (current != null) slider(a, col, "Tint strength", "tint_strength", 0.05f, 0.5f, 0.01f) { "${Math.round(it * 100)}%" }
    col.addView(settingRow(a, "High contrast and your own colours", "In Appearance.", a.button("Open") { a.openSettings(0) }), lp(bottom = a.dp(6)))

    section(a, col, "Interface")
    toggle(a, col, "Larger touch targets", "Bigger buttons with more room between them.", "larger_targets", inTypo = false) { a.rebuild() }
    val scales = listOf(0.9f, 1f, 1.1f, 1.25f, 1.5f)
    // The size the app is drawn at now (kept where the activity reads it at start-up).
    val applied = a.getSharedPreferences("ui", android.content.Context.MODE_PRIVATE).getFloat("scale", 1f)
    val curScale = scales.indexOfFirst { Math.abs(it - applied) < 0.01f }.let { if (it < 0) 1 else it }
    val scaleBox = a.column().apply {
        background = rounded(p.card, a.dpf(10), p.border, a.dp(1))
        setPadding(a.dp(16), a.dp(12), a.dp(12), a.dp(12))
    }
    scaleBox.addView(a.label("Interface size", 15f, p.text))
    scaleBox.addView(a.label("Menus, buttons and labels, on top of the phone's own size settings. Text in books has its own size.", 12.5f, p.muted))
    scaleBox.addView(a.segmented(listOf("90%", "100%", "110%", "125%", "150%"), curScale) { i -> a.setUiScale(scales[i]) }, lp(top = a.dp(10)))
    col.addView(scaleBox, lp(bottom = a.dp(6)))

    section(a, col, "Read aloud")
    slider(a, col, "Read-aloud speed", "speech_rate", 0.5f, 2f, 0.05f, inTypo = false, whole = false) { "%.2f×".format(it) }
}

private fun libraryPage(a: MainActivity, col: LinearLayout) {
    section(a, col, "Import")
    col.addView(settingRow(a, "Import a file", "EPUB books, PDFs, AO3 web pages and works saved by this app.", a.button("Open") { a.show(MainActivity.IMPORT) }), lp(bottom = a.dp(6)))

    section(a, col, "Storage")
    val dir = Core.obj("storage").getString("dir")
    col.addView(settingRow(a, "Library folder", dir, null), lp(bottom = a.dp(6)))
    col.addView(settingRow(a, "Clear downloaded works", "Frees space. Library entries and reading progress are kept.", a.button("Clear…") {
        a.dialog("Remove all downloaded works?", "Saved chapter text is deleted. Library entries and reading progress stay.", ok = "Remove", okKind = Kind.Danger) {
            Core.req("clear_cache")
            a.toast("Downloaded works removed")
            true
        }
    }), lp(bottom = a.dp(6)))
}

private fun sourcesPage(a: MainActivity, col: LinearLayout) {
    section(a, col, "Archive of Our Own")
    val session = Core.obj("session")
    val signedIn = session.getBoolean("signed_in")
    val who = if (signedIn) "Signed in as ${session.optString("username").ifEmpty { "an AO3 user" }}. Works only members can see, and kudos, use your account."
    else "Not signed in. Sign in to read works only members can see, and to leave kudos."
    col.addView(settingRow(a, "AO3 account", who, a.button(if (signedIn) "Manage" else "Sign in") { a.show(MainActivity.ACCOUNT) }), lp(bottom = a.dp(6)))

    section(a, col, "Catalogues")
    val list = runCatching { Core.arr("catalogs").objects() }.getOrDefault(emptyList())
    list.forEachIndexed { i, c ->
        val name = c.getString("name")
        if (c.optBoolean("builtin")) {
            col.addView(settingRow(a, name, "Built in: tens of thousands of free books whose copyright has run out.", null), lp(bottom = a.dp(6)))
        } else {
            col.addView(settingRow(a, name, c.getString("url"), a.button("Remove", Kind.Ghost) {
                a.dialog("Remove $name?", "Books you got from it stay in your library.", ok = "Remove", okKind = Kind.Danger) {
                    a.catalogs.catalogs = Core.arr("catalog.remove", json("index" to i)).objects()
                    a.catalogs.switch(0)
                    a.rebuild()
                    true
                }
            }), lp(bottom = a.dp(6)))
        }
    }
    col.addView(settingRow(a, "Add a catalogue", "Calibre's content server, Kavita, Komga, a library's catalogue: any OPDS feed.", a.button("Add") { addCatalogDialog(a) }), lp(bottom = a.dp(6)))

    section(a, col, "Dictionary")
    col.addView(settingRow(a, "Looking up words", "Press and hold a word in a book, then Define. Definitions come from Wiktionary, so this needs the internet.", null), lp(bottom = a.dp(6)))
}

private fun aboutPage(a: MainActivity, col: LinearLayout) {
    val p = Settings.pal
    val hero = a.card()
    val top = a.row()
    top.addView(android.widget.ImageView(a).apply {
        setImageResource(R.drawable.ic_flax)
        imageTintList = android.content.res.ColorStateList.valueOf(p.accentMark)
        importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
    }, LinearLayout.LayoutParams(a.dp(48), a.dp(48)).apply { rightMargin = a.dp(14) })
    val names = a.column()
    names.addView(a.label("Flax Reader", 21f, p.text, bold = true))
    names.addView(a.label("Version ${version(a)}  ·  Made by aga2522", 12.5f, p.muted))
    names.addView(a.label("Free and open source (MIT)", 12.5f, p.muted))
    top.addView(names, LinearLayout.LayoutParams(0, -2, 1f))
    hero.addView(top)
    hero.addView(a.label("A small, quick e-reader for your own books and free ones. Read EPUBs and PDFs, browse Project Gutenberg and your own catalogues, and read Archive of Our Own, with themes, fonts and reading aids made for long, comfortable reading.", 14f, p.text).apply { setLineSpacing(0f, 1.15f) }, lp(top = a.dp(12)))
    val buttons = a.row()
    buttons.addView(a.button("Join the Discord", Kind.Primary, R.drawable.ic_chat) { openUrl(a, DISCORD_INVITE) })
    buttons.addView(a.button("GitHub", Kind.Secondary, R.drawable.ic_code) { openUrl(a, GITHUB) }, LinearLayout.LayoutParams(-2, -2).apply { leftMargin = a.dp(8) })
    hero.addView(buttons, lp(top = a.dp(14)))
    hero.addView(a.label("The Discord is aga2522's general server: come and chat about anything, and ask about Flax Reader there too.", 12.5f, p.muted), lp(top = a.dp(10)))
    col.addView(hero, lp(top = a.dp(8)))

    section(a, col, "Help")
    col.addView(settingRow(a, "Report a problem", "Something broken or confusing? Tell us on GitHub, or ask in the Discord.", a.button("Report", Kind.Secondary, R.drawable.ic_bug) { openUrl(a, "$GITHUB/issues/new") }), lp(bottom = a.dp(6)))
    col.addView(settingRow(a, "What's new", "Release notes for every version.", a.button("Releases", Kind.Secondary, R.drawable.ic_history) { openUrl(a, "$GITHUB/releases") }), lp(bottom = a.dp(6)))

    section(a, col, "Privacy")
    col.addView(settingRow(a, "Your books stay yours", "Your library, highlights and notes stay on this phone. There are no ads, no tracking and no Flax account. The app goes online only for what you ask: AO3, catalogues and word definitions.", null), lp(bottom = a.dp(6)))

    section(a, col, "This app")
    col.addView(MemoryCard(a), lp(bottom = a.dp(6)))

    section(a, col, "Credits")
    col.addView(settingRow(a, "Thanks to", "Project Gutenberg for its free books, and Wiktionary for definitions (CC BY-SA). Atkinson Hyperlegible is by the Braille Institute and OpenDyslexic by Abbie Gonzalez, both under the SIL Open Font License.", null), lp(bottom = a.dp(6)))
    col.addView(settingRow(a, "Not affiliated with AO3", "Flax Reader is unofficial: it isn't made by Archive of Our Own or the Organization for Transformative Works.", null), lp(bottom = a.dp(6)))
}
