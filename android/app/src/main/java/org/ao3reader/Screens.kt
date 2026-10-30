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

private fun section(a: MainActivity, parent: LinearLayout, title: String) {
    parent.addView(a.label(title, 13.5f, Settings.pal.text, bold = true), lp(top = a.dp(18), bottom = a.dp(8)))
}

// ---- Fetch -----------------------------------------------------------------------------

fun fetchScreen(a: MainActivity): View = scroll(a, "Fetch a work") { col ->
    val p = Settings.pal
    val st = a.fetch
    val card = a.card()
    card.addView(a.label("AO3 link or work ID", 13f, p.text, bold = true))
    val inputRow = a.row()
    val input = a.input("archiveofourown.org/works/12345678")
    input.setText(st.input)
    input.imeOptions = EditorInfo.IME_ACTION_GO
    inputRow.addView(input, LinearLayout.LayoutParams(0, -2, 1f))
    inputRow.addView(a.button("Paste") {
        val clip = (a.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).primaryClip
        val text = clip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(a)?.toString()
        if (!text.isNullOrBlank()) input.setText(text.trim())
    }, lp(-2).apply { leftMargin = a.dp(8) })
    card.addView(inputRow, lp(top = a.dp(8)))

    fun go(open: Boolean) {
        st.input = input.text.toString().trim()
        if (st.input.isEmpty()) return
        a.startFetch(st.input, open)
    }
    input.setOnEditorActionListener { _, _, _ -> go(true); true }
    val buttons = a.row()
    val busy = st.busy != null
    buttons.addView(a.button("Download & read", Kind.Primary) { if (!busy) go(true) })
    buttons.addView(a.button("Save") { if (!busy) go(false) }, lp(-2).apply { leftMargin = a.dp(8) })
    buttons.addView(a.button("EPUB") { if (!busy) epub(a, input.text.toString()) }, lp(-2).apply { leftMargin = a.dp(8) })
    card.addView(buttons, lp(top = a.dp(12)))
    card.addView(a.button("Import a file…", Kind.Ghost) { if (!busy) import(a) }, lp(-2, top = a.dp(6)))
    if (busy) {
        val r = a.row()
        r.addView(ProgressBar(a), LinearLayout.LayoutParams(a.dp(22), a.dp(22)))
        r.addView(a.label(st.busy ?: "", 13f, p.muted).apply { setPadding(a.dp(10), 0, 0, 0) })
        card.addView(r, lp(top = a.dp(10)))
    }
    col.addView(card, lp(top = a.dp(4)))

    st.error?.let { err ->
        val box = a.column().apply {
            background = rounded(alpha(p.danger, 0.12f), a.dpf(12), alpha(p.danger, 0.6f), a.dp(1))
            setPadding(a.dp(16), a.dp(12), a.dp(16), a.dp(12))
        }
        box.addView(a.label("Couldn't download", 14f, p.danger, bold = true))
        box.addView(a.label(err, 13.5f, p.text), lp(top = a.dp(4)))
        val restricted = err.lowercase().let { "restricted" in it || "log in" in it }
        if (restricted) box.addView(a.button("Sign in to AO3") { a.show(2) }, lp(-2, top = a.dp(10)))
        col.addView(box, lp(top = a.dp(12)))
    }

    st.preview?.let { m ->
        val c = a.card()
        c.addView(a.label("Saved to your library", 12.5f, p.success, bold = true))
        c.addView(a.label(m.getString("title"), 18f, p.text, bold = true, lines = 2), lp(top = a.dp(6)))
        c.addView(a.label("by " + m.getJSONArray("authors").strings().joinToString(", "), 13.5f, p.muted), lp(top = a.dp(2)))
        val summary = m.getString("summary").split(Regex("\\s+")).joinToString(" ").trim()
        if (summary.isNotEmpty()) c.addView(a.label(summary, 13.5f, alpha(p.text, 0.85f), lines = 4), lp(top = a.dp(8)))
        c.addView(a.label("${thousands(m.getLong("words"))} words  ·  ${m.getInt("chapters")} chapters", 12.5f, p.muted), lp(top = a.dp(8)))
        c.addView(a.button("Open in reader", Kind.Primary) { a.openWork(m.getString("id")) }, lp(-2, top = a.dp(10)))
        col.addView(c, lp(top = a.dp(12)))
    }

    val tips = a.card()
    tips.addView(a.label("Tips", 12.5f, p.muted, bold = true))
    tips.addView(a.label("Paste a link to any chapter or to the work itself; the whole work is downloaded. “Import a file” opens EPUB books (including AO3's EPUB downloads) and AO3 HTML downloads. Works for registered users only need you to sign in under Account. Downloads are saved on this phone, so you can read them offline.", 13f, p.muted), lp(top = a.dp(4)))
    col.addView(tips, lp(top = a.dp(12)))
}

