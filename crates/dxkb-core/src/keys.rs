use dxkb_common::{KeyState, dev_info, dev_warn};
use usbd_hid::descriptor::KeyboardUsage;

use crate::keyboard::{HandleKey, KeyboardStateLike, SplitKeyboardLike};

macro_rules! do_on_state {
    ($st:ident, $on_pressed:tt, $on_released:tt) => {
        match $st {
            KeyState::Released => {
                $on_released;
            }
            KeyState::Pressed => {
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
    do_on_state!(key_state, { kb.hid_report_mut().add_key(key) }, {
        kb.hid_report_mut().rm_key(key)
    });
}

pub fn function_key_handle<S: KeyboardStateLike, Kb: SplitKeyboardLike<S>>(
    kb: &mut Kb,
    key: &BuiltinFunctionKey,
    key_state: KeyState,
) {
    match key {
        BuiltinFunctionKey::PushNextLayer => {
            do_on_state!(
                key_state,
                {
                    if !kb.state_mut().push_next_layer() {
                        dev_info!("No more upper layers available");
                    }
                },
                {}
            );
        }
        BuiltinFunctionKey::PushLayer(new) => {
            do_on_state!(
                key_state,
                {
                    if !kb.state_mut().push_layer_raw(*new) {
                        dev_warn!("No layer at index {new} available");
                    }
                },
                {}
            );
        }
        BuiltinFunctionKey::PopLayer => {
            do_on_state!(
                key_state,
                {
                    if kb.state_mut().pop_layer_raw().is_some() {
                        dev_warn!("No layers to pop");
                    }
                },
                {}
            );
        }
        BuiltinFunctionKey::PushNextLayerTransient => {
            do_on_state!(
                key_state,
                {
                    if !kb.state_mut().push_next_layer() {
                        dev_info!("No more upper layers available");
                    }
                },
                {
                    kb.state_mut().pop_layer_raw();
                }
            );
        }
        BuiltinFunctionKey::PushLayerTransient(new) => {
            do_on_state!(
                key_state,
                {
                    if !kb.state_mut().push_layer_raw(*new) {
                        dev_warn!("No layer at index {new} available");
                    }
                },
                {
                    kb.state_mut().pop_layer_raw();
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

#[derive(Clone)]
pub enum DefaultKey {
    NoOp,
    Standard(KeyboardUsage),
    Function(BuiltinFunctionKey),
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
macro_rules! default_key_from_alias {
    (_) => {
        $crate::keys::DefaultKey::NoOp
    };

    (f:PshNxtLyr) => {
        $crate::keys::DefaultKey::Function($crate::keys::BuiltinFunctionKey::PushNextLayer)
    };
    (f:PshLyr($layer:literal)) => {
        $crate::keys::DefaultKey::Function($crate::keys::BuiltinFunctionKey::PushLayer($layer))
    };
    (f:PopLyr) => {
        $crate::keys::DefaultKey::Function($crate::keys::BuiltinFunctionKey::PopLayer)
    };
    (f:PshNxtLyrT) => {
        $crate::keys::DefaultKey::Function($crate::keys::BuiltinFunctionKey::PushNextLayerTransient)
    };
    (f:PshLyrT($layer:literal)) => {
        $crate::keys::DefaultKey::Function($crate::keys::BuiltinFunctionKey::PushLayerTransient($layer))
    };
    ($other:tt) => {
        $crate::keys::DefaultKey::Standard($crate::hid_key_from_alias!($other))
    };
}
