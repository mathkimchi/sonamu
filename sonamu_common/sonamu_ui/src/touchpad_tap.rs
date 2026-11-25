//! Tapping the touchpad to click (#170): libinput detects taps; this decides which become clicks

use std::{
    collections::VecDeque,
    sync::atomic::{AtomicU8, AtomicU32, Ordering},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Tap {
    /// Taps don't click
    None,
    /// Each tap clicks
    Single,
    /// A tap only clicks right after moving the cursor or another tap (within [`window_ms`]), so stray taps
    /// (eg while typing) do nothing. Move, tap, then touch and move drags (libinput's tap-and-drag)
    Smart,
}

/// Set from the settings (`touchpad_tap`)
static MODE: AtomicU8 = AtomicU8::new(Tap::None as u8);

pub fn set_mode(tap: Tap) {
    MODE.store(tap as u8, Ordering::Relaxed);
}

#[must_use]
pub fn mode() -> Tap {
    match MODE.load(Ordering::Relaxed) {
        0 => Tap::None,
        1 => Tap::Single,
        _ => Tap::Smart,
    }
}

/// For [`Tap::Smart`], from moving or a tap's release to the next tap's press; set from the settings (`double_click`)
static WINDOW_MS: AtomicU32 = AtomicU32::new(400);

pub fn set_window_ms(ms: u32) {
    WINDOW_MS.store(ms, Ordering::Relaxed);
}

#[must_use]
pub fn window_ms() -> u32 {
    WINDOW_MS.load(Ordering::Relaxed)
}

/// The most `window_ms` can be (`double_click`'s range), so older motions are forgotten
const MAX_WINDOW_MS: u32 = 2000;

/// Drops the button events of taps that shouldn't click
#[derive(Debug, Default)]
pub struct TapFilter {
    /// Taps' buttons held now, and whether their press was passed on
    held: Vec<(u32, bool)>,
    /// When the last tap was released
    last_tap: Option<u32>,
    /// When the touchpad moved the cursor lately, oldest first. A tap's press is timed when its finger touched,
    /// but comes after the finger lifts (and after any motion it made itself), so motions after it are kept apart
    motions: VecDeque<u32>,
}

impl TapFilter {
    /// The touchpad moved the cursor
    pub fn moved(&mut self, time_msec: u32) {
        self.motions.push_back(time_msec);
        while self
            .motions
            .front()
            .is_some_and(|&old| time_msec.wrapping_sub(old) > MAX_WINDOW_MS)
        {
            self.motions.pop_front();
        }
    }

    /// Whether to pass on a button event. `tap`: the press came from a tap, not a physical button
    pub fn pass(
        &mut self,
        mode: Tap,
        window_ms: u32,
        tap: bool,
        button: u32,
        pressed: bool,
        time_msec: u32,
    ) -> bool {
        if !pressed {
            let Some(i) = self.held.iter().position(|&(held, _)| held == button) else {
                return true;
            };
            self.last_tap = Some(time_msec);
            return self.held.remove(i).1;
        }
        if !tap {
            return true;
        }
        let pass = match mode {
            Tap::None => false,
            Tap::Single => true,
            Tap::Smart => {
                // 0 or wrapped: at or after the press
                let just_before =
                    |&time: &u32| (1..=window_ms).contains(&time_msec.wrapping_sub(time));
                self.last_tap.as_ref().is_some_and(just_before)
                    || self.motions.iter().any(just_before)
            }
        };
        self.held.push((button, pass));
        pass
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEFT: u32 = 0x110;
    const RIGHT: u32 = 0x111;
    const WINDOW: u32 = 400;

    /// Feeds (tap, button, pressed, time) events, returning which were passed on
    fn run(mode: Tap, events: &[(bool, u32, bool, u32)]) -> Vec<bool> {
        let mut filter = TapFilter::default();
        events
            .iter()
            .map(|&(tap, button, pressed, time)| {
                filter.pass(mode, WINDOW, tap, button, pressed, time)
            })
            .collect()
    }

    #[test]
    fn modes_on_a_lone_tap() {
        let tap = [(true, LEFT, true, 0), (false, LEFT, false, 180)];
        assert_eq!(run(Tap::None, &tap), [false, false]);
        assert_eq!(run(Tap::Single, &tap), [true, true]);
        assert_eq!(run(Tap::Smart, &tap), [false, false]);
    }

    #[test]
    fn physical_clicks_always_pass() {
        let click = [(false, LEFT, true, 0), (false, LEFT, false, 50)];
        for mode in [Tap::None, Tap::Single, Tap::Smart] {
            assert_eq!(run(mode, &click), [true, true]);
        }
    }

    #[test]
    fn smart_clicks_on_the_second_tap() {
        let taps = [
            (true, LEFT, true, 0),
            (false, LEFT, false, 100),
            (true, LEFT, true, 200),
            (false, LEFT, false, 300),
        ];
        assert_eq!(run(Tap::Smart, &taps), [false, false, true, true]);
    }

    #[test]
    fn smart_then_drag_holds_the_second_press() {
        let taps = [
            (true, LEFT, true, 0),
            (false, LEFT, false, 100),
            (true, LEFT, true, 200),
            (false, LEFT, false, 2000),
        ];
        assert_eq!(run(Tap::Smart, &taps), [false, false, true, true]);
    }

    #[test]
    fn smart_three_taps_double_click() {
        let taps = [
            (true, LEFT, true, 0),
            (false, LEFT, false, 100),
            (true, LEFT, true, 200),
            (false, LEFT, false, 300),
            (true, LEFT, true, 400),
            (false, LEFT, false, 500),
        ];
        assert_eq!(
            run(Tap::Smart, &taps),
            [false, false, true, true, true, true]
        );
    }

    #[test]
    fn smart_right_clicks_after_any_tap() {
        let taps = [
            (true, LEFT, true, 0),
            (false, LEFT, false, 100),
            (true, RIGHT, true, 200),
            (false, RIGHT, false, 300),
        ];
        assert_eq!(run(Tap::Smart, &taps), [false, false, true, true]);
    }

    #[test]
    fn smart_slow_taps_dont_click() {
        let taps = [
            (true, LEFT, true, 0),
            (false, LEFT, false, 100),
            (true, LEFT, true, 100 + WINDOW + 1),
        ];
        assert_eq!(run(Tap::Smart, &taps), [false, false, false]);
    }

    /// Moves at `moves`, then a tap touching at 1000; whether its press was passed on
    fn tap_after_moves(moves: &[u32]) -> bool {
        let mut filter = TapFilter::default();
        for &time in moves {
            filter.moved(time);
        }
        filter.pass(Tap::Smart, WINDOW, true, LEFT, true, 1000)
    }

    #[test]
    fn smart_clicks_right_after_moving() {
        assert!(tap_after_moves(&[700, 800]));
    }

    #[test]
    fn smart_ignores_taps_long_after_moving() {
        assert!(!tap_after_moves(&[100, 1000 - WINDOW - 1]));
    }

    #[test]
    fn smart_ignores_the_taps_own_motion() {
        // jitter while the finger was down, which libinput reports before the tap's press
        assert!(!tap_after_moves(&[1000, 1050]));
    }

    #[test]
    fn smart_forgets_old_motions() {
        let mut filter = TapFilter::default();
        filter.moved(0);
        filter.moved(MAX_WINDOW_MS + 1);
        assert_eq!(filter.motions, [MAX_WINDOW_MS + 1]);
    }
}
