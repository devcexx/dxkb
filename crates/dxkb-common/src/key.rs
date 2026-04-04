
/**
 * Represents the possibles physical states of a key in the keyboard, this is: Pressed or Released.
 */
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum KeyState {
    Released = 0,
    Pressed = 1,
}

impl KeyState {
    pub const fn from_bool(value: bool) -> KeyState {
        match value {
            true => KeyState::Pressed,
            false => KeyState::Released,
        }
    }

    pub const fn to_bool(self) -> bool {
        match self {
            KeyState::Released => false,
            KeyState::Pressed => true,
        }
    }

    pub const fn as_logical(self) -> LogicalKeyState {
        match self {
            KeyState::Released => LogicalKeyState::Released,
            KeyState::Pressed => LogicalKeyState::Pressed,
        }
    }
}

impl Default for KeyState {
    fn default() -> Self {
        KeyState::Released
    }
}

/**
 * Represents the possible logical states of a key internally in the keyboard matrix.
 */
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LogicalKeyState {
    /// Indicates that the key is physically unpressed. Equivalent to [`KeyState::Released`].
    Released = 0,

    /// Indicates that the key is physically pressed. Equivalent to [`KeyState::Pressed`].
    Pressed = 1,

    /// Indicates that the key is physically pressed, but its effects want to be
    /// ignored. When a key enters into this state, it shouldn't run the press
    /// actions. When a key exits this state, it shouldn't run the release
    /// actions.
    PressedMasked = 2
}

impl LogicalKeyState {
    pub fn is_physically_pressed(&self) -> bool {
        match self {
            LogicalKeyState::Released => false,
            LogicalKeyState::Pressed | LogicalKeyState::PressedMasked => true,
        }
    }

    pub const fn from_u8(val: u8) -> LogicalKeyState {
        match val {
            1 => LogicalKeyState::Pressed,
            2 => LogicalKeyState::PressedMasked,
            _ => LogicalKeyState::Released,
        }
    }
}
