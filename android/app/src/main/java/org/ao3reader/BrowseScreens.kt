package org.ao3reader

import android.app.AlertDialog
import android.content.Intent
import android.net.Uri
import android.text.Editable
import android.text.InputType
import android.text.TextWatcher
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import android.widget.BaseAdapter
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.HorizontalScrollView
import android.widget.LinearLayout
import android.widget.ListView
import android.widget.PopupMenu
import android.widget.ProgressBar
import android.widget.ScrollView
import android.widget.Switch
import org.json.JSONArray
import org.json.JSONObject

// Search AO3 (or open a pasted work link) and follow authors, tags and collections. AO3 was
// down for maintenance while these screens were built, so they have only been tried against
// sample pages.

/** A pasted AO3 work link: the work's details, shown before anything is downloaded. */
class LinkState(val id: String) {
    var meta: JSONObject? = null
    var inLibrary = false
    /** What the card is waiting for, as a message. */
    var busy: String? = null
    var error: String? = null
    /** Downloaded to the library from this card. */
    var saved = false
}

/** The work id when the search box holds a link to an AO3 work (a bare number is searched for). */
private fun workLink(text: String): String? =
    if ("works/" in text) Regex("works/(\\d+)").find(text)?.groupValues?.get(1) else null

class SearchState {
    /** The core's SearchQuery as JSON. */
    var query = JSONObject()
    var link: LinkState? = null
    var filtersOpen = false
    var formScroll = 0
    var animateForm = false
    val results = mutableListOf<JSONObject>()
    var total: Long? = null
    var page = 0
    var hasNext = false
    var busy = false
    var error: String? = null
    var scroll = 0
}

class FollowingState {
    /** "author", "tag" or "collection": which tab is showing. */
    var kind = "author"
    /** Feed id shown on its own, or null for every feed of the tab. */
    var selected: String? = null
    var checking = false
    var error: String? = null
    val more = mutableListOf<JSONObject>()
    var morePage = 1
    var moreNext = true
    var moreBusy = false
    var scroll = 0
}

private val SORTS = listOf(
    "best_match" to "Best match", "updated" to "Date updated", "posted" to "Date posted", "kudos" to "Kudos",
    "hits" to "Hits", "bookmarks" to "Bookmarks", "comments" to "Comments", "words" to "Word count",
    "title" to "Title", "author" to "Author",
)
private val RATINGS = listOf(null to "Any", "General" to "G", "Teen" to "T", "Mature" to "M", "Explicit" to "E", "NotRated" to "NR")
private val CATEGORIES = listOf("F/F", "F/M", "Gen", "M/M", "Multi", "Other")

private fun filterCount(q: JSONObject): Int {
    var n = 0
    for (k in listOf("title", "creators", "fandoms", "relationships", "characters", "tags", "language")) if (q.optString(k).isNotBlank()) n++
    if (!q.isNull("rating") && q.has("rating")) n++
    if ((q.optJSONArray("categories")?.length() ?: 0) > 0) n++
    if (q.optString("complete", "any") != "any") n++
    if (q.optString("crossovers", "any") != "any") n++
    if (q.optBoolean("single_chapter")) n++
    if (q.has("words_min") && !q.isNull("words_min")) n++
    if (q.has("words_max") && !q.isNull("words_max")) n++
    return n
}

private fun chip(a: MainActivity, text: String, selected: Boolean, onClick: (View) -> Unit): View {
    val p = Settings.pal
    return a.label(text, 13.5f, p.text, bold = selected, lines = 1).apply {
        background = if (selected) rounded(p.selected, a.dpf(16)) else rounded(0, a.dpf(16), p.border, a.dp(1))
        setPadding(a.dp(14), a.dp(7), a.dp(14), a.dp(7))
        setOnClickListener { onClick(it) }
    }
}

private fun chipLp(a: MainActivity) = lp(LinearLayout.LayoutParams.WRAP_CONTENT).apply { rightMargin = a.dp(8) }

private fun hideKeyboard(a: MainActivity, v: View) {
    v.clearFocus()
    a.getSystemService(InputMethodManager::class.java)?.hideSoftInputFromWindow(v.windowToken, 0)
}

private fun errorCard(a: MainActivity, title: String, msg: String): View = a.notice(title, msg)

private fun listView(a: MainActivity) = ListView(a).apply {
    divider = null
    dividerHeight = a.dp(10)
    setPadding(a.dp(16), a.dp(6), a.dp(16), a.dp(16))
    clipToPadding = false
    selector = rounded(0, 0f)
}

private fun adapter(items: () -> List<JSONObject>, view: (JSONObject) -> View) = object : BaseAdapter() {
    override fun getCount() = items().size
    override fun getItem(i: Int) = items()[i]
    override fun getItemId(i: Int) = i.toLong()
    override fun getView(i: Int, convert: View?, parent: ViewGroup?): View = view(items()[i])
}

// ---- Search -------------------------------------------------------------------------------

