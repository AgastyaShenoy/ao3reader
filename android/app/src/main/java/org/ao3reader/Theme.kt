package org.ao3reader

import android.graphics.Typeface
import org.json.JSONObject

/** One colour theme, shared with the desktop app through the Rust core. */
class Pal(j: JSONObject) {
    val id: String = j.getString("id")
    val name: String = j.getString("name")
    val dark: Boolean = j.getBoolean("dark")
    val bg = opaque(j.getInt("bg"))
    val surface = opaque(j.getInt("surface"))
    val card = opaque(j.getInt("card"))
    val text = opaque(j.getInt("text"))
    val muted = opaque(j.getInt("muted"))
    val accent = opaque(j.getInt("accent"))
    val border = opaque(j.getInt("border"))
    val quoteBg = opaque(j.getInt("quote_bg"))
    val notesBg = opaque(j.getInt("notes_bg"))
    /** The accent as text (softened on dark themes so it stays readable). */
    val accentText = opaque(j.optInt("accent_text", j.getInt("accent")))
    /** The accent for the logo, progress and indicators: saturated, 3:1 on cards. */
    val accentMark = opaque(j.optInt("accent_mark", j.getInt("accent")))
    val dangerText = opaque(j.optInt("danger_text", 0xDC3C3C))
    val warningText = opaque(j.optInt("warning_text", 0xC08000))
    /** Fill for the current choice in tabs, segments and chips: neutral, so the accent is left for
     *  the one main action on a screen. */
    val selected = alpha(text, 0.12f)
    /** Text on an accent fill: white, or black on High Contrast's yellow. */
    val onAccent = opaque(j.optInt("on_accent", 0xFFFFFF))
    /** Hearts on romantic relationship tags. */
    val romance = opaque(j.optInt("romance", 0xE11D48))
    val danger = 0xFFDC3C3C.toInt()
    val success = 0xFF22A054.toInt()
    val inputBg = if (dark) bg else 0xFFFFFFFF.toInt()

    companion object {
        fun opaque(rgb: Int) = rgb or 0xFF000000.toInt()
    }
}

/** App settings (the core's config.json) and the active theme. */
object Settings {
    lateinit var themes: List<Pal>
    lateinit var config: JSONObject
    /** The phone is in dark mode (for "Match system"). */
    var night = false

    /** The theme in use: the one chosen, or with "Match system" the light or the dark one. */
    val themeId: String get() = when {
        !config.optBoolean("follow_system") -> config.optString("theme")
        night -> config.optString("dark_theme", "Dark")
        else -> config.optString("light_theme", "Linen")
    }
    val pal: Pal get() = themes.firstOrNull { it.id == themeId } ?: themes[0]
    /** Bigger buttons with more room between them (48 dp touch targets). */
    val big: Boolean get() = config.optBoolean("larger_targets")

    /** Theme colours again, after "Your colours" change. */
    fun reloadThemes() {
        themes = Core.arr("themes").objects().map(::Pal)
    }
    val typo: JSONObject get() = config.getJSONObject("typography")

    /** The interface font family ("serif", "monospace", …), or null for the system face. */
    val uiFamily: String? get() = typo.optString("ui_font").takeIf { it.isNotEmpty() && it != "null" }

    /** The interface typeface, bold or regular. */
    fun face(bold: Boolean = false): Typeface {
        val base = uiFamily?.let { Typeface.create(it, Typeface.NORMAL) } ?: Typeface.DEFAULT
        return Typeface.create(base, if (bold) Typeface.BOLD else Typeface.NORMAL)
    }

    fun load() {
        config = Core.obj("config.get")
    }

    fun save() {
        Core.req("config.set", json("config" to config))
    }
}

fun alpha(color: Int, a: Float): Int = (color and 0x00FFFFFF) or ((a * 255).toInt().coerceIn(0, 255) shl 24)

/** WCAG contrast ratio of two colours (1–21). */
fun contrast(a: Int, b: Int): Double {
    fun lum(c: Int): Double {
        fun ch(v: Int): Double = (v / 255.0).let { if (it <= 0.04045) it / 12.92 else Math.pow((it + 0.055) / 1.055, 2.4) }
        return 0.2126 * ch((c shr 16) and 0xFF) + 0.7152 * ch((c shr 8) and 0xFF) + 0.0722 * ch(c and 0xFF)
    }
    val (x, y) = lum(a) to lum(b)
    return (maxOf(x, y) + 0.05) / (minOf(x, y) + 0.05)
}
