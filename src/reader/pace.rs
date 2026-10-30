//! Works out how fast the user reads, from how quickly their position moves through a work.
//! Only steady reading counts: jumps (a chapter picked from the contents, a search hit),
//! going back, and pauses (the book left open) are thrown away.

use serde::{Deserialize, Serialize};

/// Reading time measured before the estimate is used for "time left".
const MIN_SECS: f64 = 180.0;
/// Older reading fades out once this much is measured, so the estimate follows the user.
const WINDOW_SECS: f64 = 4.0 * 3600.0;
/// Position changes are judged over stretches at least this long.
const SEGMENT_SECS: f64 = 30.0;
/// No movement for this long means the user stepped away.
const IDLE_SECS: f64 = 600.0;
/// One move further than this is a jump, not a page turn.
const MAX_STEP_WORDS: f64 = 1500.0;
/// Speeds outside this range are skimming or something else, not reading.
const MIN_WPM: f64 = 60.0;
const MAX_WPM: f64 = 900.0;

/// Words read and the time spent reading them, kept in the settings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ReadingSpeed {
    pub words: f64,
    pub secs: f64,
}

impl ReadingSpeed {
    /// Words per minute, once enough reading has been measured.
    pub fn wpm(&self) -> Option<u32> {
        (self.secs >= MIN_SECS && self.words > 0.0).then(|| (self.words / self.secs * 60.0).round() as u32)
    }

    /// Minutes of reading measured so far.
    pub fn minutes(&self) -> u32 {
        (self.secs / 60.0) as u32
    }

    fn add(&mut self, words: f64, secs: f64) {
        self.words += words;
        self.secs += secs;
        if self.secs > WINDOW_SECS {
            let k = WINDOW_SECS / self.secs;
            self.words *= k;
            self.secs *= k;
        }
    }
}

/// Follows one reading session. Feed it every position change.
#[derive(Debug, Clone, Default)]
pub struct PaceTracker {
    /// Start of the stretch being measured: (time, words read).
    anchor: Option<(f64, f64)>,
    /// The last position seen: (time, words read).
    last: Option<(f64, f64)>,
}

impl PaceTracker {
    /// `t` is seconds on any steady clock; `words` is how far into the work the reader is.
    /// Returns true when `speed` took in a new stretch of reading.
    pub fn observe(&mut self, t: f64, words: f64, speed: &mut ReadingSpeed) -> bool {
        let Some((last_t, last_w)) = self.last.replace((t, words)) else {
            self.anchor = Some((t, words));
            return false;
        };
        let step = words - last_w;
        if !(0.0..=MAX_STEP_WORDS).contains(&step) || t - last_t > IDLE_SECS {
            self.anchor = Some((t, words));
            return false;
        }
        let (t0, w0) = *self.anchor.get_or_insert((t, words));
        let dt = t - t0;
        if dt < SEGMENT_SECS {
            return false;
        }
        self.anchor = Some((t, words));
        let read = words - w0;
        let wpm = read / dt * 60.0;
        if (MIN_WPM..=MAX_WPM).contains(&wpm) {
            speed.add(read, dt);
            true
        } else {
            false
        }
    }

    /// Starts over, e.g. when another work is opened.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Turns a page of `words` every `secs` seconds, `n` times, starting at `t`.
    fn read(p: &mut PaceTracker, s: &mut ReadingSpeed, t: &mut f64, w: &mut f64, words: f64, secs: f64, n: usize) {
        for _ in 0..n {
            *t += secs;
            *w += words;
            p.observe(*t, *w, s);
        }
    }

    #[test]
    fn steady_reading_gives_its_speed() {
        let (mut p, mut s) = (PaceTracker::default(), ReadingSpeed::default());
        let (mut t, mut w) = (0.0, 0.0);
        p.observe(t, w, &mut s);
        // 300 words a minute: 250-word pages every 50 seconds, for ten minutes.
        read(&mut p, &mut s, &mut t, &mut w, 250.0, 50.0, 12);
        assert_eq!(s.wpm(), Some(300));
    }

    #[test]
    fn needs_a_few_minutes_first() {
        let (mut p, mut s) = (PaceTracker::default(), ReadingSpeed::default());
        let (mut t, mut w) = (0.0, 0.0);
        p.observe(t, w, &mut s);
        read(&mut p, &mut s, &mut t, &mut w, 250.0, 50.0, 2);
        assert_eq!(s.wpm(), None);
    }

    #[test]
    fn jumps_and_pauses_are_ignored() {
        let (mut p, mut s) = (PaceTracker::default(), ReadingSpeed::default());
        let (mut t, mut w) = (0.0, 0.0);
        p.observe(t, w, &mut s);
        read(&mut p, &mut s, &mut t, &mut w, 200.0, 60.0, 5);
        // Jump ahead a few chapters, leave the book open for an hour, flick back.
        w += 20_000.0;
        t += 5.0;
        p.observe(t, w, &mut s);
        t += 3600.0;
        w += 200.0;
        p.observe(t, w, &mut s);
        w -= 5000.0;
        t += 2.0;
        p.observe(t, w, &mut s);
        read(&mut p, &mut s, &mut t, &mut w, 200.0, 60.0, 5);
        assert_eq!(s.wpm(), Some(200));
    }

    #[test]
    fn skimming_is_ignored() {
        let (mut p, mut s) = (PaceTracker::default(), ReadingSpeed::default());
        let (mut t, mut w) = (0.0, 0.0);
        p.observe(t, w, &mut s);
        read(&mut p, &mut s, &mut t, &mut w, 250.0, 60.0, 6);
        // Flicking through pages a second each.
        read(&mut p, &mut s, &mut t, &mut w, 300.0, 1.0, 60);
        read(&mut p, &mut s, &mut t, &mut w, 250.0, 60.0, 6);
        assert_eq!(s.wpm(), Some(250));
    }
}
