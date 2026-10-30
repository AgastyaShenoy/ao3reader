package org.ao3reader

import android.app.AlertDialog
import android.content.Intent
import android.text.Editable
import android.text.TextWatcher
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.BaseAdapter
import android.widget.HorizontalScrollView
import android.widget.LinearLayout
import android.widget.ListView
import android.widget.PopupMenu
import android.widget.ProgressBar
import org.json.JSONObject

private val SHELVES = listOf("reading" to "Reading", "completed" to "Completed", "later" to "For later", "favorites" to "Favorites")
private val SORTS = listOf("Recently read", "Title", "Recently added", "Author")
private val EXPORTS = listOf(Triple("html", "Web page", "text/html"), Triple("md", "Markdown", "text/markdown"), Triple("txt", "Plain text", "text/plain"))

private fun inFilter(e: JSONObject, f: Int): Boolean = when (f) {
    0 -> true
    1 -> e.getString("shelf") == "reading"
    2 -> e.getString("shelf") == "completed"
    3 -> e.getString("shelf") == "later"
    else -> e.getBoolean("favorite") || e.getString("shelf") == "favorites"
}

private fun matches(e: JSONObject, q: String): Boolean {
    if (q.isBlank()) return true
    val m = e.getJSONObject("meta")
    val needle = q.trim().lowercase()
    val fields = listOf(m.getString("title")) + listOf("authors", "fandoms", "relationships", "characters", "tags").flatMap { m.getJSONArray(it).strings() }
    return fields.any { it.lowercase().contains(needle) }
}

fun libraryScreen(a: MainActivity): View {
    val p = Settings.pal
    val st = a.library
    val all = Core.arr("library").objects()
    val box = a.column()
    box.addView(a.header("Library", "${all.size} works"))

    val search = a.input("Search title, author, fandom, tag")
    search.setText(st.query)
    box.addView(search, lp(top = 0).apply { leftMargin = a.dp(16); rightMargin = a.dp(16) })

    val chips = a.row().apply { setPadding(a.dp(16), a.dp(10), a.dp(16), a.dp(6)) }
    box.addView(HorizontalScrollView(a).apply { isHorizontalScrollBarEnabled = false; addView(chips) }, lp())

    val list = ListView(a).apply {
        divider = null
        dividerHeight = a.dp(10)
        setPadding(a.dp(16), a.dp(6), a.dp(16), a.dp(16))
        clipToPadding = false
        selector = rounded(0, 0f)
    }
    val empty = a.column().apply { gravity = Gravity.CENTER_HORIZONTAL; setPadding(a.dp(24), a.dp(40), a.dp(24), 0) }
    box.addView(empty, lp())
    box.addView(list, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f))

    var shown = listOf<JSONObject>()
    val adapter = object : BaseAdapter() {
        override fun getCount() = shown.size
        override fun getItem(i: Int) = shown[i]
        override fun getItemId(i: Int) = i.toLong()
        override fun getView(i: Int, convert: View?, parent: ViewGroup?): View = workCard(a, shown[i])
    }
    list.adapter = adapter

    fun refresh() {
        var l = all.filter { inFilter(it, st.filter) && matches(it, st.query) }
        l = when (st.sort) {
            1 -> l.sortedBy { it.getJSONObject("meta").getString("title").lowercase() }
            2 -> l.sortedByDescending { it.getLong("added") }
            3 -> l.sortedBy { it.getJSONObject("meta").getJSONArray("authors").optString(0).lowercase() }
            else -> l
        }
        shown = l
        adapter.notifyDataSetChanged()
        empty.removeAllViews()
        empty.gone(l.isNotEmpty())
        if (all.isEmpty()) {
            empty.addView(a.label("Your library is empty. Download a work from AO3 to start reading.", 15f, p.muted).apply { gravity = Gravity.CENTER })
            empty.addView(a.button("Fetch a work", Kind.Primary) { a.show(1) }, lp(LinearLayout.LayoutParams.WRAP_CONTENT, top = a.dp(16)))
        } else if (l.isEmpty()) {
            empty.addView(a.label("No works match this search or filter.", 15f, p.muted).apply { gravity = Gravity.CENTER })
        }

        chips.removeAllViews()
        val names = listOf("All", "Reading", "Completed", "For later", "Favorites")
        names.forEachIndexed { f, n ->
            val sel = f == st.filter
            val chip = a.label("$n ${all.count { inFilter(it, f) }}", 13.5f, if (sel) p.onAccent else p.text, bold = sel).apply {
                background = if (sel) rounded(p.accent, a.dpf(16)) else rounded(0, a.dpf(16), p.border, a.dp(1))
                setPadding(a.dp(14), a.dp(7), a.dp(14), a.dp(7))
                setOnClickListener { st.filter = f; refresh() }
            }
            chips.addView(chip, lp(LinearLayout.LayoutParams.WRAP_CONTENT).apply { rightMargin = a.dp(8) })
        }
        val sort = a.label("⇅ ${SORTS[st.sort]}", 13.5f, p.text).apply {
            background = rounded(0, a.dpf(16), p.border, a.dp(1))
            setPadding(a.dp(14), a.dp(7), a.dp(14), a.dp(7))
        }
        sort.setOnClickListener {
            val m = PopupMenu(a, sort)
            SORTS.forEachIndexed { i, s -> m.menu.add(0, i, i, s).apply { isCheckable = true; isChecked = i == st.sort } }
            m.setOnMenuItemClickListener { st.sort = it.itemId; refresh(); true }
            m.show()
        }
        chips.addView(sort)
    }

    search.addTextChangedListener(object : TextWatcher {
        override fun beforeTextChanged(s: CharSequence?, a1: Int, b: Int, c: Int) {}
        override fun onTextChanged(s: CharSequence?, a1: Int, b: Int, c: Int) {}
        override fun afterTextChanged(s: Editable?) {
            st.query = s?.toString() ?: ""
            refresh()
        }
    })
    refresh()
    return box
}