fun searchScreen(a: MainActivity): View {
    val p = Settings.pal
    val st = a.search
    val q = st.query
    val box = a.column()
    box.addView(a.header("Search AO3", st.total?.takeIf { st.link == null }?.let { "${thousands(it)} works" }))

    val row = a.row().apply { setPadding(a.dp(16), 0, a.dp(8), 0) }
    val input = a.input("Search AO3, or paste a link to a work")
    input.setText(q.optString("query"))
    input.imeOptions = EditorInfo.IME_ACTION_SEARCH
    row.addView(input, LinearLayout.LayoutParams(0, -2, 1f))
    box.addView(row)

    // Under the search box: the results, or the work behind a pasted link. Swapped in place, so
    // typing isn't interrupted by a redraw of the whole screen.
    val body = FrameLayout(a)
    lateinit var showBody: () -> Unit

    fun lookUp(id: String) {
        val link = LinkState(id)
        st.link = link
        val local = Core.arr("library").objects().firstOrNull { it.getJSONObject("meta").getString("id") == id }
        if (local != null) {
            link.meta = local.getJSONObject("meta")
            link.inLibrary = true
        } else {
            link.busy = "Looking up the work on AO3…"
            Core.background("ao3.work", json("input" to id)) { r ->
                if (st.link !== link) return@background
                link.busy = null
                r.onSuccess {
                    val o = it as JSONObject
                    link.meta = o.getJSONObject("meta")
                    link.inLibrary = o.getBoolean("in_library")
                    link.error = null
                }.onFailure { link.error = it.message }
                if (a.currentTab == MainActivity.SEARCH) a.rebuild()
            }
        }
        st.filtersOpen = false
        hideKeyboard(a, input)
        showBody()
    }

    // Keep what is typed, even before searching, across redraws (sorting, filters). A pasted
    // work link is looked up once typing pauses, so one typed by hand isn't looked up digit by digit.
    var pending: Runnable? = null
    input.addTextChangedListener(object : TextWatcher {
        override fun beforeTextChanged(s: CharSequence?, x: Int, y: Int, z: Int) {}
        override fun onTextChanged(s: CharSequence?, x: Int, y: Int, z: Int) {}
        override fun afterTextChanged(s: Editable?) {
            val text = s?.toString() ?: ""
            q.put("query", text)
            pending?.let { input.removeCallbacks(it) }
            val id = workLink(text)
            if (id == null) {
                if (st.link != null) { st.link = null; showBody() }
            } else if (id != st.link?.id) {
                pending = Runnable { lookUp(id) }.also { input.postDelayed(it, 400) }
            }
        }
    })

    fun run(page: Int) {
        if (st.busy) return
        val text = input.text.toString().trim()
        workLink(text)?.let { id ->
            // Enter on a link looks it up (again, if that failed).
            val l = st.link
            if (l == null || l.id != id || (l.busy == null && l.meta == null)) lookUp(id)
            return
        }
        q.put("query", text)
        if (q.optString("query").isBlank() && filterCount(q) == 0) return a.toast("Type something to search for, or set a filter.")
        hideKeyboard(a, input)
        st.busy = true
        st.error = null
        st.filtersOpen = false
        if (page == 1) st.scroll = 0
        a.rebuild()
        Core.background("ao3.search", json("query" to q, "page" to page)) { r ->
            st.busy = false
            r.onSuccess {
                val o = it as JSONObject
                if (page == 1) st.results.clear()
                st.results += o.getJSONArray("works").objects()
                st.total = if (o.isNull("total")) null else o.getLong("total")
                st.page = page
                st.hasNext = o.getBoolean("has_next")
            }.onFailure { st.error = it.message }
            if (a.currentTab == MainActivity.SEARCH) a.rebuild()
        }
    }
    row.addView(a.iconButton(R.drawable.ic_search, p.text) { run(1) })
    input.setOnEditorActionListener { _, _, _ -> run(1); true }

    // Filters, sort and direction: three equally wide buttons.
    val tools = a.row().apply { setPadding(a.dp(16), a.dp(10), a.dp(16), a.dp(6)) }
    fun toolLp(first: Boolean) = LinearLayout.LayoutParams(0, -2, 1f).apply { if (!first) leftMargin = a.dp(8) }
    val n = filterCount(q)
    tools.addView(a.toolButton(if (n > 0) "Filters · $n" else "Filters", R.drawable.ic_filter, st.filtersOpen || n > 0) {
        q.put("query", input.text.toString())
        st.filtersOpen = !st.filtersOpen
        st.animateForm = st.filtersOpen
        st.formScroll = 0
        a.rebuild()
    }, toolLp(true))
    val sortKey = q.optString("sort", "best_match")
    tools.addView(a.toolButton(SORTS.firstOrNull { it.first == sortKey }?.second ?: "Best match", R.drawable.ic_sort, false) { v ->
        val m = PopupMenu(a, v)
        SORTS.forEachIndexed { i, (k, label) -> m.menu.add(0, i, i, label).apply { isCheckable = true; isChecked = k == sortKey } }
        m.setOnMenuItemClickListener {
            q.put("sort", SORTS[it.itemId].first)
            if (st.results.isNotEmpty()) run(1) else a.rebuild()
            true
        }
        m.show()
    }, toolLp(false))
    val asc = q.optBoolean("ascending")
    tools.addView(a.toolButton(if (asc) "Ascending" else "Descending", if (asc) R.drawable.ic_arrow_up else R.drawable.ic_arrow_down, false) {
        q.put("ascending", !asc)
        if (st.results.isNotEmpty()) run(1) else a.rebuild()
    }, toolLp(false))
    box.addView(tools, lp())

    if (st.filtersOpen) {
        val form = filtersForm(a, q) { run(1) }
        // Choices redraw the form; keep it where it was scrolled to.
        form.setOnScrollChangeListener { _, _, y, _, _ -> st.formScroll = y }
        form.post { form.scrollTo(0, st.formScroll) }
        box.addView(form, LinearLayout.LayoutParams(-1, 0, 1f))
        if (st.animateForm) Anim.enter(form, dy = -a.dpf(12), ms = 170)
        st.animateForm = false
        return box
    }

    fun results(): View {
        val list = listView(a)
        val status = a.column()
        when {
            st.busy && st.results.isEmpty() -> status.addView(spinnerRow(a, "Searching AO3…"))
            st.error != null -> status.addView(errorCard(a, "Search didn't work", st.error!!), lp(bottom = a.dp(6)))
            st.results.isEmpty() && st.total == 0L -> status.addView(a.label("No works found. Try fewer filters or other words.", 14.5f, p.muted).apply { setPadding(a.dp(4), a.dp(20), a.dp(4), 0) })
            st.results.isEmpty() -> status.addView(searchTips(a))
        }
        list.addHeaderView(status, null, false)
        if (st.hasNext || (st.busy && st.results.isNotEmpty())) {
            val more = FrameLayout(a).apply { setPadding(0, a.dp(4), 0, a.dp(8)) }
            if (st.busy) more.addView(spinnerRow(a, "Loading more…")) else more.addView(a.button("Load more") { st.scroll = list.firstVisiblePosition; run(st.page + 1) }, frame(-1, -2))
            list.addFooterView(more, null, false)
        }
        val followed = followedKeys()
        list.adapter = adapter({ st.results }) { w -> blurbCard(a, w, followed) }
        list.setSelection(st.scroll)
        list.setOnScrollListener(object : android.widget.AbsListView.OnScrollListener {
            override fun onScrollStateChanged(v: android.widget.AbsListView?, s: Int) {}
            override fun onScroll(v: android.widget.AbsListView?, first: Int, visible: Int, total: Int) { st.scroll = first }
        })
        return list
    }

    showBody = {
        body.removeAllViews()
        val link = st.link
        body.addView(if (link != null) ScrollView(a).apply { addView(linkCard(a, link, { showBody() }, { lookUp(link.id) })) } else results(), frame())
    }
    showBody()
    box.addView(body, LinearLayout.LayoutParams(-1, 0, 1f))
    return box
}

