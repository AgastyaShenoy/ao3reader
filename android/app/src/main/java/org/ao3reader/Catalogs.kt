package org.ao3reader

import android.view.Gravity
import android.view.View
import android.view.inputmethod.EditorInfo
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ScrollView
import org.json.JSONObject

/** One page of a catalog: its address, and what it lists once loaded. */
class CatalogPage(val title: String, val url: String) {
    var feed: JSONObject? = null
}

/** The Browse tab's state: which source (0 is AO3, then the catalogs) and where in it. */
class CatalogState {
    var source = 0
    /** The catalogs (Project Gutenberg, then the reader's), from the core. */
    var catalogs: List<JSONObject> = emptyList()
    val stack = mutableListOf<CatalogPage>()
    var busy = false
    var error: String? = null
    var query = ""
    var scroll = 0
    /** Books being downloaded, by entry key: "busy", "done:<work id>" or "failed:<message>". */
    val getting = HashMap<String, String>()

    fun switch(to: Int) {
        source = to
        stack.clear()
        busy = false
        error = null
        query = ""
        scroll = 0
    }
}

/** The Browse tab: AO3's search beside OPDS catalogs (Project Gutenberg, and any the reader adds). */
fun browseScreen(a: MainActivity): View {
    val cs = a.catalogs
    if (cs.catalogs.isEmpty()) cs.catalogs = runCatching { Core.arr("catalogs").objects() }.getOrDefault(emptyList())
    cs.source = cs.source.coerceIn(0, cs.catalogs.size)
    val box = a.column()
    val st = a.search
    val count = if (cs.source == 0) st.total?.takeIf { st.link == null }?.let { "${thousands(it)} works" } else null
    // Books the reader already has come in through Settings' import page.
    val head = a.header("Browse", count) as LinearLayout
    head.gravity = android.view.Gravity.CENTER_VERTICAL
    head.addView(View(a), LinearLayout.LayoutParams(0, 1, 1f))
    head.addView(a.button("Import files", Kind.Secondary, R.drawable.ic_import) { a.show(MainActivity.IMPORT) }.apply {
        contentDescription = "Import your own files: EPUB books, PDFs and AO3 downloads"
    })
    box.addView(head)

    // The sources, and adding a catalog.
    val chips = a.row().apply { setPadding(a.dp(16), 0, a.dp(16), a.dp(8)) }
    val names = listOf("AO3") + cs.catalogs.map { it.getString("name") }
    names.forEachIndexed { i, n ->
        chips.addView(a.countChip(n, 0, i == cs.source) { if (i != cs.source) { cs.switch(i); a.rebuild() } }, LinearLayout.LayoutParams(-2, -2).apply { rightMargin = a.dp(8) })
    }
    chips.addView(a.countChip("+ Add a catalogue", 0, false) { addCatalogDialog(a) })
    box.addView(a.pillScroller(chips), lp())

    box.addView(if (cs.source == 0) searchScreen(a, embedded = true) else catalogView(a), LinearLayout.LayoutParams(-1, 0, 1f))
    return box
}

