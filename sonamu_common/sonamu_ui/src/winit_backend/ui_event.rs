use smithay::reexports::winit;

use crate::display_units::DisplayContainerSize;

/// TODO: not great that I am reexporting smithay's event, given that the goal is to be backend agnostic.
/// I am doing it right now because I'd rather get something working sooner, even if I have to compromise a bit
///
/// TODO: also, figure out a way to easily match keypresses and shortcuts
///
/// TODO: figure out a standard way of "forwarding" events to child
#[derive(Debug, Clone, Copy)]
pub enum UIEvent {
    Key {
        symbol: Option<KeySymbol>,
        modifiers: KeyModifiers,
        raw_keycode: u32,
        /// TODO: also have a repeat
        pressed: bool,
    },
    WindowResized(DisplayContainerSize),
    Mouse(MouseEvent),
    /// A step of a touchpad swipe, sent each time the fingers move far enough, like scrolling
    Swipe {
        fingers: u32,
        direction: SwipeDirection,
        /// Not the swipe's first step
        repeat: bool,
    },
}
/// Which way the fingers moved
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwipeDirection {
    Up,
    Down,
    Left,
    Right,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouseEvent {
    /// Cursor position in physical pixels, relative to the window
    pub position: [f64; 2],
    pub kind: MouseEventKind,
    /// When the device produced it, in CLOCK_MONOTONIC ms (eg from libinput).
    /// Clients use it for scroll velocity, so it matters when events get handled in batches.
    pub time_msec: Option<u32>,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MouseEventKind {
    Motion,
    /// `button` is an evdev code (e.g. `BTN_LEFT` = 0x110)
    Button {
        button: u32,
        pressed: bool,
    },
    /// Positive scrolls down/right (Wayland convention).
    /// `v120` is set for discrete (wheel) scrolling, in 1/120ths of a notch.
    /// `stop` is set per axis when the fingers lift (touchpad), so clients can end the gesture / start kinetic scrolling.
    Scroll {
        delta: [f64; 2],
        v120: Option<[i32; 2]>,
        stop: [bool; 2],
    },
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub caps_lock: bool,
    pub logo: bool,
    // pub num_lock: bool,
}
#[derive(Debug, Clone, Copy)]
pub enum KeySymbol {
    ArrowKeyUp,
    ArrowKeyDown,
    ArrowKeyLeft,
    ArrowKeyRight,
    Enter,
    Backspace,
    PageDown,
    PageUp,
    Escape,
    Char(char),
}

/// TODO: Get rid of keytrait and just implement directly?
pub trait KeyTrait {
    fn to_alphabet(&self) -> Option<char>;
    fn to_digit(&self) -> Option<u8>;
    fn to_char(&self) -> Option<char>;
}
impl KeyTrait for KeySymbol {
    fn to_alphabet(&self) -> Option<char> {
        let c = self.to_char()?;
        if c.is_ascii() { Some(c) } else { None }
    }

    fn to_digit(&self) -> Option<u8> {
        let c = self.to_char()?;
        c.to_digit(10).map(|c| c as u8)
    }

    fn to_char(&self) -> Option<char> {
        // if self.raw_code == 28 {
        //     // FIXME: I added this bc I thought ENTER had no char, but it is actually `\r` already.
        //     return Some('\n');
        // }
        // self.logical_key.to_text().and_then(|s| s.chars().nth(0))
        match self {
            Self::Enter => Some('\n'),
            Self::Backspace => Some('\u{8}'),
            Self::Char(c) => Some(*c),
            _ => None,
        }
    }
}

impl KeyModifiers {
    pub const NONE: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
        caps_lock: false,
        logo: false,
        // num_lock: false,
    };

    pub const CTRL: Self = Self {
        ctrl: true,
        alt: false,
        shift: false,
        caps_lock: false,
        logo: false,
        // num_lock: false,
    };

    pub const ALT: Self = Self {
        ctrl: false,
        alt: true,
        shift: false,
        caps_lock: false,
        logo: false,
        // num_lock: false,
    };

    pub const SHIFT: Self = Self {
        ctrl: false,
        alt: false,
        shift: true,
        caps_lock: false,
        logo: false,
        // num_lock: false,
    };

    pub const LOGO: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
        caps_lock: false,
        logo: true,
        // num_lock: false,
    };

    pub const CTRL_SHIFT: Self = Self::both(Self::CTRL, Self::SHIFT);

    /// For example, combine(CTRL, SHIFT) is CTRL_SHIFT
    #[must_use]
    pub const fn both(self, rhs: Self) -> Self {
        Self {
            ctrl: self.ctrl | rhs.ctrl,
            alt: self.alt | rhs.alt,
            shift: self.shift | rhs.shift,
            caps_lock: self.caps_lock | rhs.caps_lock,
            logo: self.logo | rhs.logo,
            // num_lock: self.num_lock | rhs.num_lock,
        }
    }
}
impl From<winit::event::Modifiers> for KeyModifiers {
    fn from(value: winit::event::Modifiers) -> Self {
        Self {
            ctrl: value.state().control_key(),
            alt: value.state().alt_key(),
            shift: value.state().shift_key(),
            // TODO
            caps_lock: false,
            logo: value.state().meta_key(),
            // num_lock,
        }
    }
}
impl std::ops::BitOr for KeyModifiers {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self::both(self, rhs)
    }
}
impl std::ops::BitAnd for KeyModifiers {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self {
            ctrl: self.ctrl & rhs.ctrl,
            alt: self.alt & rhs.alt,
            shift: self.shift & rhs.shift,
            caps_lock: self.caps_lock & rhs.caps_lock,
            logo: self.logo & rhs.logo,
            // num_lock: self.num_lock & rhs.num_lock,
        }
    }
}

impl TryFrom<winit::event::KeyEvent> for KeySymbol {
    type Error = ();

    fn try_from(value: winit::event::KeyEvent) -> Result<Self, Self::Error> {
        match value.physical_key {
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowLeft) => {
                Ok(Self::ArrowKeyLeft)
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowRight) => {
                Ok(Self::ArrowKeyRight)
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowDown) => {
                Ok(Self::ArrowKeyDown)
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowUp) => {
                Ok(Self::ArrowKeyUp)
            }

            winit::keyboard::PhysicalKey::Code(
                winit::keyboard::KeyCode::Enter | winit::keyboard::KeyCode::NumpadEnter,
                // NOTE: my laptop says enter is numpad enter for some reason.
            ) => Ok(Self::Enter),

            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::PageDown) => {
                Ok(Self::PageDown)
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::PageUp) => {
                Ok(Self::PageUp)
            }

            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Backspace) => {
                Ok(Self::Backspace)
            }

            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape) => {
                Ok(Self::Escape)
            }

            _ => value
                .logical_key
                .to_text()
                .and_then(|s| s.chars().next())
                .map(KeySymbol::Char)
                .ok_or(()),
        }
    }
}
