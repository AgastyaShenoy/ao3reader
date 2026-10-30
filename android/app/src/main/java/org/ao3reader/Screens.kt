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
        "AO3 web pages" to "AO3's HTML downloads, or a work's page saved from a browser.",
        "AO3 Reader works" to "Works saved by this app (.json).",
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
            Core.background("import", json("path" to tmp.absolutePath)) { r ->
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
        putExtra(Intent.EXTRA_MIME_TYPES, arrayOf("application/epub+zip", "text/html", "application/json", "application/octet-stream", "application/zip"))
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

private fun toggle(a: MainActivity, parent: LinearLayout, title: String, sub: String?, key: String, inTypo: Boolean = true) {
    val obj = if (inTypo) Settings.typo else Settings.config
    val sw = Switch(a).apply {
        isChecked = obj.optBoolean(key)
        setOnCheckedChangeListener { _, v -> obj.put(key, v); Settings.save() }
        val p = Settings.pal
        thumbTintList = android.content.res.ColorStateList(arrayOf(intArrayOf(android.R.attr.state_checked), intArrayOf()), intArrayOf(p.onAccent, p.muted))
        trackTintList = android.content.res.ColorStateList(arrayOf(intArrayOf(android.R.attr.state_checked), intArrayOf()), intArrayOf(p.accent, p.border))
    }
    parent.addView(settingRow(a, title, sub, sw), lp(bottom = a.dp(6)))
}

private fun slider(a: MainActivity, parent: LinearLayout, title: String, key: String, min: Float, max: Float, step: Float, inTypo: Boolean = true, fmt: (Float) -> String) {
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
        progress = ((t.optDouble(key).toFloat() - min) / step).toInt().coerceIn(0, steps)
        progressTintList = android.content.res.ColorStateList.valueOf(p.accentMark)
        thumbTintList = android.content.res.ColorStateList.valueOf(p.accentMark)
        setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
            override fun onProgressChanged(s: SeekBar?, v: Int, user: Boolean) {
                val f = min + v * step
                if (inTypo) t.put(key, f.toDouble()) else t.put(key, f.toInt())
                value.text = fmt(f)
            }
            override fun onStartTrackingTouch(s: SeekBar?) {}
            override fun onStopTrackingTouch(s: SeekBar?) = Settings.save()
        })
    }, lp(top = a.dp(4)))
    parent.addView(box, lp(bottom = a.dp(6)))
}