/** The work behind a pasted link: its details, and buttons to read, save or get AO3's EPUB.
 *  `refresh` redraws the card; `retry` looks the link up again. */
private fun linkCard(a: MainActivity, link: LinkState, refresh: () -> Unit, retry: () -> Unit): View {
    val p = Settings.pal
    val col = a.column().apply { setPadding(a.dp(16), a.dp(6), a.dp(16), a.dp(24)) }
    val id = link.id
    val m = link.meta
    if (m == null) {
        val err = link.error
        if (err == null) {
            col.addView(spinnerRow(a, link.busy ?: "Looking up the work on AO3…"))
            return col
        }
        val box = a.notice("Couldn't open this link", err)
        val buttons = a.row().apply { setPadding(a.dp(28), 0, 0, 0) }
        buttons.addView(a.button("Try again", iconRes = R.drawable.ic_refresh) { retry() })
        val restricted = err.lowercase().let { "restricted" in it || "sign in" in it }
        if (restricted) buttons.addView(a.button("Sign in to AO3") { a.show(MainActivity.ACCOUNT) }, lp(-2).apply { leftMargin = a.dp(8) })
        box.addView(buttons, lp(top = a.dp(10)))
        col.addView(box)
        return col
    }

    val title = m.getString("title")
    val c = a.card().apply { setPadding(a.dp(18), a.dp(16), a.dp(18), a.dp(16)) }
    val (caption, capColor) = when {
        link.saved -> "Saved to your library" to p.success
        link.inLibrary -> "In your library" to p.success
        else -> "AO3 work" to p.muted
    }
    c.addView(a.label(caption, 12.5f, capColor, bold = true))
    c.addView(a.label(title + if (m.optBoolean("restricted")) "  🔒" else "", 20f, p.text, bold = true, lines = 3), lp(top = a.dp(4)))
    val fandoms = m.getJSONArray("fandoms").strings().take(3)
    val by = "by " + m.getJSONArray("authors").strings().joinToString(", ") + if (fandoms.isNotEmpty()) "  ·  " + fandoms.joinToString(", ") else ""
    c.addView(a.label(by, 14f, p.muted, lines = 3), lp(top = a.dp(2)))
    c.addView(a.pillScroller(workPills(a, m, ships = 8)), lp(top = a.dp(12)))
    val tags = m.getJSONArray("characters").strings().take(6) + m.getJSONArray("tags").strings().take(12)
    if (tags.isNotEmpty()) c.addView(a.label(tags.joinToString(" · "), 13f, p.muted, lines = 4).apply { setLineSpacing(0f, 1.15f) }, lp(top = a.dp(10)))
    val summary = m.getString("summary").split(Regex("\\s+")).joinToString(" ").trim()
    if (summary.isNotEmpty()) c.addView(a.label(summary, 14.5f, alpha(p.text, 0.9f), lines = 12).apply {
        setLineSpacing(0f, 1.2f)
        background = rounded(p.notesBg, a.dpf(10))
        setPadding(a.dp(14), a.dp(12), a.dp(14), a.dp(12))
    }, lp(top = a.dp(12)))
    val total = if (m.isNull("chapters_total")) "?" else m.getInt("chapters_total").toString()
    val stats = mutableListOf("${thousands(m.getLong("words"))} words", "${m.getInt("chapters")}/$total chapters")
    if (m.getLong("kudos") > 0) stats += "${thousands(m.getLong("kudos"))} kudos"
    if (m.getLong("hits") > 0) stats += "${thousands(m.getLong("hits"))} hits"
    if (m.optString("published").isNotEmpty()) stats += "Published ${m.getString("published")}"
    c.addView(a.label(stats.joinToString("  ·  ") { it.replace(' ', ' ') }, 12.5f, p.muted), lp(top = a.dp(12)))

    val busy = link.busy != null
    fun failed(e: String) {
        link.busy = null
        link.error = e
        if (a.currentTab == MainActivity.SEARCH) a.rebuild()
    }
    fun download(open: Boolean) {
        if (busy) return
        link.busy = "Downloading the work… long works can take a little while."
        link.error = null
        refresh()
        a.startFetch(id, open, done = {
            link.busy = null
            link.saved = true
            link.inLibrary = true
            if (!open && a.currentTab == MainActivity.SEARCH) a.rebuild()
        }, failed = ::failed)
    }
    val main = a.row()
    val half = { first: Boolean -> LinearLayout.LayoutParams(0, -2, 1f).apply { if (!first) leftMargin = a.dp(8) } }
    if (link.inLibrary) {
        main.addView(a.button("Open", Kind.Primary, R.drawable.ic_book) { if (!busy) a.openWork(id) }, half(true))
        main.addView(a.button("Download again", iconRes = R.drawable.ic_refresh) { download(false) }, half(false))
    } else {
        main.addView(a.button("Read", Kind.Primary, R.drawable.ic_book) { download(true) }, half(true))
        main.addView(a.button("Save", iconRes = R.drawable.ic_download) { download(false) }, half(false))
    }
    c.addView(main, lp(top = a.dp(16)))
    val more = a.row()
    more.addView(a.button("EPUB file", Kind.Ghost) {
        if (busy) return@button
        link.busy = "Downloading AO3's EPUB…"
        link.error = null
        refresh()
        downloadEpub(a, id, title) { err ->
            link.busy = null
            link.error = err
            if (a.currentTab == MainActivity.SEARCH) a.rebuild()
        }
    })
    more.addView(a.button("Open on AO3", Kind.Ghost) { openUrl(a, "https://archiveofourown.org/works/$id") }, lp(-2).apply { leftMargin = a.dp(4) })
    c.addView(more, lp(top = a.dp(6)))
    col.addView(c)

    link.busy?.let { col.addView(spinnerRow(a, it)) }
    if (link.busy == null) link.error?.let { col.addView(a.notice("Couldn't download", it), lp(top = a.dp(12))) }
    return col
}

