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