fun settingsScreen(a: MainActivity): View {
    val version = a.packageManager.getPackageInfo(a.packageName, 0).versionName
    return scroll(a, "Settings") { col ->
        val p = Settings.pal
        val t = Settings.typo

        section(a, col, "Account")
        val session = Core.obj("session")
        val who = if (session.getBoolean("signed_in")) "Signed in as ${session.optString("username").ifEmpty { "an AO3 user" }}" else "Not signed in"
        col.addView(settingRow(a, "AO3 account", who, a.button(if (session.getBoolean("signed_in")) "Manage" else "Sign in") { a.show(MainActivity.ACCOUNT) }), lp(bottom = a.dp(6)))

        section(a, col, "Theme")
        val themes = a.column()
        Settings.themes.chunked(2).forEach { pair ->
            val r = a.row()
            pair.forEach { th ->
                val sel = th.id == Settings.pal.id
                val c = a.column().apply {
                    background = rounded(th.bg, a.dpf(12), if (sel) p.text else p.border, a.dp(if (sel) 2 else 1))
                    setPadding(a.dp(14), a.dp(12), a.dp(14), a.dp(12))
                    setOnClickListener {
                        Settings.config.put("theme", th.id)
                        Settings.save()
                        a.rebuild()
                    }
                }
                c.addView(a.label(th.name, 15f, th.text, bold = true))
                c.addView(a.label("The quick brown fox", 12.5f, th.muted, lines = 1))
                c.addView(View(a).apply { background = rounded(th.accent, a.dpf(3)) }, LinearLayout.LayoutParams(a.dp(40), a.dp(6)).apply { topMargin = a.dp(10) })
                r.addView(c, LinearLayout.LayoutParams(0, -2, 1f).apply { rightMargin = a.dp(6); leftMargin = a.dp(6) })
            }
            themes.addView(r, lp(bottom = a.dp(10)))
        }
        col.addView(themes)

        section(a, col, "Reading")
        val pagedNow = t.optString("reading_mode") != "Scroll"
        col.addView(settingRow(a, "Layout", "Turn pages, or scroll through each chapter.",
            a.segmented(listOf("Pages", "Scroll"), if (pagedNow) 0 else 1) { i -> t.put("reading_mode", if (i == 0) "Paged" else "Scroll"); Settings.save(); a.rebuild() }), lp(bottom = a.dp(6)))
        toggle(a, col, "Chapter summaries", null, "show_chapter_summary")
        toggle(a, col, "Author notes before chapters", null, "show_chapter_pre_notes")
        toggle(a, col, "Author notes after chapters", null, "show_chapter_post_notes")
        toggle(a, col, "Progress bar", "Chapter, page and time left under the text.", "show_reading_progress_bar")
        toggle(a, col, "Animations", "Page turns when you swipe, and screen and menu transitions. Tapping the sides always turns instantly.", "animations", inTypo = false)

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

        section(a, col, "Text")
        val fonts = listOf("SansSerif" to "Sans", "Serif" to "Serif", "Monospace" to "Mono")
        val curFont = fonts.indexOfFirst { it.first == t.opt("font_family") }.coerceAtLeast(0)
        col.addView(settingRow(a, "Font", null, a.segmented(fonts.map { it.second }, curFont) { i ->
            t.put("font_family", fonts[i].first); Settings.save(); a.rebuild()
        }.apply { layoutParams = LinearLayout.LayoutParams(a.dp(210), -2) }), lp(bottom = a.dp(6)))
        slider(a, col, "Text size", "font_size", 12f, 36f, 1f) { "${it.toInt()}" }
        slider(a, col, "Line height", "line_spacing_multiplier", 1.2f, 2.6f, 0.05f) { "%.2f×".format(it) }
        slider(a, col, "Space between paragraphs", "paragraph_spacing", 4f, 32f, 1f) { "${it.toInt()}" }
        val justified = t.optString("text_alignment") == "Justified"
        col.addView(settingRow(a, "Alignment", null, a.segmented(listOf("Left", "Justified"), if (justified) 1 else 0) { i ->
            t.put("text_alignment", if (i == 1) "Justified" else "Left"); Settings.save(); a.rebuild()
        }.apply { layoutParams = LinearLayout.LayoutParams(a.dp(190), -2) }), lp(bottom = a.dp(6)))
        toggle(a, col, "Indent first line of paragraphs", null, "indent_paragraphs")

        // The app's own interface font; most people never change it.
        section(a, col, "Advanced")
        val faces = listOf<Pair<String?, String>>(null to "Default", "serif" to "Serif", "monospace" to "Mono", "sans-serif-condensed" to "Narrow")
        val curFace = faces.indexOfFirst { it.first == Settings.uiFamily }.coerceAtLeast(0)
        val faceBox = a.column().apply {
            background = rounded(p.card, a.dpf(10), p.border, a.dp(1))
            setPadding(a.dp(16), a.dp(12), a.dp(12), a.dp(12))
        }
        faceBox.addView(a.label("Interface font", 15f, p.text))
        faceBox.addView(a.label("Menus, buttons and labels. Default keeps the phone's own font.", 12.5f, p.muted))
        faceBox.addView(a.segmented(faces.map { it.second }, curFace) { i ->
            t.put("ui_font", faces[i].first ?: org.json.JSONObject.NULL); Settings.save(); a.rebuild()
        }, lp(top = a.dp(10)))
        col.addView(faceBox, lp(bottom = a.dp(6)))

        section(a, col, "Import")
        col.addView(settingRow(a, "Import a file", "EPUB books, AO3 web pages and works saved by this app.", a.button("Open") { a.show(MainActivity.IMPORT) }), lp(bottom = a.dp(6)))

        section(a, col, "Storage")
        val dir = Core.obj("storage").getString("dir")
        col.addView(settingRow(a, "Library folder", dir, null), lp(bottom = a.dp(6)))
        col.addView(settingRow(a, "Clear downloaded works", "Frees space. Library entries and reading progress are kept.", a.button("Clear…") {
            android.app.AlertDialog.Builder(a).setTitle("Remove all downloaded works?")
                .setMessage("Saved chapter text is deleted. Library entries and reading progress stay.")
                .setPositiveButton("Remove") { _, _ -> Core.req("clear_cache"); a.toast("Downloaded works removed") }
                .setNegativeButton("Cancel", null).show()
        }), lp(bottom = a.dp(6)))

        section(a, col, "Memory")
        col.addView(MemoryCard(a), lp(bottom = a.dp(6)))

        section(a, col, "About")
        col.addView(settingRow(a, "AO3 Reader $version",
            "A lightweight, unofficial reader for Archive of Our Own. Find works and save them to read offline, turn pages like a book or scroll, choose your own fonts and themes, and follow the authors and tags you love. It's small and quick, and your library stays on this phone.\n\nNot affiliated with the Organization for Transformative Works. The AO3 logo is their trademark.", null), lp(bottom = a.dp(6)))
    }
}
