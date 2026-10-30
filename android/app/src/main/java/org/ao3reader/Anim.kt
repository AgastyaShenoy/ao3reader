package org.ao3reader

import android.animation.ValueAnimator
import android.view.View
import android.view.animation.DecelerateInterpolator
import android.view.animation.PathInterpolator

/**
 * Short interface animations. All of them are skipped when the user turns animations off in
 * Settings, or when Android's own "remove animations" accessibility option is on.
 */
object Anim {
    val on: Boolean get() = Settings.config.optBoolean("animations", true) && ValueAnimator.areAnimatorsEnabled()

    /** Material's "emphasized decelerate" curve: quick start, gentle landing. */
    val ease = PathInterpolator(0.05f, 0.7f, 0.1f, 1f)
    val decel = DecelerateInterpolator(1.6f)

    /** Fades a view in, optionally sliding it from `dx`/`dy`. */
    fun enter(v: View, dx: Float = 0f, dy: Float = 0f, ms: Long = 180, alpha: Float = 0f) {
        v.animate().cancel()
        if (!on) {
            v.alpha = 1f; v.translationX = 0f; v.translationY = 0f
            return
        }
        v.alpha = alpha
        v.translationX = dx
        v.translationY = dy
        v.animate().alpha(1f).translationX(0f).translationY(0f).setDuration(ms).setInterpolator(ease).withEndAction(null).start()
    }

    /** Fades a view out (optionally sliding to `dx`/`dy`), then runs `end`. */
    fun exit(v: View, dx: Float = 0f, dy: Float = 0f, ms: Long = 140, end: () -> Unit) {
        v.animate().cancel()
        if (!on) return end()
        v.animate().alpha(0f).translationX(dx).translationY(dy).setDuration(ms).setInterpolator(decel).withEndAction(end).start()
    }
}
