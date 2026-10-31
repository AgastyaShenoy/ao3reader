package org.ao3reader

import android.app.Dialog
import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Paint
import android.graphics.Typeface
import android.graphics.drawable.ColorDrawable
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.RippleDrawable
import android.text.InputType
import android.text.TextUtils
import android.view.ContextThemeWrapper
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.Window
import android.view.WindowManager
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.PopupMenu
import android.widget.ScrollView
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
        typeface = Settings.face(bold)
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

/** Round icon button with a ripple. `tip` names it: shown on long-press (or mouse hover) and read
 *  by screen readers. `large` is for the reader's bars, reached for while holding the device. */
fun Context.iconButton(res: Int, color: Int, tip: String, active: Boolean = false, large: Boolean = false, onClick: () -> Unit): FrameLayout {
    val p = Settings.pal
    return FrameLayout(this).apply {
        // 42 dp, or the 48 dp Android recommends with larger touch targets. Large is 58 or 64 dp on
        // tablets, and 46 or 48 dp on phones, where seven in a row still leave room for the title.
        val wide = resources.configuration.screenWidthDp >= 600
        val glyph = if (large) (if (wide) 28 else 24) else 22
        val pad = dp(when {
            large && wide -> if (Settings.big) 18 else 15
            large -> if (Settings.big) 12 else 11
            else -> if (Settings.big) 13 else 10
        })
        val radius = dpf(glyph / 2f) + pad
        setPadding(pad, pad, pad, pad)
        background = RippleDrawable(ColorStateList.valueOf(alpha(p.text, 0.15f)), if (active) rounded(p.selected, radius) else null, rounded(0xFFFFFFFF.toInt(), radius))
        addView(icon(res, color, glyph).apply { layoutParams = FrameLayout.LayoutParams(dp(glyph), dp(glyph)) })
        isClickable = true
        setOnClickListener { onClick() }
        tooltipText = tip
        contentDescription = tip
    }
}

/** Primary: the one main action on a screen (accent fill). Tonal: a card's main action, repeated
 *  down a list (quiet neutral fill). */
enum class Kind { Primary, Tonal, Secondary, Ghost, Danger }

