package org.ao3reader

import android.app.Activity
import android.content.Intent
import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import android.widget.Toast
import android.window.OnBackInvokedDispatcher

/** One activity; screens are plain views swapped in and out. */
class MainActivity : Activity() {
    private lateinit var root: FrameLayout
    private lateinit var shell: LinearLayout
    private lateinit var content: FrameLayout
    private lateinit var nav: LinearLayout
    private var tab = LIBRARY
    private var animateTab = false
    private var reader: ReaderScreen? = null

    val library = LibraryState()
    val search = SearchState()
    val following = FollowingState()
    val imports = ImportState()

    companion object {
        const val LIBRARY = 0
        const val SEARCH = 1
        const val FOLLOWING = 2
        const val SETTINGS = 3
        /** Opened from Settings; not in the tab bar. */
        const val ACCOUNT = 4
        const val IMPORT = 5
    }

    /** Called with the result of a document picker or creator started by a screen. */
    var onDocument: ((Int, Intent?) -> Unit)? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val init = Core.obj("init", json("dir" to filesDir.absolutePath))
        Settings.themes = init.getJSONArray("themes").objects().map(::Pal)
        Settings.load()

        root = FrameLayout(this)
        // Keep the first text field from taking focus (and the keyboard) on launch.
        root.isFocusableInTouchMode = true
        // When a text field gives up focus it lands here; don't grey the whole screen to show it.
        if (Build.VERSION.SDK_INT >= 26) root.defaultFocusHighlightEnabled = false
        setContentView(root)
        root.setOnApplyWindowInsetsListener { v, insets -> applyInsets(insets); insets }
        rebuild()

