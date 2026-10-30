package org.ao3reader

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
    val onAccent = 0xFFFFFFFF.toInt()
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

    val pal: Pal get() = themes.firstOrNull { it.id == config.optString("theme") } ?: themes[0]
    val typo: JSONObject get() = config.getJSONObject("typography")

    fun load() {
        config = Core.obj("config.get")
    }

    fun save() {
        Core.req("config.set", json("config" to config))
    }
}

fun alpha(color: Int, a: Float): Int = (color and 0x00FFFFFF) or ((a * 255).toInt().coerceIn(0, 255) shl 24)