fun Context.button(text: String, kind: Kind = Kind.Secondary, iconRes: Int = 0, onClick: () -> Unit): TextView {
    val p = Settings.pal
    val (fill, fg) = when (kind) {
        Kind.Primary -> p.accent to p.onAccent
        Kind.Tonal -> alpha(p.text, 0.09f) to p.text
        Kind.Danger -> p.danger to p.onAccent
        Kind.Secondary -> p.card to p.text
        Kind.Ghost -> 0 to p.text
    }
    return TextView(this).apply {
        this.text = text
        textSize = 14f
        setTextColor(fg)
        gravity = Gravity.CENTER
        typeface = Settings.face(kind == Kind.Primary || kind == Kind.Tonal)
        val padY = dp(if (Settings.big) 13 else 9)
        setPadding(dp(if (iconRes != 0) 12 else 16), padY, dp(16), padY)
        if (Settings.big) minHeight = dp(48)
        if (iconRes != 0) {
            val d = getDrawable(iconRes)!!.mutate()
            d.setTint(fg)
            d.setBounds(0, 0, dp(18), dp(18))
            setCompoundDrawablesRelative(d, null, null, null)
            compoundDrawablePadding = dp(6)
        }
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
    typeface = Settings.face()
    textSize = 15f
    isSingleLine = true
    inputType = if (password) InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD else InputType.TYPE_CLASS_TEXT
    // After the input type: a password type switches the font to monospace.
    typeface = Settings.face()
    background = rounded(p.inputBg, dpf(8), p.border, dp(1))
    setPadding(dp(12), dp(10), dp(12), dp(10))
}

/** One of AO3's coloured markers (rating, category, warning): the colour is a dot on a quiet pill,
 *  so a card with several of them doesn't become a row of bright blocks. */
fun Context.markerPill(text: String, dot: Int): TextView = label(text, 12f, Settings.pal.text, lines = 1).apply {
    background = rounded(alpha(Settings.pal.text, 0.07f), dpf(12))
    setPadding(dp(9), dp(3), dp(10), dp(4))
    val d = GradientDrawable().apply { shape = GradientDrawable.OVAL; setColor(dot); setSize(dp(7), dp(7)) }
    d.setBounds(0, 0, dp(7), dp(7))
    setCompoundDrawablesRelative(d, null, null, null)
    compoundDrawablePadding = dp(6)
}

/** An outlined tag (relationships, fandoms); `quiet` for status words like "Complete". */
fun Context.tagPill(text: String, quiet: Boolean = false): TextView {
    val p = Settings.pal
    return label(text, 12f, if (quiet) p.muted else p.text, lines = 1).apply {
        background = rounded(0, dpf(12), p.border, dp(1))
        setPadding(dp(10), dp(3), dp(10), dp(4))
    }
}

/** A relationship tag. Romantic ones ("A/B") get a heart on a rose tint, the bit of colour a reader
 *  looks for first; platonic ones ("A & B") a person sign on a plain outline. */
fun Context.shipPill(text: String): TextView {
    val p = Settings.pal
    val love = '/' in text
    return label(text, 12f, p.text, bold = love, lines = 1).apply {
        background = if (love) rounded(alpha(p.romance, 0.14f), dpf(12), alpha(p.romance, 0.45f), dp(1)) else rounded(0, dpf(12), p.border, dp(1))
        setPadding(dp(9), dp(3), dp(10), dp(4))
        val d = getDrawable(if (love) R.drawable.ic_heart_fill else R.drawable.ic_person)!!.mutate()
        d.setTint(if (love) p.romance else p.muted)
        d.setBounds(0, 0, dp(12), dp(12))
        setCompoundDrawablesRelative(d, null, null, null)
        compoundDrawablePadding = dp(5)
    }
}

/** A small rounded number: how many works a filter holds (quiet), or how many are new (accent
 *  tint). */
fun Context.countBadge(n: Int, new: Boolean = false, strong: Boolean = false): TextView {
    val p = Settings.pal
    val (fill, fg) = if (new) alpha(p.accentText, 0.18f) to p.accentText else alpha(p.text, if (strong) 0.16f else 0.09f) to (if (strong) p.text else p.muted)
    return label(if (n > 999) "999+" else "$n", 11f, fg, bold = true, lines = 1).apply {
        gravity = Gravity.CENTER
        minWidth = dp(18)
        background = rounded(fill, dpf(9))
        setPadding(dp(6), dp(1), dp(6), dp(1))
    }
}

/** A filter chip with a count badge (none for 0); `new` when the count is of new works. */
fun Context.countChip(text: String, n: Int, selected: Boolean, new: Boolean = false, onClick: () -> Unit): View {
    val p = Settings.pal
    return row().apply {
        background = if (selected) rounded(p.selected, dpf(16)) else rounded(0, dpf(16), p.border, dp(1))
        val padY = dp(if (Settings.big) 12 else 7)
        setPadding(dp(14), padY, dp(if (n > 0) 10 else 14), padY)
        addView(label(text, 13.5f, p.text, bold = selected, lines = 1))
        if (n > 0) addView(countBadge(n, new, strong = selected), lp(LinearLayout.LayoutParams.WRAP_CONTENT).apply { leftMargin = dp(6) })
        isSelected = selected
        contentDescription = if (n > 0) "$text, $n" else text
        setOnClickListener { onClick() }
    }
}

/** One of a row of equally wide tool buttons (Filters, sort, direction). */
fun Context.toolButton(text: String, iconRes: Int, active: Boolean, onClick: (View) -> Unit): TextView {
    val p = Settings.pal
    return label(text, 13f, p.text, bold = active, lines = 1).apply {
        gravity = Gravity.CENTER
        background = if (active) rounded(p.selected, dpf(18)) else rounded(0, dpf(18), p.border, dp(1))
        val padY = dp(if (Settings.big) 13 else 8)
        setPadding(dp(10), padY, dp(10), padY)
        val d = getDrawable(iconRes)!!.mutate()
        d.setTint(p.text)
        d.setBounds(0, 0, dp(16), dp(16))
        setCompoundDrawablesRelative(d, null, null, null)
        compoundDrawablePadding = dp(6)
        setOnClickListener { onClick(it) }
    }
}

/** "New" (accent) or "Updated" (neutral) next to a followed work. */
fun Context.markBadge(text: String): TextView {
    val p = Settings.pal
    val isNew = text == "New"
    return label(text, 12f, if (isNew) p.accentText else p.text, bold = true, lines = 1).apply {
        background = rounded(if (isNew) alpha(p.accentText, 0.16f) else p.selected, dpf(12))
        setPadding(dp(9), dp(3), dp(9), dp(4))
    }
}

/** A problem the user can't fix in place (AO3 down, a failed download): a plain card with an amber
 *  warning sign, noticeable without turning the screen red. */
fun Context.notice(title: String, msg: String): LinearLayout {
    val p = Settings.pal
    return column().apply {
        background = rounded(p.card, dpf(12), p.border, dp(1))
        setPadding(dp(16), dp(12), dp(16), dp(14))
        val head = row()
        head.addView(icon(R.drawable.ic_warning, p.warningText, 18), LinearLayout.LayoutParams(dp(18), dp(18)).apply { rightMargin = dp(10) })
        head.addView(label(title, 14f, p.text, bold = true))
        addView(head)
        addView(label(msg, 13.5f, alpha(p.text, 0.85f)).apply { setLineSpacing(0f, 1.1f); setPadding(dp(28), 0, 0, 0) }, lp(top = dp(4)))
    }
}

/** Horizontally scrolling row of pills that fades out at the edge instead of cutting a pill off. */
fun Context.pillScroller(pills: View): View = android.widget.HorizontalScrollView(this).apply {
    isHorizontalScrollBarEnabled = false
    isHorizontalFadingEdgeEnabled = true
    setFadingEdgeLength(dp(28))
    addView(pills)
}

/** Row of mutually exclusive options, optionally each with a count badge (`newCounts`: the
 *  counts are new works, shown in the accent tint). */
fun Context.segmented(options: List<String>, selected: Int, counts: List<Int> = emptyList(), newCounts: Boolean = false, onPick: (Int) -> Unit): LinearLayout {
    val p = Settings.pal
    val box = row().apply {
        background = rounded(p.bg, dpf(8), p.border, dp(1))
        setPadding(dp(3), dp(3), dp(3), dp(3))
    }
    // Every segment is as wide as the widest label in bold, so equal shares never squeeze the
    // selected (bold) one onto two lines.
    val bold = Paint().apply { textSize = android.util.TypedValue.applyDimension(android.util.TypedValue.COMPLEX_UNIT_SP, 13.5f, resources.displayMetrics); typeface = Settings.face(true) }
    // Four or more share a phone's width: smaller text and padding keep the labels whole.
    val many = options.size >= 4
    val size = if (many) 12.5f else 13.5f
    if (many) bold.textSize = android.util.TypedValue.applyDimension(android.util.TypedValue.COMPLEX_UNIT_SP, size, resources.displayMetrics)
    val minW = (options.maxOf { bold.measureText(it) } + dp(if (many) 8 else 18) + if (!many && counts.any { it > 0 }) dp(26) else 0).toInt()
    options.forEachIndexed { i, o ->
        val sel = i == selected
        // Four tabs leave no room for count badges on a phone.
        val n = if (many) 0 else counts.getOrElse(i) { 0 }
        val text = label(o, size, if (sel) p.text else p.muted, bold = sel, lines = 1).apply { gravity = Gravity.CENTER }
        val seg: View = if (n == 0) text else row().apply {
            gravity = Gravity.CENTER
            addView(text)
            addView(countBadge(n, newCounts, sel), lp(LinearLayout.LayoutParams.WRAP_CONTENT).apply { leftMargin = dp(6) })
        }
        seg.minimumWidth = minW
        val padY = dp(if (Settings.big) 12 else 7)
        seg.setPadding(dp(if (many) 3 else 8), padY, dp(if (many) 3 else 8), padY)
        if (sel) seg.background = rounded(p.selected, dpf(6))
        // TalkBack says which one is chosen.
        seg.isSelected = sel
        seg.contentDescription = if (n > 0) "$o, $n" else o
        seg.setOnClickListener { if (!sel) onPick(i) }
        box.addView(seg, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
    }
    return box
}

/** A dialog in the reader's theme (the platform's own are always dark grey, whatever the theme):
 *  a title, a message or a form, Cancel and the one action. `onOk` returns false to stay open, for a
 *  field to fix. */
fun Context.dialog(
    title: String,
    message: String? = null,
    body: View? = null,
    ok: String? = null,
    okKind: Kind = Kind.Primary,
    cancel: String = "Cancel",
    onOk: () -> Boolean = { true },
): Dialog {
    val p = Settings.pal
    val d = Dialog(this)
    d.requestWindowFeature(Window.FEATURE_NO_TITLE)
    val box = column().apply {
        background = rounded(p.card, dpf(16), p.border, dp(1))
        setPadding(dp(22), dp(20), dp(22), dp(16))
    }
    box.addView(label(title, 19f, p.text, bold = true))
    if (message != null) box.addView(label(message, 14.5f, alpha(p.text, 0.82f)).apply { setLineSpacing(0f, 1.15f) }, lp(top = dp(8)))
    if (body != null) box.addView(body, lp(top = dp(16)))
    val buttons = row().apply { gravity = Gravity.END or Gravity.CENTER_VERTICAL }
    buttons.addView(button(cancel, if (ok == null) Kind.Tonal else Kind.Ghost) { d.dismiss() })
    if (ok != null) buttons.addView(button(ok, okKind) { if (onOk()) d.dismiss() }, LinearLayout.LayoutParams(-2, -2).apply { leftMargin = dp(8) })
    box.addView(buttons, lp(top = dp(20)))
    d.setContentView(ScrollView(this).apply { isVerticalScrollBarEnabled = false; addView(box) })
    d.window?.apply {
        setBackgroundDrawable(ColorDrawable(0))
        setDimAmount(if (p.dark) 0.6f else 0.4f)
        setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE)
    }
    d.show()
    d.window?.setLayout(minOf(resources.displayMetrics.widthPixels - dp(32), dp(440)), ViewGroup.LayoutParams.WRAP_CONTENT)
    return d
}

/** A pop-up menu in the theme's light or dark. */
fun Context.popupMenu(anchor: View): PopupMenu =
    PopupMenu(ContextThemeWrapper(this, if (Settings.pal.dark) android.R.style.Theme_Material else android.R.style.Theme_Material_Light), anchor)

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