        if (Build.VERSION.SDK_INT >= 33) {
            onBackInvokedDispatcher.registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT) { handleBack() }
        }
    }

    @Deprecated("Used before Android 13")
    override fun onBackPressed() {
        handleBack()
    }

    private fun handleBack() {
        val r = reader
        when {
            r != null -> if (!r.back()) closeReader()
            tab == ACCOUNT || tab == IMPORT -> show(SETTINGS)
            tab != LIBRARY -> show(LIBRARY)
            else -> finish()
        }
    }

    override fun onPause() {
        super.onPause()
        reader?.saveProgress()
    }

    @Deprecated("Document picker results")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (resultCode == RESULT_OK) onDocument?.invoke(requestCode, data)
        onDocument = null
    }

    private var insetTop = 0
    private var insetBottom = 0
    private var cutoutTop = 0
    private var hintShown = false

    @Suppress("DEPRECATION")
    private fun applyInsets(insets: WindowInsets) {
        val (top, bottom) = if (Build.VERSION.SDK_INT >= 30) {
            val i = insets.getInsets(WindowInsets.Type.systemBars() or WindowInsets.Type.ime())
            i.top to i.bottom
        } else insets.systemWindowInsetTop to insets.systemWindowInsetBottom
        insetTop = top
        insetBottom = bottom
        // Camera cutouts stay in place when the status bar hides; the page must avoid them.
        cutoutTop = when {
            Build.VERSION.SDK_INT >= 30 -> insets.getInsets(WindowInsets.Type.displayCutout()).top
            Build.VERSION.SDK_INT >= 28 -> insets.displayCutout?.safeInsetTop ?: 0
            else -> 0
        }
        shell.setPadding(0, top, 0, bottom)
        reader?.setInsets(top, bottom, cutoutTop)
    }

    /** Rebuilds every view, e.g. after the theme changes. */
    fun rebuild() {
        val p = Settings.pal
        root.removeAllViews()
        root.setBackgroundColor(p.bg)
        window.decorView.setBackgroundColor(p.bg)
        styleSystemBars(p)

        shell = column()
        content = FrameLayout(this)
        shell.addView(content, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f))
        nav = row().apply {
            setBackgroundColor(p.surface)
            setPadding(0, dp(6), 0, dp(6))
        }
        shell.addView(View(this).apply { setBackgroundColor(p.border) }, lp(h = dp(1)))
        shell.addView(nav, lp())
        root.addView(shell, frame())
        shell.setPadding(0, insetTop, 0, insetBottom)

        buildNav()
        val screenView = screen(tab)
        content.addView(screenView, frame())
        if (animateTab) Anim.enter(screenView, dy = dpf(10), ms = 170)
        animateTab = false

        reader?.let { r ->
            r.rebuild()
            root.addView(r, frame())
        }
        root.requestApplyInsets()
    }

    /** Redraws only the tab bar (e.g. a new unread count), leaving the screen as it is. */
    fun refreshNav() {
        if (::nav.isInitialized) buildNav()
    }

    private fun buildNav() {
        val p = Settings.pal
        nav.removeAllViews()
        val tabs = listOf(R.drawable.ic_book to "Library", R.drawable.ic_search to "Search", R.drawable.ic_follow to "Following", R.drawable.ic_tune to "Settings")
        val unread = runCatching { Core.obj("follows").getInt("unread") }.getOrDefault(0)
        tabs.forEachIndexed { i, (res, name) ->
            val sel = i == tab || ((tab == ACCOUNT || tab == IMPORT) && i == SETTINGS)
            val item = column().apply {
                gravity = Gravity.CENTER
                setPadding(0, dp(4), 0, dp(4))
                setOnClickListener { show(i) }
            }
            val iconBox = FrameLayout(this)
            // Material-style indicator: a neutral pill behind the current tab's icon, which takes
            // the accent. Everything else in the bar stays quiet.
            if (sel) iconBox.background = rounded(p.selected, dpf(14))
            iconBox.addView(icon(res, if (sel) p.accentMark else p.muted, 24), frame(dp(24), dp(24), Gravity.CENTER))
            if (i == FOLLOWING && unread > 0) {
                // New works from followed authors and tags.
                iconBox.addView(label(if (unread > 99) "99+" else "$unread", 9.5f, p.onAccent, bold = true).apply {
                    gravity = Gravity.CENTER
                    background = rounded(p.accent, dpf(8))
                    setPadding(dp(4), 0, dp(4), 0)
                    minWidth = dp(16)
                }, frame(ViewGroup.LayoutParams.WRAP_CONTENT, dp(16), Gravity.TOP or Gravity.END))
                iconBox.contentDescription = "$unread new"
            }
            item.addView(iconBox, LinearLayout.LayoutParams(dp(56), dp(30)))
            item.addView(label(name, 11.5f, if (sel) p.text else p.muted, bold = sel, lines = 1).apply { gravity = Gravity.CENTER }, lp(top = dp(2)))
            nav.addView(item, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        }
    }

    fun show(i: Int) {
        animateTab = i != tab
        tab = i
        rebuild()
    }

    val currentTab get() = tab

    private fun screen(i: Int): View = when (i) {
        SEARCH -> searchScreen(this)
        FOLLOWING -> followingScreen(this)
        SETTINGS -> settingsScreen(this)
        ACCOUNT -> accountScreen(this)
        IMPORT -> importScreen(this)
        else -> libraryScreen(this)
    }

    @Suppress("DEPRECATION")
    fun styleSystemBars(p: Pal) {
        window.statusBarColor = Color.TRANSPARENT
        window.navigationBarColor = Color.TRANSPARENT
        if (Build.VERSION.SDK_INT >= 30) {
            val light = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS
            window.insetsController?.setSystemBarsAppearance(if (p.dark) 0 else light, light)
        } else {
            window.decorView.systemUiVisibility = View.SYSTEM_UI_FLAG_LAYOUT_STABLE or View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or
                View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION or (if (p.dark) 0 else View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR)
        }
    }

    /** Hides or shows the status and navigation bars (immersive reading). */
    fun immersive(on: Boolean) {
        if (Build.VERSION.SDK_INT >= 30) {
            val c = window.insetsController ?: return
            if (on) {
                c.hide(WindowInsets.Type.systemBars())
                c.systemBarsBehavior = WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            } else c.show(WindowInsets.Type.systemBars())
        } else {
            @Suppress("DEPRECATION")
            window.decorView.systemUiVisibility = if (on) {
                View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or View.SYSTEM_UI_FLAG_FULLSCREEN or View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or
                    View.SYSTEM_UI_FLAG_LAYOUT_STABLE or View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
            } else View.SYSTEM_UI_FLAG_LAYOUT_STABLE or View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
        }
    }

    fun openWork(id: String) {
        val opened = try {
            Core.obj("work.open", json("id" to id))
        } catch (e: CoreError) {
            if (e.message == "missing" && id.all { it.isDigit() }) {
                toast("Downloading this work again…")
                startFetch(id, open = true)
            } else toast(e.message ?: "Couldn't open the work")
            return
        }
        reader?.saveProgress()
        currentFocus?.let { v ->
            v.clearFocus()
            getSystemService(android.view.inputmethod.InputMethodManager::class.java)?.hideSoftInputFromWindow(v.windowToken, 0)
        }
        runCatching { Core.req("follows.read", json("work" to id)) }
        val r = ReaderScreen(this, id, opened)
        reader?.let { root.removeView(it) }
        reader = r
        root.addView(r, frame())
        r.setInsets(insetTop, insetBottom, cutoutTop)
        Anim.enter(r, dy = dpf(28), ms = 220)
        if (!hintShown) {
            hintShown = true
            toast("Tap the middle of the page for the menu; tap the sides to turn pages.")
        }
    }

    fun closeReader() {
        val r = reader ?: return
        r.saveProgress()
        reader = null
        immersive(false)
        // Rebuild the screens underneath, then let the reader fade away on top of them.
        rebuild()
        root.addView(r)
        Anim.exit(r, dy = dpf(28), ms = 160) { root.removeView(r) }
    }

    /** Downloads a work; `done` gets the work id on success, `failed` the error (else it's a toast). */
    fun startFetch(input: String, open: Boolean, done: ((String) -> Unit)? = null, failed: ((String) -> Unit)? = null) {
        Core.background("fetch", json("input" to input)) { r ->
            r.onSuccess {
                val o = it as org.json.JSONObject
                toast("Saved “${o.getJSONObject("meta").optString("title")}”")
                done?.invoke(o.getString("id"))
                if (open) openWork(o.getString("id"))
            }.onFailure {
                val msg = it.message ?: "Couldn't download the work"
                if (failed != null) failed(msg) else toast(msg)
            }
        }
    }

    fun toast(msg: String) = Toast.makeText(this, msg, Toast.LENGTH_SHORT).show()

    fun header(title: String, sub: String? = null, logo: Boolean = false): View {
        val p = Settings.pal
        val box = row().apply { setPadding(dp(20), dp(18), dp(20), dp(10)) }
        if (logo) {
            // The Archive's mark on the home screen, top left, as on the desktop app.
            box.addView(ImageView(this).apply {
                setImageResource(R.drawable.ao3_logo)
                imageTintList = android.content.res.ColorStateList.valueOf(p.accentMark)
                contentDescription = "AO3"
            }, LinearLayout.LayoutParams(dp(40), dp(28)).apply { rightMargin = dp(12) })
        }
        box.addView(label(title, 26f, p.text, bold = true))
        if (sub != null) box.addView(label(sub, 13f, p.muted).apply { setPadding(dp(10), dp(8), 0, 0) })
        return box
    }
}

/** Remembers screen state across rebuilds. */
class LibraryState {
    var query = ""
    var filter = 0
    var sort = 0
}

class ImportState {
    var busy = false
    var error: String? = null
    /** The work imported last: its meta JSON. */
    var done: org.json.JSONObject? = null
}

fun textOrEmpty(v: TextView) = v.text?.toString() ?: ""
