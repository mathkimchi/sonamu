//! Tapping the touchpad to click (#170): libinput detects taps; this decides which become clicks

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Tap {
    /// Taps don't click
    None,
    /// Each tap clicks
    Single,
    /// A tap only clicks right after another tap (within [`TWICE_WINDOW_MS`]), so a lone tap does nothing.
    /// Tap twice then drag drags, three taps double click
    Twice,
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
        _ => Tap::Twice,
    }
}

/// For [`Tap::Twice`], from a tap's release to the next tap's press
pub const TWICE_WINDOW_MS: u32 = 300;

/// Drops the button events of taps that shouldn't click
#[derive(Debug, Default)]
pub struct TapFilter {
    /// Taps' buttons held now, and whether their press was passed on
    held: Vec<(u32, bool)>,
    /// When the last tap was released
    last_tap: Option<u32>,
}

impl TapFilter {
    /// Whether to pass on a button event. `tap`: the press came from a tap, not a physical button
    pub fn pass(
        &mut self,
        mode: Tap,
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
            Tap::Twice => self
                .last_tap
                .is_some_and(|last| time_msec.wrapping_sub(last) <= TWICE_WINDOW_MS),
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

    /// Feeds (tap, button, pressed, time) events, returning which were passed on
    fn run(mode: Tap, events: &[(bool, u32, bool, u32)]) -> Vec<bool> {
        let mut filter = TapFilter::default();
        events
            .iter()
            .map(|&(tap, button, pressed, time)| filter.pass(mode, tap, button, pressed, time))
            .collect()
    }

    #[test]
    fn modes_on_a_lone_tap() {
        let tap = [(true, LEFT, true, 0), (false, LEFT, false, 180)];
        assert_eq!(run(Tap::None, &tap), [false, false]);
        assert_eq!(run(Tap::Single, &tap), [true, true]);
        assert_eq!(run(Tap::Twice, &tap), [false, false]);
    }

    #[test]
    fn physical_clicks_always_pass() {
        let click = [(false, LEFT, true, 0), (false, LEFT, false, 50)];
        for mode in [Tap::None, Tap::Single, Tap::Twice] {
            assert_eq!(run(mode, &click), [true, true]);
        }
    }

    #[test]
    fn twice_clicks_on_the_second_tap() {
        let taps = [
            (true, LEFT, true, 0),
            (false, LEFT, false, 100),
            (true, LEFT, true, 200),
            (false, LEFT, false, 300),
        ];
        assert_eq!(run(Tap::Twice, &taps), [false, false, true, true]);
    }

    #[test]
    fn twice_then_drag_holds_the_second_press() {
        let taps = [
            (true, LEFT, true, 0),
            (false, LEFT, false, 100),
            (true, LEFT, true, 200),
            (false, LEFT, false, 2000),
        ];
        assert_eq!(run(Tap::Twice, &taps), [false, false, true, true]);
    }

    #[test]
    fn twice_three_taps_double_click() {
        let taps = [
            (true, LEFT, true, 0),
            (false, LEFT, false, 100),
            (true, LEFT, true, 200),
            (false, LEFT, false, 300),
            (true, LEFT, true, 400),
            (false, LEFT, false, 500),
        ];
        assert_eq!(
            run(Tap::Twice, &taps),
            [false, false, true, true, true, true]
        );
    }

    #[test]
    fn twice_right_clicks_after_any_tap() {
        let taps = [
            (true, LEFT, true, 0),
            (false, LEFT, false, 100),
            (true, RIGHT, true, 200),
            (false, RIGHT, false, 300),
        ];
        assert_eq!(run(Tap::Twice, &taps), [false, false, true, true]);
    }

    #[test]
    fn twice_slow_taps_dont_click() {
        let taps = [
            (true, LEFT, true, 0),
            (false, LEFT, false, 100),
            (true, LEFT, true, 100 + TWICE_WINDOW_MS + 1),
        ];
        assert_eq!(run(Tap::Twice, &taps), [false, false, false]);
    }
}
