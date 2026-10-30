package org.ao3reader

import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.RippleDrawable
import android.text.InputType
import android.text.TextUtils
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import kotlin.math.roundToInt

// Small helpers for building screens in code with the platform's own widgets.

fun Context.dp(v: Number): Int = (v.toFloat() * resources.displayMetrics.density).roundToInt()
fun Context.dpf(v: Number): Float = v.toFloat() * resources.displayMetrics.density

fun rounded(color: Int, radius: Float, stroke: Int = 0, strokeWidth: Int = 0): GradientDrawable =
    GradientDrawable().apply {
        setColor(color)
        cornerRadius = radius
        if (strokeWidth > 0) setStroke(strokeWidth, stroke)
    }

fun Context.label(text: CharSequence, sp: Float, color: Int, bold: Boolean = false, lines: Int = 0): TextView =
    TextView(this).apply {
        this.text = text
        textSize = sp
        setTextColor(color)
        if (bold) typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
        if (lines > 0) {
            maxLines = lines
            ellipsize = TextUtils.TruncateAt.END
        }
    }

fun Context.column(): LinearLayout = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
fun Context.row(): LinearLayout = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL }

fun Context.icon(res: Int, color: Int, size: Int = 24): ImageView = ImageView(this).apply {
    setImageResource(res)
    imageTintList = ColorStateList.valueOf(color)
    layoutParams = LinearLayout.LayoutParams(dp(size), dp(size))
}

/** Round icon button with a ripple. */
fun Context.iconButton(res: Int, color: Int, active: Boolean = false, onClick: () -> Unit): FrameLayout {
    val p = Settings.pal
    return FrameLayout(this).apply {
        val pad = dp(10)
        setPadding(pad, pad, pad, pad)
        background = RippleDrawable(ColorStateList.valueOf(alpha(p.text, 0.15f)), if (active) rounded(alpha(p.accent, 0.2f), dpf(22)) else null, rounded(0xFFFFFFFF.toInt(), dpf(22)))
        addView(icon(res, color, 22).apply { layoutParams = FrameLayout.LayoutParams(dp(22), dp(22)) })
        isClickable = true
        setOnClickListener { onClick() }
    }
}

enum class Kind { Primary, Secondary, Ghost, Danger }

fun Context.button(text: String, kind: Kind = Kind.Secondary, onClick: () -> Unit): TextView {
    val p = Settings.pal
    val (fill, fg) = when (kind) {
        Kind.Primary -> p.accent to p.onAccent
        Kind.Danger -> p.danger to p.onAccent
        Kind.Secondary -> p.card to p.text
        Kind.Ghost -> 0 to p.text
    }
    return TextView(this).apply {
        this.text = text
        textSize = 14f
        setTextColor(fg)
        gravity = Gravity.CENTER
        if (kind == Kind.Primary) typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
        setPadding(dp(16), dp(9), dp(16), dp(9))
        val shape = rounded(fill, dpf(8), p.border, if (kind == Kind.Secondary) dp(1) else 0)
        background = RippleDrawable(ColorStateList.valueOf(alpha(p.text, 0.15f)), shape, rounded(0xFFFFFFFF.toInt(), dpf(8)))
        isClickable = true
        setOnClickListener { onClick() }
    }
}

fun Context.card(): LinearLayout = column().apply {
    val p = Settings.pal
    background = rounded(p.card, dpf(12), p.border, dp(1))
    setPadding(dp(16), dp(14), dp(16), dp(14))
}

fun Context.input(hint: String, password: Boolean = false): EditText = EditText(this).apply {
    val p = Settings.pal
    this.hint = hint
    setHintTextColor(p.muted)
    setTextColor(p.text)
    textSize = 15f
    isSingleLine = true
    inputType = if (password) InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD else InputType.TYPE_CLASS_TEXT
    background = rounded(p.inputBg, dpf(8), p.border, dp(1))
    setPadding(dp(12), dp(10), dp(12), dp(10))
}

/** A pill-shaped tag. */
fun Context.pill(text: String, fill: Int, fg: Int, outlined: Boolean = false): TextView = label(text, 12f, fg, lines = 1).apply {
    background = if (outlined) rounded(0, dpf(12), fill, dp(1)) else rounded(fill, dpf(12))
    setPadding(dp(10), dp(3), dp(10), dp(4))
}

/** Row of mutually exclusive options. */
fun Context.segmented(options: List<String>, selected: Int, onPick: (Int) -> Unit): LinearLayout {
    val p = Settings.pal
    val box = row().apply {
        background = rounded(p.bg, dpf(8), p.border, dp(1))
        setPadding(dp(3), dp(3), dp(3), dp(3))
    }
    options.forEachIndexed { i, o ->
        val sel = i == selected
        box.addView(label(o, 13.5f, if (sel) p.onAccent else p.text, bold = sel).apply {
            gravity = Gravity.CENTER
            setPadding(dp(8), dp(7), dp(8), dp(7))
            if (sel) background = rounded(p.accent, dpf(6))
            setOnClickListener { if (!sel) onPick(i) }
        }, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
    }
    return box
}

fun lp(w: Int = ViewGroup.LayoutParams.MATCH_PARENT, h: Int = ViewGroup.LayoutParams.WRAP_CONTENT, top: Int = 0, bottom: Int = 0): LinearLayout.LayoutParams =
    LinearLayout.LayoutParams(w, h).apply { topMargin = top; bottomMargin = bottom }

fun View.gone(hide: Boolean) {
    visibility = if (hide) View.GONE else View.VISIBLE
}

fun frame(w: Int = ViewGroup.LayoutParams.MATCH_PARENT, h: Int = ViewGroup.LayoutParams.MATCH_PARENT, gravity: Int = Gravity.NO_GRAVITY) =
    FrameLayout.LayoutParams(w, h, gravity)

fun thousands(n: Long): String = "%,d".format(n)

fun minutes(words: Long, wpm: Int): String {
    val m = (words.toDouble() / wpm.coerceAtLeast(60)).roundToInt().coerceAtLeast(1)
    return if (m >= 60) "${m / 60} h ${m % 60} min" else "$m min"
}
