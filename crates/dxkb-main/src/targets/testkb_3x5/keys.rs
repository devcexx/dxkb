use dxkb_common::{LogicalKeyState, dev_info};
use dxkb_core::{do_on_key_state_ignore_masked, hid::HidKeyboard, keyboard::{HandleKey, KeyboardUsage}, keys::DefaultKey};

pub struct CustomKeyContext {
    pub plus_pending_press: bool,
}

impl CustomKeyContext {
    pub const fn new() -> CustomKeyContext {
        Self {
            plus_pending_press: false,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum CustomKey {
    Default(DefaultKey),
    /// When pressed, presses both the LShift and the = key, so the plus symbol can be sent without any other keystroke.
    Plus
}

impl HandleKey for CustomKey {
    type User = CustomKeyContext;

    fn handle_key_state_change<S: dxkb_core::keyboard::KeyboardStateLike, Kb: dxkb_core::keyboard::SplitKeyboardLike<S>>(
        &self,
        kb: &mut Kb,
        user: &mut Self::User,
        old_state: LogicalKeyState,
        new_state: LogicalKeyState,
    ) {
        match self {
            CustomKey::Default(default_key) => default_key.handle_key_state_change(kb, &mut (), old_state, new_state),
            CustomKey::Plus => {
                let hid = kb.hid_mut();
                do_on_key_state_ignore_masked!(old_state, new_state,
                    {
                        hid.press_key(KeyboardUsage::KeyboardLeftShift);
                        user.plus_pending_press = true;
                    },
                    {
                        hid.release_key(KeyboardUsage::KeyboardEqualPlus);
                        hid.release_key(KeyboardUsage::KeyboardLeftShift);
                    }
                );
            },
        }

    }
}

#[macro_export]
macro_rules! custom_key_from_alias {
    (u:Plus) => {
        CustomKey::Plus
    };

    ($($other:tt)*) => {
        CustomKey::Default(dxkb_core::default_key_from_alias!($($other)*))
    }
}