private fun workCard(a: MainActivity, e: JSONObject): View {
    val p = Settings.pal
    val m = e.getJSONObject("meta")
    val id = m.getString("id")
    val card = a.card()

    val top = a.row()
    top.addView(View(a).apply { background = rounded(Pal.opaque(m.getInt("rating_color")), a.dpf(2)) }, LinearLayout.LayoutParams(a.dp(4), a.dp(22)).apply { rightMargin = a.dp(10) })
    top.addView(a.label(m.getString("title"), 17f, p.text, bold = true, lines = 2).apply { setOnClickListener { a.openWork(id) } },
        LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
    val fav = e.getBoolean("favorite")
    top.addView(a.iconButton(if (fav) R.drawable.ic_heart_fill else R.drawable.ic_heart, if (fav) 0xFFE11D48.toInt() else p.muted) {
        Core.req("library.favorite", json("id" to id))
        a.rebuild()
    })
    card.addView(top)

    val fandoms = m.getJSONArray("fandoms").strings().take(2)
    val by = "by " + m.getJSONArray("authors").strings().joinToString(", ") + if (fandoms.isNotEmpty()) "  ·  " + fandoms.joinToString(", ") else ""
    card.addView(a.label(by, 13f, p.muted, lines = 1), lp(top = a.dp(2)))

    val pills = a.row()
    for (c in m.getJSONArray("categories").objects()) pills.addView(a.pill(c.getString("name"), Pal.opaque(c.getInt("color")), p.onAccent), pillLp(a))
    for (w in m.getJSONArray("warnings").strings().filter { !it.contains("No Archive Warnings") }) pills.addView(a.pill(w, 0xFFB42828.toInt(), p.onAccent), pillLp(a))
    for (r in m.getJSONArray("relationships").strings().take(3)) pills.addView(a.pill(r, p.accent, p.text, outlined = true), pillLp(a))
    pills.addView(a.pill(if (m.getBoolean("complete")) "Complete" else "In progress", p.border, p.muted, outlined = true), pillLp(a))
    card.addView(HorizontalScrollView(a).apply { isHorizontalScrollBarEnabled = false; addView(pills) }, lp(top = a.dp(10)))

    val summary = m.getString("summary").split(Regex("\\s+")).joinToString(" ").trim()
    if (summary.isNotEmpty()) card.addView(a.label(summary, 13.5f, alpha(p.text, 0.85f), lines = 3).apply { setLineSpacing(0f, 1.15f) }, lp(top = a.dp(10)))

    val started = e.getInt("chapter") > 1 || e.getDouble("percentage") > 0 || e.getDouble("position") > 0
    val completed = e.getBoolean("completed")
    val pct = e.getDouble("percentage")
    val status = when {
        completed -> "Finished"
        started -> "Chapter ${e.getInt("chapter")} of ${m.getInt("chapters")}  ·  ${pct.toInt()}%"
        else -> "${m.getInt("chapters")} chapters  ·  not started"
    }
    val prog = a.row()
    prog.addView(a.label(status, 12.5f, alpha(p.text, 0.85f)))
    prog.addView(ProgressBar(a, null, android.R.attr.progressBarStyleHorizontal).apply {
        max = 1000
        progress = (pct * 10).toInt()
        progressTintList = android.content.res.ColorStateList.valueOf(p.accent)
        progressBackgroundTintList = android.content.res.ColorStateList.valueOf(p.border)
    }, LinearLayout.LayoutParams(0, a.dp(4), 1f).apply { leftMargin = a.dp(12) })
    card.addView(prog, lp(top = a.dp(12)))
    card.addView(a.label("${thousands(m.getLong("words"))} words" + if (m.getLong("kudos") > 0) "  ·  ${thousands(m.getLong("kudos"))} kudos" else "", 12f, p.muted), lp(top = a.dp(4)))

    val actions = a.row()
    actions.addView(a.button(if (completed) "Read again" else if (started) "Resume" else "Read", Kind.Primary) { a.openWork(id) })
    val shelf = SHELVES.firstOrNull { it.first == e.getString("shelf") }?.second ?: "Reading"
    val shelfBtn = a.button(shelf) {}
    shelfBtn.setOnClickListener {
        val menu = PopupMenu(a, shelfBtn)
        SHELVES.forEachIndexed { i, (key, name) -> menu.menu.add(0, i, i, name).apply { isCheckable = true; isChecked = key == e.getString("shelf") } }
        menu.setOnMenuItemClickListener {
            Core.req("library.shelf", json("id" to id, "shelf" to SHELVES[it.itemId].first))
            a.rebuild()
            true
        }
        menu.show()
    }
    actions.addView(shelfBtn, lp(LinearLayout.LayoutParams.WRAP_CONTENT).apply { leftMargin = a.dp(8) })
    actions.addView(View(a), LinearLayout.LayoutParams(0, 1, 1f))
    lateinit var more: View
    more = a.iconButton(R.drawable.ic_more, p.text) {
        val menu = PopupMenu(a, more)
        EXPORTS.forEachIndexed { i, (_, name, _) -> menu.menu.add(0, i, i, "Export as $name") }
        menu.menu.add(0, 99, 99, "Delete from library")
        menu.setOnMenuItemClickListener {
            if (it.itemId == 99) confirmDelete(a, id, m.getString("title")) else export(a, id, it.itemId)
            true
        }
        menu.show()
    }
    actions.addView(more)
    card.addView(actions, lp(top = a.dp(12)))
    return card
}

private fun pillLp(a: MainActivity) = lp(LinearLayout.LayoutParams.WRAP_CONTENT).apply { rightMargin = a.dp(6) }

private fun confirmDelete(a: MainActivity, id: String, title: String) {
    AlertDialog.Builder(a)
        .setTitle("Delete this work?")
        .setMessage("“$title” and its downloaded text will be removed from your library.")
        .setPositiveButton("Delete") { _, _ ->
            Core.req("library.delete", json("id" to id))
            a.toast("Removed “$title”")
            a.rebuild()
        }
        .setNegativeButton("Cancel", null)
        .show()
}

/** Exports through the system "save as" picker. */
private fun export(a: MainActivity, id: String, format: Int) {
    val (ext, _, mime) = EXPORTS[format]
    val res = try {
        Core.obj("export", json("id" to id, "format" to ext))
    } catch (e: CoreError) {
        a.toast(e.message ?: "Export failed")
        return
    }
    val name = res.getString("title").replace(Regex("[\\\\/:*?\"<>|]"), "_").take(100) + ".$ext"
    a.onDocument = { _, data ->
        val uri = data?.data
        if (uri != null) {
            runCatching { a.contentResolver.openOutputStream(uri)?.use { it.write(res.getString("text").toByteArray()) } }
                .onSuccess { a.toast("Saved $name") }.onFailure { a.toast("Couldn't save: ${it.message}") }
        }
    }
    @Suppress("DEPRECATION")
    a.startActivityForResult(Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
        addCategory(Intent.CATEGORY_OPENABLE)
        type = mime
        putExtra(Intent.EXTRA_TITLE, name)
    }, 1)
}
