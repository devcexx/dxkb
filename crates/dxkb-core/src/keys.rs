use dxkb_common::{KeyState, dev_info, dev_warn};
use hut::Consumer;
use usbd_hid::descriptor::KeyboardUsage;

use crate::{
    hid::HidKeyboard,
    keyboard::{HandleKey, KeyboardStateLike, SplitKeyboardLike},
};

#[macro_export]
macro_rules! do_on_key_state {
    ($st:ident, $on_pressed:tt, $on_released:tt) => {
        match $st {
            ::dxkb_common::KeyState::Released => {
                $on_released;
            }
            ::dxkb_common::KeyState::Pressed => {
                $on_pressed;
            }
        }
    };
}

pub fn standard_key_handle<S, Kb: SplitKeyboardLike<S>>(
    kb: &mut Kb,
    key: KeyboardUsage,
    key_state: KeyState,
) {
    do_on_key_state!(key_state, { kb.hid_mut().press_key(key) }, {
        kb.hid_mut().release_key(key)
    });
}

pub fn consumer_control_key_handle<S, Kb: SplitKeyboardLike<S>>(
    kb: &mut Kb,
    key: Consumer,
    key_state: KeyState,
) {
    do_on_key_state!(
        key_state,
        { kb.hid_mut().press_consumer_control_key(key) },
        { kb.hid_mut().release_consumer_control_key(key) }
    );
}

pub fn function_key_handle<S: KeyboardStateLike, Kb: SplitKeyboardLike<S>>(
    kb: &mut Kb,
    key: &BuiltinFunctionKey,
    key_state: KeyState,
) {
    match key {
        BuiltinFunctionKey::PushNextLayer => {
            do_on_key_state!(
                key_state,
                {
                    let _ = kb.state_mut().push_next_layer();
                },
                {}
            );
        }
        BuiltinFunctionKey::PushLayer(new) => {
            do_on_key_state!(
                key_state,
                {
                    let _ = kb.state_mut().push_layer_raw(*new);
                },
                {}
            );
        }
        BuiltinFunctionKey::PopLayer => {
            do_on_key_state!(
                key_state,
                {
                    let _ = kb.state_mut().pop_layer_raw();
                },
                {}
            );
        }
        BuiltinFunctionKey::PushNextLayerTransient => {
            do_on_key_state!(
                key_state,
                {
                    let _ = kb.state_mut().push_next_layer();
                },
                {
                    let _ = kb.state_mut().pop_layer_raw();
                }
            );
        }
        BuiltinFunctionKey::PushLayerTransient(new) => {
            do_on_key_state!(
                key_state,
                {
                    let _ = kb.state_mut().push_layer_raw(*new);
                },
                {
                    let _ = kb.state_mut().pop_layer_raw();
                }
            );
        }
    }
}

#[derive(Clone)]
pub enum BuiltinFunctionKey {
    PushNextLayer,
    PushLayer(u8),
    PopLayer,
    PushNextLayerTransient,
    PushLayerTransient(u8),
}

// TODO after the inclusion of the consumer control keys, the size of this enum
// has increased to 4 bytes, which is probably too much. The definition of the
// keys for a standard 104 keys keyboard will reach 416 bytes. It doesn't seem
// possible to make the rust enum optimization work for such amount of nested
// enums, without changing the layout of the underlying enums, so alternatives
// should be considered (e. g collapsing all the nested enums into the same one
// and manually using the not used variants, etc).
#[derive(Clone)]
pub enum DefaultKey {
    NoOp,
    Standard(KeyboardUsage),
    Function(BuiltinFunctionKey),
    ConsumerControl(Consumer),
}

impl From<KeyboardUsage> for DefaultKey {
    fn from(value: KeyboardUsage) -> Self {
        DefaultKey::Standard(value)
    }
}

impl HandleKey for DefaultKey {
    type User = ();

    fn handle_key_state_change<S: KeyboardStateLike, Kb: SplitKeyboardLike<S>>(
        &self,
        kb: &mut Kb,
        _user: &mut Self::User,
        key_state: KeyState,
    ) {
        match self {
            DefaultKey::NoOp => {}
            DefaultKey::Standard(keyboard_usage) => {
                standard_key_handle(kb, *keyboard_usage, key_state);
            }
            DefaultKey::Function(builtin_function_key) => {
                function_key_handle(kb, builtin_function_key, key_state);
            }
            DefaultKey::ConsumerControl(key) => {
                consumer_control_key_handle(kb, *key, key_state);
            }
        }
    }
}