private fun catalogView(a: MainActivity): View {
    val p = Settings.pal
    val cs = a.catalogs
    val index = cs.source - 1
    val catalog = cs.catalogs[index]
    val box = a.column()

    fun reload() {
        if (a.currentTab == MainActivity.SEARCH) a.rebuild()
    }

    fun load(page: CatalogPage, url: String = page.url, append: Boolean = false) {
        cs.busy = true
        cs.error = null
        Core.background("catalog.feed", json("index" to index, "url" to url)) { r ->
            cs.busy = false
            r.onSuccess {
                val feed = it as JSONObject
                val have = page.feed
                if (append && have != null) {
                    val entries = have.getJSONArray("entries")
                    feed.getJSONArray("entries").objects().forEach { e -> entries.put(e) }
                    have.put("next", feed.opt("next"))
                } else page.feed = feed
            }.onFailure { cs.error = it.message }
            reload()
        }
    }

    fun open(title: String, url: String) {
        val page = CatalogPage(title, url)
        cs.stack += page
        cs.scroll = 0
        load(page)
    }

    if (cs.stack.isEmpty()) open(catalog.getString("name"), catalog.getString("url"))
    val page = cs.stack.last()

    // Back, where we are, and removing the reader's own catalog.
    val top = a.row().apply { setPadding(a.dp(8), 0, a.dp(8), 0) }
    if (cs.stack.size > 1) top.addView(a.iconButton(R.drawable.ic_back, p.text, "Back") {
        cs.stack.removeAt(cs.stack.size - 1)
        cs.error = null
        cs.scroll = 0
        a.rebuild()
    })
    top.addView(a.label(page.title, 16f, p.text, bold = true, lines = 1).apply { setPadding(a.dp(8), 0, 0, 0) }, LinearLayout.LayoutParams(0, -2, 1f))
    // Follow the shelf on screen: a page of books (not a search, nor a list of shelves).
    val shelf = page.feed?.getJSONArray("entries")?.objects()?.any { !it.isNull("book") } == true || cs.stack.size > 1
    if (shelf && !page.url.startsWith("search:")) {
        val following = followedLinks().contains(page.url)
        top.addView(a.button(if (following) "Following" else "Follow", Kind.Secondary, if (following) R.drawable.ic_check else R.drawable.ic_add) {
            if (!following) followShelf(a, "${page.title} · ${catalog.getString("name")}", page.url, catalog.getString("url"))
        })
    }
    if (!catalog.optBoolean("builtin")) top.addView(a.iconButton(R.drawable.ic_delete, p.muted, "Remove this catalogue") {
        a.dialog("Remove ${catalog.getString("name")}?", "Books you got from it stay in your library.", ok = "Remove", okKind = Kind.Danger) {
            cs.catalogs = Core.arr("catalog.remove", json("index" to index)).objects()
            cs.switch(1)
            a.rebuild()
            true
        }
    })
    box.addView(top, lp(bottom = a.dp(4)))

    // Search, when the catalog has it.
    val search = (page.feed ?: cs.stack.first().feed)?.optJSONObject("search")
    if (search != null) {
        val row = a.row().apply { setPadding(a.dp(16), 0, a.dp(8), a.dp(6)) }
        val input = a.input("Search ${catalog.getString("name")}")
        input.setText(cs.query)
        input.imeOptions = EditorInfo.IME_ACTION_SEARCH
        fun run() {
            val q = input.text.toString().trim()
            if (q.isEmpty()) return
            cs.query = q
            (a.getSystemService(android.view.inputmethod.InputMethodManager::class.java))?.hideSoftInputFromWindow(input.windowToken, 0)
            while (cs.stack.size > 1) cs.stack.removeAt(cs.stack.size - 1)
            val results = CatalogPage("“$q”", "search:$q")
            cs.stack += results
            cs.busy = true
            cs.error = null
            cs.scroll = 0
            Core.background("catalog.search", json("index" to index, "search" to search, "query" to q)) { r ->
                cs.busy = false
                r.onSuccess { results.feed = it as JSONObject }.onFailure { cs.error = it.message }
                reload()
            }
            a.rebuild()
        }
        input.setOnEditorActionListener { _, _, _ -> run(); true }
        row.addView(input, LinearLayout.LayoutParams(0, -2, 1f))
        row.addView(a.iconButton(R.drawable.ic_search, p.text, "Search") { run() })
        box.addView(row)
    }

    val feed = page.feed
    when {
        cs.error != null -> {
            box.addView(FrameLayout(a).apply { setPadding(a.dp(16), a.dp(8), a.dp(16), 0); addView(a.notice("Couldn't load the catalogue", cs.error!!)) })
            return box
        }
        feed == null -> {
            box.addView(a.label("Loading…", 14.5f, p.muted).apply { gravity = Gravity.CENTER; setPadding(0, a.dp(40), 0, 0) }, lp())
            return box
        }
    }
    val entries = feed!!.getJSONArray("entries").objects()
    if (entries.isEmpty()) {
        box.addView(a.label("Nothing here.", 14.5f, p.muted).apply { gravity = Gravity.CENTER; setPadding(0, a.dp(40), 0, 0) }, lp())
        return box
    }

    val list = a.column().apply { setPadding(a.dp(16), a.dp(4), a.dp(16), a.dp(16)) }
    for (e in entries) list.addView(entryCard(a, e, index, ::open), lp(bottom = a.dp(8)))
    if (!feed.isNull("next")) {
        list.addView(if (cs.busy) a.label("Loading more…", 14f, p.muted).apply { gravity = Gravity.CENTER } else a.button("More") {
            load(page, feed.getString("next"), append = true)
            a.rebuild()
        }, lp(top = a.dp(4)))
    }
    val scroller = ScrollView(a).apply { addView(list) }
    scroller.setOnScrollChangeListener { _, _, y, _, _ -> cs.scroll = y }
    scroller.post { scroller.scrollTo(0, cs.scroll) }
    box.addView(scroller, LinearLayout.LayoutParams(-1, 0, 1f))
    return box
}