/** Downloads AO3's EPUB of a work and offers to save it; `done` gets an error, or null. */
fun downloadEpub(a: MainActivity, id: String, title: String, done: (String?) -> Unit) {
    val tmp = java.io.File(a.cacheDir, "ao3_$id.epub")
    Core.background("epub", json("id" to id, "path" to tmp.absolutePath)) { r ->
        r.onFailure { done(it.message ?: "Couldn't download the EPUB") }.onSuccess {
            done(null)
            a.onDocument = { _, data ->
                data?.data?.let { uri -> a.contentResolver.openOutputStream(uri)?.use { out -> tmp.inputStream().use { it.copyTo(out) } } }
                tmp.delete()
                a.toast("EPUB saved")
            }
            @Suppress("DEPRECATION")
            a.startActivityForResult(Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
                addCategory(Intent.CATEGORY_OPENABLE)
                type = "application/epub+zip"
                putExtra(Intent.EXTRA_TITLE, title.replace(Regex("[\\\\/:*?\"<>|]"), "_").take(100) + ".epub")
            }, 2)
        }
    }
}

private fun spinnerRow(a: MainActivity, text: String): View {
    val r = a.row().apply { setPadding(a.dp(4), a.dp(16), a.dp(4), a.dp(8)); gravity = Gravity.CENTER }
    r.addView(ProgressBar(a), LinearLayout.LayoutParams(a.dp(22), a.dp(22)))
    r.addView(a.label(text, 13.5f, Settings.pal.muted).apply { setPadding(a.dp(10), 0, 0, 0) })
    return r
}

private fun searchTips(a: MainActivity): View {
    val p = Settings.pal
    val c = a.card()
    c.addView(a.label("Search all of AO3", 15f, p.text, bold = true))
    c.addView(a.label("Type words from a title, summary or tag, or open Filters to search by fandom, relationship, character, rating, length and more. Sort by kudos, hits or date to find popular or recent works.\n\nHave a link? Paste a link to an AO3 work (or any of its chapters) to see the work and download it.\n\nTap Read to download a work and open it; it's then saved in your Library for offline reading. Use ⋮ on a result to follow its author or tags.", 13.5f, p.muted), lp(top = a.dp(6)))
    return c
}