#[macro_export]
macro_rules! hid_key_from_alias {
    // Aliases to letters
    (A) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardAa };
    (B) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardBb };
    (C) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardCc };
    (D) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardDd };
    (E) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardEe };
    (F) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardFf };
    (G) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardGg };
    (H) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardHh };
    (I) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardIi };
    (J) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardJj };
    (K) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardKk };
    (L) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardLl };
    (M) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardMm };
    (N) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardNn };
    (O) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardOo };
    (P) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardPp };
    (Q) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardQq };
    (R) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardRr };
    (S) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardSs };
    (T) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardTt };
    (U) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardUu };
    (V) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardVv };
    (W) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardWw };
    (X) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardXx };
    (Y) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardYy };
    (Z) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardZz };

    ('A') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardAa };
    ('B') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardBb };
    ('C') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardCc };
    ('D') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardDd };
    ('E') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardEe };
    ('F') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardFf };
    ('G') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardGg };
    ('H') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardHh };
    ('I') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardIi };
    ('J') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardJj };
    ('K') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardKk };
    ('L') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardLl };
    ('M') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardMm };
    ('N') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardNn };
    ('O') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardOo };
    ('P') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardPp };
    ('Q') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardQq };
    ('R') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardRr };
    ('S') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardSs };
    ('T') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardTt };
    ('U') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardUu };
    ('V') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardVv };
    ('W') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardWw };
    ('X') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardXx };
    ('Y') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardYy };
    ('Z') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardZz };

    // Aliases to numbers
    (0) => { ::usbd_hid::descriptor::KeyboardUsage::Keyboard0CloseParens };
    (1) => { ::usbd_hid::descriptor::KeyboardUsage::Keyboard1Exclamation };
    (2) => { ::usbd_hid::descriptor::KeyboardUsage::Keyboard2At };
    (3) => { ::usbd_hid::descriptor::KeyboardUsage::Keyboard3Hash };
    (4) => { ::usbd_hid::descriptor::KeyboardUsage::Keyboard4Dollar };
    (5) => { ::usbd_hid::descriptor::KeyboardUsage::Keyboard5Percent };
    (6) => { ::usbd_hid::descriptor::KeyboardUsage::Keyboard6Caret };
    (7) => { ::usbd_hid::descriptor::KeyboardUsage::Keyboard7Ampersand };
    (8) => { ::usbd_hid::descriptor::KeyboardUsage::Keyboard8Asterisk };
    (9) => { ::usbd_hid::descriptor::KeyboardUsage::Keyboard9OpenParens };

    ('0') => { ::usbd_hid::descriptor::KeyboardUsage::Keyboard0CloseParens };
    ('1') => { ::usbd_hid::descriptor::KeyboardUsage::Keyboard1Exclamation };
    ('2') => { ::usbd_hid::descriptor::KeyboardUsage::Keyboard2At };
    ('3') => { ::usbd_hid::descriptor::KeyboardUsage::Keyboard3Hash };
    ('4') => { ::usbd_hid::descriptor::KeyboardUsage::Keyboard4Dollar };
    ('5') => { ::usbd_hid::descriptor::KeyboardUsage::Keyboard5Percent };
    ('6') => { ::usbd_hid::descriptor::KeyboardUsage::Keyboard6Caret };
    ('7') => { ::usbd_hid::descriptor::KeyboardUsage::Keyboard7Ampersand };
    ('8') => { ::usbd_hid::descriptor::KeyboardUsage::Keyboard8Asterisk };
    ('9') => { ::usbd_hid::descriptor::KeyboardUsage::Keyboard9OpenParens };

    // Other aliases
    (Esc) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardEscape };
    (LCtl) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardLeftControl };
    (RCtl) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardRightControl };
    (LSft) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardLeftShift };
    (RSft) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardRightShift };
    (LAlt) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardLeftAlt };
    (RAlt) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardRightAlt };
    (LWin) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardLeftGUI };
    (RWin) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardRightGUI };
    (LGui) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardLeftGUI };
    (RGui) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardRightGUI };
    (Bksp) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardBackspace };
    (Home) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardHome };
    (End) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardEnd };
    (PrScr) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardPrintScreen };
    (Up) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardUpArrow };
    (Down) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardDownArrow };
    (Left) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardLeftArrow };
    (Right) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardRightArrow };
    (Insrt) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardInsert };
    (Del) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardDelete };
    (Caps) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardCapsLock };
    (PgUp) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardPageUp };
    (PgDn) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardPageDown };

    ('`') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardBacktickTilde };
    ('\\') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardBackslashBar };
    (',') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardCommaLess };
    ('.') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardPeriodGreater };
    ('/') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardSlashQuestion };
    ('-') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardDashUnderscore };
    ('=') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardEqualPlus };
    ('[') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardOpenBracketBrace };
    (']') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardCloseBracketBrace };
    ("'") => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardSingleDoubleQuote };
    ('\'') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardSingleDoubleQuote };
    (' ') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardSpacebar };
    (Spc) => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardSpacebar };
    (';') => { ::usbd_hid::descriptor::KeyboardUsage::KeyboardSemiColon };

    // Function (F1, F2, ...) keys don't need any further alias, they can use
    // the fallback ones.

    // Use this fallback branch for matching any other KeyboardUsage variant
    // (without including the Keyboard prefix).
    ($id:ident) => {
        ::usbd_hid::descriptor::KeyboardUsage::${concat("Keyboard", $id)}
     };

    // Use this fallback branch for matching any other KeyboardUsage variant,
    // when it happens that starts with a number (e.g for referencing
    // Keyboard3Hash you can do hid_key_from_alias!("3Hash")).
    ($id:literal) => {
        ::usbd_hid::descriptor::KeyboardUsage::${concat("Keyboard", $id)}
    };
}