/** One entry: a book to get (then read), or a shelf to open. */
private fun entryCard(a: MainActivity, e: JSONObject, index: Int, open: (String, String) -> Unit): View {
    val p = Settings.pal
    val cs = a.catalogs
    val card = a.card()
    val row = a.row().apply { gravity = Gravity.CENTER_VERTICAL }
    val text = a.column()
    text.addView(a.label(e.getString("title"), 15.5f, p.text, bold = true, lines = 2))
    val authors = e.getJSONArray("authors").strings().joinToString(", ")
    if (authors.isNotEmpty()) text.addView(a.label(authors, 13f, p.muted, lines = 1), lp(top = a.dp(2)))
    val summary = e.optString("summary")
    if (summary.isNotEmpty() && summary != authors) text.addView(a.label(summary, 13f, alpha(p.text, 0.82f), lines = 3).apply { setLineSpacing(0f, 1.1f) }, lp(top = a.dp(4)))
    row.addView(text, LinearLayout.LayoutParams(0, -2, 1f))

    val key = e.getString("key")
    val book = e.optJSONObject("book")
    val feed = e.optJSONObject("feed")
    val state = cs.getting[key]
    val story = book?.optString("mime") == STORY_MIME
    val action: View? = when {
        book != null && state == "busy" -> a.label("Getting…", 13.5f, p.muted)
        book != null && state?.startsWith("done:") == true -> a.button("Read", Kind.Tonal, R.drawable.ic_book) { a.openWork(state.removePrefix("done:")) }
        book != null -> a.button(if (state?.startsWith("failed:") == true) "Try again" else "Get", Kind.Tonal, R.drawable.ic_download) {
            cs.getting[key] = "busy"
            a.rebuild()
            // A forum story is read page by page: the slow worker, so Browse stays quick.
            Core.background("catalog.get", json("index" to index, "url" to book.getString("href"), "mime" to book.optString("mime"), "title" to e.getString("title")), slow = story) { r ->
                r.onSuccess {
                    val id = (it as JSONObject).getString("id")
                    cs.getting[key] = "done:$id"
                    a.toast("Saved “${e.getString("title")}” to your library")
                }.onFailure {
                    cs.getting[key] = "failed:${it.message}"
                    a.toast(it.message ?: "Couldn't get the book")
                }
                if (a.currentTab == MainActivity.SEARCH) a.rebuild()
            }
        }
        feed != null -> a.button("Open", Kind.Ghost, R.drawable.ic_next) { open(e.getString("title"), feed.getString("href")) ; a.rebuild() }
        else -> null
    }
    if (action != null) row.addView(action, LinearLayout.LayoutParams(-2, -2).apply { leftMargin = a.dp(10) })
    card.addView(row)
    if (story && book != null) {
        // A story can be followed for its new chapters (which saves it too).
        val following = followedLinks().contains(book.getString("href"))
        card.addView(a.button(if (following) "Following" else "Follow for new chapters", Kind.Ghost, if (following) R.drawable.ic_check else R.drawable.ic_add) {
            if (!following) followStory(a, book.getString("href"))
        }, lp(top = a.dp(6)).apply { width = LinearLayout.LayoutParams.WRAP_CONTENT })
    }
    if (feed != null && book == null) card.setOnClickListener { open(e.getString("title"), feed.getString("href")); a.rebuild() }
    return card
}

/** Addresses of the followed stories and shelves. */
private fun followedLinks(): Set<String> =
    runCatching { Core.obj("follows").getJSONArray("follows").objects().map { it.optString("link") }.filter { it.isNotEmpty() }.toSet() }.getOrDefault(emptySet())

/** Adding a catalog: its address, a name for its chip, and a login if it needs one. */
fun addCatalogDialog(a: MainActivity) {
    val p = Settings.pal
    val form = a.column()
    fun heading(text: String, top: Int) = form.addView(a.label(text, 13f, p.text, bold = true), lp(top = top, bottom = a.dp(6)))
    val url = a.input("https://…").apply {
        inputType = android.text.InputType.TYPE_CLASS_TEXT or android.text.InputType.TYPE_TEXT_VARIATION_URI
        typeface = Settings.face()
    }
    val problem = a.label("", 13f, p.dangerText).apply { visibility = View.GONE }
    val name = a.input("Optional, e.g. Home library")
    val user = a.input("User name")
    val pass = a.input("Password", password = true)
    heading("Address", 0)
    form.addView(url, lp())
    form.addView(problem, lp(top = a.dp(6)))
    heading("Name", a.dp(14))
    form.addView(name, lp())
    heading("Sign-in, if the catalogue asks for one", a.dp(18))
    form.addView(user, lp(bottom = a.dp(8)))
    form.addView(pass, lp())
    a.dialog(
        "Add a catalogue",
        "Paste the address of an OPDS feed: Calibre's content server (…/opds), Kavita, Komga, or a library's catalogue.",
        form,
        ok = "Add",
    ) {
        try {
            val list = Core.arr("catalog.add", json("name" to name.text.toString(), "url" to url.text.toString(), "user" to user.text.toString(), "password" to pass.text.toString())).objects()
            a.catalogs.catalogs = list
            a.catalogs.switch(list.size)
            a.rebuild()
            true
        } catch (e: CoreError) {
            problem.text = e.message
            problem.visibility = View.VISIBLE
            false
        }
    }
}
