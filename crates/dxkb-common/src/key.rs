
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
}

impl Default for KeyState {
    fn default() -> Self {
        KeyState::Released
    }
}