private fun epub(a: MainActivity, input: String) {
    val id = Regex("(\\d{3,})").find(input)?.value ?: return a.toast("That doesn't look like an AO3 work link or ID.")
    val tmp = File(a.cacheDir, "ao3_$id.epub")
    a.fetch.busy = "Downloading the EPUB from AO3…"
    a.fetch.error = null
    a.rebuild()
    Core.background("epub", json("id" to id, "path" to tmp.absolutePath)) { r ->
        a.fetch.busy = null
        r.onFailure { a.fetch.error = it.message; a.rebuild() }.onSuccess {
            a.rebuild()
            a.onDocument = { _, data ->
                data?.data?.let { uri -> a.contentResolver.openOutputStream(uri)?.use { out -> tmp.inputStream().use { it.copyTo(out) } } }
                tmp.delete()
                a.toast("EPUB saved")
            }
            @Suppress("DEPRECATION")
            a.startActivityForResult(Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
                addCategory(Intent.CATEGORY_OPENABLE)
                type = "application/epub+zip"
                putExtra(Intent.EXTRA_TITLE, "ao3_$id.epub")
            }, 2)
        }
    }
}

/** Imports an AO3 .html download or a saved .json work picked with the system file picker. */
private fun import(a: MainActivity) {
    a.onDocument = { _, data ->
        val uri = data?.data
        if (uri != null) {
            val tmp = File(a.cacheDir, "import.bin")
            a.contentResolver.openInputStream(uri)?.use { input -> tmp.outputStream().use { input.copyTo(it) } }
            a.fetch.busy = "Importing the file…"
            a.rebuild()
            Core.background("import", json("path" to tmp.absolutePath)) { r ->
                a.fetch.busy = null
                tmp.delete()
                r.onSuccess {
                    val o = it as org.json.JSONObject
                    a.fetch.preview = o.getJSONObject("meta")
                    a.openWork(o.getString("id"))
                }.onFailure { a.fetch.error = it.message; a.rebuild() }
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
        val msg = a.label("", 13f, p.danger)
        fun login() {
            if (user.text.isBlank() || pass.text.isEmpty()) return
            msg.setTextColor(p.muted)
            msg.text = "Signing in…"
            Core.background("login", json("user" to user.text.toString().trim(), "pass" to pass.text.toString())) { r ->
                r.onSuccess { a.toast("Signed in to AO3"); a.rebuild() }.onFailure { msg.setTextColor(p.danger); msg.text = it.message }
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
                r.onSuccess { a.toast("Signed in to AO3"); a.rebuild() }.onFailure { msg.setTextColor(p.danger); msg.text = it.message }
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

private fun slider(a: MainActivity, parent: LinearLayout, title: String, key: String, min: Float, max: Float, step: Float, fmt: (Float) -> String) {
    val p = Settings.pal
    val t = Settings.typo
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
        progressTintList = android.content.res.ColorStateList.valueOf(p.accent)
        thumbTintList = android.content.res.ColorStateList.valueOf(p.accent)
        setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
            override fun onProgressChanged(s: SeekBar?, v: Int, user: Boolean) {
                val f = min + v * step
                t.put(key, f.toDouble())
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
    return scroll(a, "Settings", "v$version") { col ->
        val p = Settings.pal
        val t = Settings.typo

        section(a, col, "Theme")
        val themes = a.column()
        Settings.themes.chunked(2).forEach { pair ->
            val r = a.row()
            pair.forEach { th ->
                val sel = th.id == Settings.pal.id
                val c = a.column().apply {
                    background = rounded(th.bg, a.dpf(12), if (sel) th.accent else p.border, a.dp(if (sel) 3 else 1))
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
            "A lightweight reader for Archive of Our Own: native Android views on top of the same Rust core as the desktop app.", null), lp(bottom = a.dp(6)))
    }
}