#[macro_export]
macro_rules! function_key_from_alias {
    (PshNxtLyr) => {
        $crate::keys::BuiltinFunctionKey::PushNextLayer
    };
    (PshLyr($layer:literal)) => {
        $crate::keys::BuiltinFunctionKey::PushLayer($layer)
    };
    (PopLyr) => {
        $crate::keys::BuiltinFunctionKey::PopLayer
    };
    (PshNxtLyrT) => {
        $crate::keys::BuiltinFunctionKey::PushNextLayerTransient
    };
    (PshLyrT($layer:literal)) => {
        $crate::keys::BuiltinFunctionKey::PushLayerTransient(
            $layer,
        )
    };
}

#[macro_export]
macro_rules! consumer_control_usage_from_alias {
    // Some basic aliases for common keys
    (VolUp) => {
        ::hut::Consumer::VolumeIncrement
    };
    (VolDown) => {
        ::hut::Consumer::VolumeDecrement
    };
    (VUp) => {
        ::hut::Consumer::VolumeIncrement
    };
    (VDn) => {
        ::hut::Consumer::VolumeDecrement
    };
    (Pwr) => {
        ::hut::Consumer::Power
    };
    (Rst) => {
        ::hut::Consumer::Restart
    };
    (Sleep) => {
        ::hut::Consumer::Sleep
    };
    (Slp) => {
        ::hut::Consumer::Sleep
    };
    (BrightUp) => {
        ::hut::Consumer::DisplayBrightnessIncrement
    };
    (BrightDown) => {
        ::hut::Consumer::DisplayBrightnessDecrement
    };
    (PlayPause) => {
        ::hut::Consumer::PlayPause
    };
    (Ply) => {
        ::hut::Consumer::PlayPause
    };
    (Next) => {
        ::hut::Consumer::ScanNextTrack
    };
    (Prev) => {
        ::hut::Consumer::ScanPreviousTrack
    };
    (Nxt) => {
        ::hut::Consumer::ScanNextTrack
    };
    (Prv) => {
        ::hut::Consumer::ScanPreviousTrack
    };

    // Just delegate on the names defined by the HID spec.
    ($id:ident) => {
        ::hut::Consumer::$id
    };
}

#[macro_export]
macro_rules! default_key_from_alias {
    (_) => {
        $crate::keys::DefaultKey::NoOp
    };

    (f:$($f:tt)*) => {
        $crate::keys::DefaultKey::Function($crate::function_key_from_alias!($($f)*))
    };

    (c:$($cc:tt)*) => {
        $crate::keys::DefaultKey::ConsumerControl($crate::consumer_control_usage_from_alias!($($cc)*))
    };

    ($($other:tt)*) => {
        $crate::keys::DefaultKey::Standard($crate::hid_key_from_alias!($($other)*))
    };
}
