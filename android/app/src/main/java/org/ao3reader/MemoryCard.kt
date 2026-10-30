package org.ao3reader

import android.app.ActivityManager
import android.content.Context
import android.os.Debug
import android.os.Handler
import android.os.Looper
import android.view.View
import android.widget.LinearLayout
import android.widget.TextView
import java.util.concurrent.Executors

/**
 * Live memory readout for Settings: this app's PSS (the measure Android's own memory screens
 * use) split by where it goes. Measures every two seconds, only while the card is on screen.
 */
class MemoryCard(private val a: MainActivity) : LinearLayout(a) {
    private class Part(val keys: List<String>, val value: TextView, val bar: View, val rest: View)

    private val p = Settings.pal
    private val total = a.label("…", 26f, p.text, bold = true)
    private val swap = a.label("", 12.5f, p.muted)
    private val device = a.label("", 12.5f, p.muted)
    private val parts = mutableListOf<Part>()
    private val main = Handler(Looper.getMainLooper())
    private var running = false

    private val tick = object : Runnable {
        override fun run() {
            measurer.execute {
                // Reading /proc/self/smaps takes a few milliseconds, so keep it off the UI thread.
                val kb = Debug.MemoryInfo().also { Debug.getMemoryInfo(it) }.memoryStats
                    .mapValues { it.value.toLongOrNull() ?: 0L }.toMutableMap()
                // The core counts its own allocations; the rest of the native heap is Android's.
                val core = runCatching { Core.obj("memory") }.getOrNull()
                val rust = (core?.optLong("live") ?: 0L) / 1024
                kb["rust"] = rust
                kb["android-native"] = ((kb["summary.native-heap"] ?: 0L) - rust).coerceAtLeast(0)
                val rustPeak = (core?.optLong("peak") ?: 0L) / 1024
                val am = a.getSystemService(Context.ACTIVITY_SERVICE) as ActivityManager
                val phone = ActivityManager.MemoryInfo().also { am.getMemoryInfo(it) }
                main.post { show(kb, rustPeak, phone, am.isLowRamDevice) }
            }
            main.postDelayed(this, 2000)
        }
    }

    init {
        orientation = VERTICAL
        background = rounded(p.card, a.dpf(12), p.border, a.dp(1))
        setPadding(a.dp(16), a.dp(14), a.dp(16), a.dp(14))
        addView(a.label("In use now", 12.5f, p.muted))
        addView(total)
        addView(swap)
        swap.gone(true)
        listOf(
            "Rust core (library, text, parsing)" to listOf("rust"),
            "Android native (drawing, fonts)" to listOf("android-native"),
            "Kotlin interface" to listOf("summary.java-heap"),
            "Graphics" to listOf("summary.graphics"),
            "Code (app and Android libraries)" to listOf("summary.code"),
            "Shared with Android" to listOf("summary.system"),
            "Other" to listOf("summary.stack", "summary.private-other"),
        ).forEach { (name, keys) ->
            val head = a.row()
            head.addView(a.label(name, 13.5f, p.text), LayoutParams(0, -2, 1f))
            val value = a.label("", 13.5f, p.muted)
            head.addView(value)
            addView(head, lp(top = a.dp(10)))
            val track = a.row().apply { background = rounded(alpha(p.text, 0.08f), a.dpf(2)) }
            val bar = View(a).apply { background = rounded(p.accentMark, a.dpf(2)) }
            val rest = View(a)
            track.addView(bar, LayoutParams(0, -1, 0f))
            track.addView(rest, LayoutParams(0, -1, 1f))
            addView(track, lp(h = a.dp(4), top = a.dp(4)))
            parts += Part(keys, value, bar, rest)
        }
        addView(device, lp(top = a.dp(14)))
        addView(a.label("PSS: memory Android shares between apps (system libraries, fonts) is split among the apps using it. Graphics grows with screen resolution.", 12f, p.muted), lp(top = a.dp(4)))
    }

    private fun show(kb: Map<String, Long>, rustPeak: Long, phone: ActivityManager.MemoryInfo, lowRam: Boolean) {
        val all = kb["summary.total-pss"] ?: 0L
        total.text = mb(all)
        val swapped = kb["summary.total-swap"] ?: 0L
        swap.text = "plus ${mb(swapped)} compressed in ZRAM"
        swap.gone(swapped < 512)
        for (part in parts) {
            val v = part.keys.sumOf { kb[it] ?: 0L }
            part.value.text = if (part.keys == listOf("rust")) "${mb(v)} · peak ${mb(rustPeak)}" else mb(v)
            part.bar.layoutParams = LayoutParams(0, -1, v.toFloat())
            part.rest.layoutParams = LayoutParams(0, -1, (all - v).coerceAtLeast(0).toFloat())
        }
        val gb = 1024.0 * 1024 * 1024
        device.text = "Phone: %.1f GB free of %.1f GB".format(phone.availMem / gb, phone.totalMem / gb) +
            if (lowRam) " · low-RAM device" else ""
    }

    private fun mb(kb: Long) = "%.1f MB".format(kb / 1024.0)

    // Window visibility drops when the app goes to the background, so measuring stops there too.
    override fun onWindowVisibilityChanged(visibility: Int) {
        super.onWindowVisibilityChanged(visibility)
        if (visibility == View.VISIBLE) start() else stop()
    }

    override fun onDetachedFromWindow() {
        stop()
        super.onDetachedFromWindow()
    }

    private fun start() {
        if (running) return
        running = true
        main.post(tick)
    }

    private fun stop() {
        running = false
        main.removeCallbacks(tick)
    }

    companion object {
        private val measurer = Executors.newSingleThreadExecutor { r -> Thread(r, "memory").apply { isDaemon = true } }
    }
}