private fun filtersForm(a: MainActivity, q: JSONObject, apply: () -> Unit): View {
    val p = Settings.pal
    val col = a.column().apply { setPadding(a.dp(16), a.dp(4), a.dp(16), a.dp(24)) }
    fun heading(t: String) = col.addView(a.label(t, 12.5f, p.muted, bold = true), lp(top = a.dp(14), bottom = a.dp(6)))
    fun text(label: String, key: String, hint: String) {
        heading(label)
        val e = a.input(hint)
        e.setText(q.optString(key))
        e.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, x: Int, y: Int, z: Int) {}
            override fun onTextChanged(s: CharSequence?, x: Int, y: Int, z: Int) {}
            override fun afterTextChanged(s: Editable?) { q.put(key, s?.toString() ?: "") }
        })
        col.addView(e)
    }
    fun number(e: EditText, key: String) {
        e.inputType = InputType.TYPE_CLASS_NUMBER
        if (q.has(key) && !q.isNull(key)) e.setText(q.getLong(key).toString())
        e.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, x: Int, y: Int, z: Int) {}
            override fun onTextChanged(s: CharSequence?, x: Int, y: Int, z: Int) {}
            override fun afterTextChanged(s: Editable?) {
                val v = s?.toString()?.trim()?.toLongOrNull()
                if (v == null) q.put(key, JSONObject.NULL) else q.put(key, v)
            }
        })
    }

    text("Fandoms", "fandoms", "e.g. Harry Potter - J. K. Rowling")
    text("Relationships", "relationships", "e.g. Hermione Granger/Harry Potter")
    text("Characters", "characters", "Separate several with commas")
    text("Additional tags", "tags", "e.g. Fluff, Time Travel")
    text("Title", "title", "Words in the title")
    text("Author", "creators", "AO3 user name")

    heading("Rating")
    val rating = if (q.isNull("rating")) null else q.optString("rating", "").ifEmpty { null }
    col.addView(a.segmented(RATINGS.map { it.second }, RATINGS.indexOfFirst { it.first == rating }.coerceAtLeast(0)) { i ->
        q.put("rating", RATINGS[i].first ?: JSONObject.NULL); a.rebuild()
    })

    heading("Categories (any of)")
    val cats = q.optJSONArray("categories")?.strings()?.toMutableSet() ?: mutableSetOf()
    val catRow = a.row()
    CATEGORIES.forEach { c ->
        catRow.addView(chip(a, c, c in cats) {
            if (!cats.remove(c)) cats += c
            q.put("categories", JSONArray(cats.toList()))
            a.rebuild()
        }, chipLp(a))
    }
    col.addView(HorizontalScrollView(a).apply { isHorizontalScrollBarEnabled = false; addView(catRow) })

    heading("Status")
    val tri = listOf("any", "only", "exclude")
    col.addView(a.segmented(listOf("Any", "Complete", "In progress"), tri.indexOf(q.optString("complete", "any")).coerceAtLeast(0)) { i ->
        q.put("complete", tri[i]); a.rebuild()
    })
    heading("Crossovers")
    col.addView(a.segmented(listOf("Include", "Only", "Exclude"), tri.indexOf(q.optString("crossovers", "any")).coerceAtLeast(0)) { i ->
        q.put("crossovers", tri[i]); a.rebuild()
    })

    heading("Word count")
    val words = a.row()
    val min = a.input("Min")
    val max = a.input("Max")
    number(min, "words_min")
    number(max, "words_max")
    words.addView(min, LinearLayout.LayoutParams(0, -2, 1f))
    words.addView(a.label("to", 13.5f, p.muted).apply { setPadding(a.dp(10), 0, a.dp(10), 0) })
    words.addView(max, LinearLayout.LayoutParams(0, -2, 1f))
    col.addView(words)

    val single = Switch(a).apply {
        text = "One-shots only (single chapter)"
        setTextColor(p.text)
        textSize = 14.5f
        isChecked = q.optBoolean("single_chapter")
        setOnCheckedChangeListener { _, v -> q.put("single_chapter", v) }
    }
    col.addView(single, lp(top = a.dp(14)))

    text("Language", "language", "Language code such as en, es, fr; empty for any")

    val buttons = a.row()
    buttons.addView(a.button("Search", Kind.Primary) { apply() })
    buttons.addView(a.button("Clear filters", Kind.Ghost) {
        val keep = listOf("query", "sort", "ascending").associateWith { q.opt(it) }
        q.keys().asSequence().toList().forEach { q.remove(it) }
        keep.forEach { (k, v) -> if (v != null) q.put(k, v) }
        a.rebuild()
    }, lp(-2).apply { leftMargin = a.dp(8) })
    col.addView(buttons, lp(top = a.dp(20)))
    return ScrollView(a).apply { addView(col) }
}

/** "kind:name" (lower case) for every followed feed, to offer Follow or Unfollow on cards. */
private fun followedKeys(): Map<String, String> =
    Core.obj("follows").getJSONArray("follows").objects().associate { (it.getString("kind") + ":" + it.getString("name").lowercase()) to it.getString("id") }

