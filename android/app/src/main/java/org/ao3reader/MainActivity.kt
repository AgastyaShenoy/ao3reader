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
    private var tab = 0
    private var reader: ReaderScreen? = null

    val library = LibraryState()
    val fetch = FetchState()

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
            tab != 0 -> show(0)
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

        val tabs = listOf(R.drawable.ic_book to "Library", R.drawable.ic_download to "Fetch", R.drawable.ic_person to "Account", R.drawable.ic_tune to "Settings")
        tabs.forEachIndexed { i, (res, name) ->
            val sel = i == tab
            val item = column().apply {
                gravity = Gravity.CENTER
                setPadding(0, dp(4), 0, dp(4))
                setOnClickListener { show(i) }
            }
            item.addView(icon(res, if (sel) p.accent else p.muted, 24))
            item.addView(label(name, 11.5f, if (sel) p.accent else p.muted, bold = sel).apply { gravity = Gravity.CENTER })
            nav.addView(item, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        }
        content.addView(screen(tab), frame())

        reader?.let { r ->
            r.rebuild()
            root.addView(r, frame())
        }
        root.requestApplyInsets()
    }

    fun show(i: Int) {
        tab = i
        rebuild()
    }

    private fun screen(i: Int): View = when (i) {
        1 -> fetchScreen(this)
        2 -> accountScreen(this)
        3 -> settingsScreen(this)
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
                fetch.input = id
                show(1)
                startFetch(id, open = true)
            } else toast(e.message ?: "Couldn't open the work")
            return
        }
        reader?.saveProgress()
        currentFocus?.let { v ->
            v.clearFocus()
            getSystemService(android.view.inputmethod.InputMethodManager::class.java)?.hideSoftInputFromWindow(v.windowToken, 0)
        }
        val r = ReaderScreen(this, id, opened)
        reader = r
        root.addView(r, frame())
        r.setInsets(insetTop, insetBottom, cutoutTop)
        if (!hintShown) {
            hintShown = true
            toast("Tap the middle of the page for the menu; tap the sides to turn pages.")
        }
    }

    fun closeReader() {
        reader?.let {
            it.saveProgress()
            root.removeView(it)
        }
        reader = null
        immersive(false)
        rebuild()
    }

    fun startFetch(input: String, open: Boolean) {
        fetch.busy = "Downloading and reading the work… long works can take a little while."
        fetch.error = null
        if (tab == 1) rebuild()
        Core.background("fetch", json("input" to input)) { r ->
            fetch.busy = null
            r.onSuccess {
                val o = it as org.json.JSONObject
                fetch.preview = o.getJSONObject("meta")
                toast("Saved “${fetch.preview?.optString("title")}”")
                if (open) openWork(o.getString("id"))
            }.onFailure { fetch.error = it.message }
            if (tab == 1 && reader == null) rebuild()
        }
    }

    fun toast(msg: String) = Toast.makeText(this, msg, Toast.LENGTH_SHORT).show()

    fun header(title: String, sub: String? = null): View {
        val p = Settings.pal
        val box = row().apply { setPadding(dp(20), dp(18), dp(20), dp(10)) }
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

class FetchState {
    var input = ""
    var busy: String? = null
    var error: String? = null
    var preview: org.json.JSONObject? = null
}

fun textOrEmpty(v: TextView) = v.text?.toString() ?: ""