/** A work from AO3 (search result or followed feed). `mark` is "New", "Updated" or "". */
private fun blurbCard(a: MainActivity, w: JSONObject, followed: Map<String, String>, mark: String = "", from: List<String> = emptyList()): View {
    val p = Settings.pal
    val id = w.getString("id")
    val title = w.getString("title")
    val card = a.card()

    fun read() {
        if (w.optBoolean("in_library")) return a.openWork(id)
        a.toast("Downloading “$title”…")
        a.startFetch(id, open = true, done = { w.put("in_library", true) })
    }

    val top = a.row()
    if (mark.isNotEmpty()) top.addView(a.markBadge(mark), lp(-2).apply { rightMargin = a.dp(8) })
    top.addView(a.label(title + if (w.optBoolean("restricted")) "  🔒" else "", 17f, p.text, bold = true, lines = 2).apply { setOnClickListener { read() } },
        LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
    lateinit var more: View
    more = a.iconButton(R.drawable.ic_more, p.text) { blurbMenu(a, w, followed, more) }
    top.addView(more)
    card.addView(top)

    val fandoms = w.getJSONArray("fandoms").strings().take(2)
    val by = "by " + w.getJSONArray("authors").strings().joinToString(", ") + if (fandoms.isNotEmpty()) "  ·  " + fandoms.joinToString(", ") else ""
    card.addView(a.label(by, 13f, p.muted, lines = 2), lp(top = a.dp(2)))

    card.addView(a.pillScroller(workPills(a, w)), lp(top = a.dp(10)))

    val tags = (w.getJSONArray("characters").strings().take(3) + w.getJSONArray("tags").strings().take(5))
    if (tags.isNotEmpty()) card.addView(a.label(tags.joinToString(" · "), 12.5f, p.muted, lines = 2), lp(top = a.dp(8)))

    val summary = w.getString("summary").replace(Regex("\\*\\*|\\*|~~"), "").split(Regex("\\s+")).joinToString(" ").trim()
    if (summary.isNotEmpty()) card.addView(a.label(summary, 13.5f, alpha(p.text, 0.85f), lines = 4).apply { setLineSpacing(0f, 1.15f) }, lp(top = a.dp(8)))

    val total = if (w.isNull("chapters_total")) "?" else w.getInt("chapters_total").toString()
    val stats = mutableListOf("${thousands(w.getLong("words"))} words", "${w.getInt("chapters")}/$total ch")
    if (w.getLong("kudos") > 0) stats += "${thousands(w.getLong("kudos"))} kudos"
    if (w.getLong("hits") > 0) stats += "${thousands(w.getLong("hits"))} hits"
    if (w.optString("updated").isNotEmpty()) stats += w.getString("updated")
    // Non-breaking spaces keep "28 Oct 2026" and "1,234 words" from splitting across lines.
    card.addView(a.label(stats.joinToString("  ·  ") { it.replace(' ', '\u00A0') }, 12f, p.muted), lp(top = a.dp(8)))
    if (from.isNotEmpty()) card.addView(a.label("From " + from.joinToString(", "), 12f, alpha(p.text, 0.7f), lines = 1), lp(top = a.dp(4)))

    val actions = a.row()
    val inLib = w.optBoolean("in_library")
    actions.addView(a.button(if (inLib) "Open" else "Read", Kind.Tonal, R.drawable.ic_book) { read() })
    if (!inLib) actions.addView(a.button("Save") {
        a.toast("Saving “$title”…")
        a.startFetch(id, open = false, done = { w.put("in_library", true); if (a.currentTab == MainActivity.SEARCH || a.currentTab == MainActivity.FOLLOWING) a.rebuild() })
    }, lp(-2).apply { leftMargin = a.dp(8) })
    card.addView(actions, lp(top = a.dp(12)))
    return card
}

private fun blurbMenu(a: MainActivity, w: JSONObject, followed: Map<String, String>, anchor: View) {
    val menu = PopupMenu(a, anchor)
    val options = mutableListOf<Pair<String, () -> Unit>>()
    fun offer(kind: String, name: String, what: String) {
        val existing = followed["$kind:${name.lowercase()}"]
        options += if (existing != null) "Unfollow $what$name" to { unfollow(a, existing, name) } else "Follow $what$name" to { follow(a, kind, name) }
    }
    for (author in w.getJSONArray("authors").strings().filter { it != "Anonymous" && !it.contains("orphan_account") }) offer("author", author, "author ")
    for (t in w.getJSONArray("relationships").strings().take(3)) offer("tag", t, "")
    for (t in w.getJSONArray("fandoms").strings().take(2)) offer("tag", t, "")
    options += "Open on AO3" to { openUrl(a, "https://archiveofourown.org/works/${w.getString("id")}") }
    options.forEachIndexed { i, (label, _) -> menu.menu.add(0, i, i, label) }
    menu.setOnMenuItemClickListener { options[it.itemId].second(); true }
    menu.show()
}

private fun openUrl(a: MainActivity, url: String) {
    runCatching { a.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url))) }.onFailure { a.toast("No browser found to open the link.") }
}

/** Follows a feed the user chose and loads its newest works in the background. */
private fun follow(a: MainActivity, kind: String, name: String) {
    val added = try {
        Core.obj("follows.add", json("kind" to kind, "name" to name))
    } catch (e: CoreError) {
        return a.toast(e.message ?: "Couldn't follow that")
    }
    a.toast("Following ${added.getString("name")}")
    // A pasted link may be of another kind than the tab it was typed in.
    a.following.kind = added.getString("id").substringBefore(':')
    a.following.selected = null
    checkFollows(a, json("id" to added.getString("id")))
}

private fun unfollow(a: MainActivity, id: String, name: String) {
    Core.req("follows.remove", json("id" to id))
    a.toast("Unfollowed $name")
    if (a.following.selected == id) a.following.selected = null
    a.rebuild()
}

private fun checkFollows(a: MainActivity, arg: JSONObject) {
    val st = a.following
    st.checking = true
    st.error = null
    if (a.currentTab == MainActivity.FOLLOWING) a.rebuild()
    Core.background("follows.check", arg, slow = true) { r ->
        st.checking = false
        r.onSuccess {
            val o = it as JSONObject
            st.error = o.getJSONArray("errors").strings().firstOrNull()
            val fresh = o.getInt("new")
            if (fresh > 0) a.toast(if (fresh == 1) "1 new work from your follows" else "$fresh new works from your follows")
        }.onFailure { st.error = it.message }
        // Only the Following screen is redrawn; elsewhere just the tab badge, so lists keep their place.
        if (a.currentTab == MainActivity.FOLLOWING) a.rebuild() else a.refreshNav()
    }
}

// ---- Following ------------------------------------------------------------------------------

private val KINDS = listOf("author" to "Authors", "tag" to "Tags", "collection" to "Collections")

private fun ago(ts: Long): String {
    val m = (System.currentTimeMillis() / 1000 - ts) / 60
    return when {
        m < 1 -> "just now"
        m < 60 -> "$m min ago"
        m < 60 * 24 -> "${m / 60} h ago"
        else -> "${m / 60 / 24} d ago"
    }
}

/** "+ name" chips from the user's library; tapping one follows it (nothing is followed otherwise). */
private fun suggestionChips(a: MainActivity, kind: String, onPick: (String) -> Unit): View? {
    val names = runCatching { Core.arr("follows.suggest", json("kind" to kind)).strings() }.getOrDefault(emptyList())
    if (names.isEmpty()) return null
    val box = a.column()
    box.addView(a.label("From your library — tap to follow", 12.5f, Settings.pal.muted, bold = true), lp(bottom = a.dp(6)))
    val row = a.row()
    names.forEach { n -> row.addView(chip(a, "+ $n", false) { onPick(n) }, chipLp(a)) }
    box.addView(HorizontalScrollView(a).apply { isHorizontalScrollBarEnabled = false; addView(row) })
    return box
}

fun followingScreen(a: MainActivity): View {
    val p = Settings.pal
    val st = a.following
    val info = Core.obj("follows")
    val all = info.getJSONArray("follows").objects()
    val byKind = info.getJSONObject("unread_by_kind")
    val unread = info.getInt("unread")
    val hasCollections = all.any { it.getString("kind") == "collection" }
    if (st.kind == "collection" && !hasCollections) st.kind = "author"
    val follows = all.filter { it.getString("kind") == st.kind }
    if (st.selected != null && follows.none { it.getString("id") == st.selected }) st.selected = null
    val scope = if (st.selected != null) json("id" to st.selected) else json("kind" to st.kind)
    val plural = KINDS.first { it.first == st.kind }.second.lowercase()

    val box = a.column()
    box.addView(a.header("Following", if (unread > 0) "$unread new" else if (all.isNotEmpty()) "${all.size} followed" else null))

    // Authors | Tags (| Collections, once any are followed), each with its unread count.
    val kinds = KINDS.filter { it.first != "collection" || hasCollections }
    val tabs = a.segmented(kinds.map { it.second }, kinds.indexOfFirst { it.first == st.kind }.coerceAtLeast(0), counts = kinds.map { byKind.optInt(it.first) }, newCounts = true) { i ->
        st.kind = kinds[i].first
        st.selected = null; st.more.clear(); st.morePage = 1; st.moreNext = true; st.scroll = 0
        a.rebuild()
    }
    box.addView(tabs, lp(bottom = a.dp(10)).apply { leftMargin = a.dp(16); rightMargin = a.dp(16) })

    val actions = a.row().apply { setPadding(a.dp(16), 0, a.dp(16), 0) }
    actions.addView(a.button("Follow", Kind.Primary, R.drawable.ic_add) { addFollowDialog(a) })
    if (follows.isNotEmpty()) {
        actions.addView(a.button(if (st.checking) "Checking…" else "Refresh") { if (!st.checking) checkFollows(a, scope) }, lp(-2).apply { leftMargin = a.dp(8) })
    }
    val scopedUnread = if (st.selected != null) follows.first { it.getString("id") == st.selected }.getInt("unread") else byKind.optInt(st.kind)
    if (scopedUnread > 0) actions.addView(a.button("Mark read", Kind.Ghost) {
        Core.req("follows.read", scope)
        a.rebuild()
    }, lp(-2).apply { leftMargin = a.dp(4) })
    box.addView(actions)

    // Check feeds that haven't been checked (or tried) for a while. A failed check counts as
    // tried, so a down AO3 isn't asked again and again.
    if (!st.checking && all.any { it.optBoolean("stale") }) box.post { if (!st.checking) checkFollows(a, json("stale" to true)) }

    if (follows.isEmpty()) {
        val c = a.card()
        c.addView(a.label("No $plural followed yet", 15f, p.text, bold = true))
        val text = when (st.kind) {
            "author" -> "Follow AO3 users to see their new works and updates here. Tap Follow, or use ⋮ on any search result."
            "tag" -> "Follow any AO3 tag: a fandom, a character, or a relationship such as Hermione Granger/Harry Potter. Tap Follow, or use ⋮ on any search result."
            else -> "Follow an AO3 collection (a challenge, fest or community) by its name or link."
        }
        c.addView(a.label(text, 13.5f, p.muted), lp(top = a.dp(6)))
        suggestionChips(a, st.kind) { follow(a, st.kind, it) }?.let { c.addView(it, lp(top = a.dp(14))) }
        box.addView(c, lp(top = a.dp(14)).apply { leftMargin = a.dp(16); rightMargin = a.dp(16) })
        return box
    }

    val chips = a.row().apply { setPadding(a.dp(16), a.dp(12), a.dp(16), a.dp(6)) }
    val tabUnread = byKind.optInt(st.kind)
    chips.addView(chip(a, if (tabUnread > 0) "All $plural · $tabUnread" else "All $plural", st.selected == null) {
        st.selected = null; st.more.clear(); st.scroll = 0; a.rebuild()
    }, chipLp(a))
    for (f in follows) {
        val id = f.getString("id")
        val n = f.getInt("unread")
        chips.addView(chip(a, f.getString("name") + if (n > 0) " · $n" else "", id == st.selected) {
            st.selected = id; st.more.clear(); st.morePage = 1; st.moreNext = true; st.scroll = 0
            a.rebuild()
        }, chipLp(a))
    }
    box.addView(HorizontalScrollView(a).apply { isHorizontalScrollBarEnabled = false; addView(chips) }, lp())

    val list = listView(a)
    val header = a.column()
    val sel = follows.firstOrNull { it.getString("id") == st.selected }
    if (sel != null) {
        val c = a.row().apply { setPadding(a.dp(4), 0, 0, a.dp(8)) }
        val t = a.column()
        t.addView(a.label(sel.getString("name"), 14f, p.text, bold = true, lines = 2))
        val sub = mutableListOf<String>()
        if (!sel.isNull("total")) sub += "${thousands(sel.getLong("total"))} works"
        sub += if (sel.isNull("checked")) "not checked yet" else "checked ${ago(sel.getLong("checked"))}"
        t.addView(a.label(sub.joinToString("  ·  "), 12.5f, p.muted))
        c.addView(t, LinearLayout.LayoutParams(0, -2, 1f))
        lateinit var more: View
        more = a.iconButton(R.drawable.ic_more, p.text) {
            val m = PopupMenu(a, more)
            m.menu.add(0, 0, 0, "Open on AO3")
            m.menu.add(0, 1, 1, "Unfollow")
            m.setOnMenuItemClickListener {
                if (it.itemId == 0) openUrl(a, sel.getString("url")) else AlertDialog.Builder(a)
                    .setTitle("Unfollow ${sel.getString("name")}?")
                    .setPositiveButton("Unfollow") { _, _ -> unfollow(a, sel.getString("id"), sel.getString("name")) }
                    .setNegativeButton("Cancel", null).show()
                true
            }
            m.show()
        }
        c.addView(more)
        header.addView(c)
        if (!sel.isNull("error")) header.addView(errorCard(a, "Couldn't check this feed", sel.getString("error")), lp(bottom = a.dp(8)))
    }
    if (st.checking) header.addView(spinnerRow(a, "Checking AO3 for new works…"))
    else if (st.error != null && sel == null) header.addView(errorCard(a, "Couldn't check every feed", st.error!!), lp(bottom = a.dp(8)))

    val timeline = Core.arr("follows.timeline", scope).objects()
    val items = timeline + st.more.filter { m -> timeline.none { it.getString("id") == m.getString("id") } }
    if (items.isEmpty() && !st.checking) header.addView(a.label("No works yet. Tap Refresh to load the newest works from AO3.", 14f, p.muted).apply { setPadding(a.dp(4), a.dp(16), a.dp(4), 0) })
    list.addHeaderView(header, null, false)

    if (sel != null && items.isNotEmpty() && st.moreNext) {
        val footer = FrameLayout(a).apply { setPadding(0, a.dp(4), 0, a.dp(8)) }
        if (st.moreBusy) footer.addView(spinnerRow(a, "Loading older works…")) else footer.addView(a.button("Load older works") {
            st.moreBusy = true
            st.scroll = list.firstVisiblePosition
            a.rebuild()
            Core.background("ao3.feed", json("kind" to sel.getString("kind"), "name" to sel.getString("name"), "page" to st.morePage + 1)) { r ->
                st.moreBusy = false
                r.onSuccess {
                    val o = it as JSONObject
                    st.more += o.getJSONArray("works").objects()
                    st.morePage += 1
                    st.moreNext = o.getBoolean("has_next")
                }.onFailure { a.toast(it.message ?: "Couldn't load more") }
                if (a.currentTab == MainActivity.FOLLOWING) a.rebuild()
            }
        }, frame(-1, -2))
        list.addFooterView(footer, null, false)
    }
    val followed = followedKeys()
    list.adapter = adapter({ items }) { w ->
        val fromNames = if (st.selected == null) w.optJSONArray("from")?.strings() ?: emptyList() else emptyList()
        blurbCard(a, w, followed, mark = w.optString("mark"), from = fromNames)
    }
    list.setSelection(st.scroll)
    list.setOnScrollListener(object : android.widget.AbsListView.OnScrollListener {
        override fun onScrollStateChanged(v: android.widget.AbsListView?, s: Int) {}
        override fun onScroll(v: android.widget.AbsListView?, first: Int, visible: Int, total: Int) { st.scroll = first }
    })
    box.addView(list, LinearLayout.LayoutParams(-1, 0, 1f))
    return box
}

private fun addFollowDialog(a: MainActivity) {
    val p = Settings.pal
    val hints = mapOf(
        "author" to "AO3 user name, or a link to their page",
        "tag" to "Fandom, ship or tag, e.g. Hermione Granger/Harry Potter",
        "collection" to "Collection name, or a link to it",
    )
    var kind = KINDS.indexOfFirst { it.first == a.following.kind }.coerceAtLeast(0)
    val col = a.column().apply { setPadding(a.dp(20), a.dp(8), a.dp(20), 0) }
    val input = a.input(hints.getValue(KINDS[kind].first))
    val seg = FrameLayout(a)
    val suggestions = FrameLayout(a)
    lateinit var dialog: AlertDialog
    fun fill() {
        seg.removeAllViews()
        seg.addView(a.segmented(KINDS.map { it.second }, kind) { i -> kind = i; input.hint = hints.getValue(KINDS[i].first); fill() })
        suggestions.removeAllViews()
        suggestionChips(a, KINDS[kind].first) { name -> dialog.dismiss(); follow(a, KINDS[kind].first, name) }?.let { suggestions.addView(it) }
    }
    col.addView(seg)
    col.addView(input, lp(top = a.dp(12)))
    col.addView(a.label("Tag names must match AO3's exactly. A link copied from AO3 always works.", 12.5f, p.muted), lp(top = a.dp(8)))
    col.addView(suggestions, lp(top = a.dp(12)))
    dialog = AlertDialog.Builder(a)
        .setTitle("Follow on AO3")
        .setView(col)
        .setPositiveButton("Follow") { _, _ ->
            val name = input.text.toString().trim()
            if (name.isNotEmpty()) follow(a, KINDS[kind].first, name)
        }
        .setNegativeButton("Cancel", null)
        .create()
    fill()
    dialog.show()
}
