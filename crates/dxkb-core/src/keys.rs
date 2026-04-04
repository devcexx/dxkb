use dxkb_common::{KeyState, LogicalKeyState, dev_info, dev_warn};
use hut::Consumer;
use usbd_hid::descriptor::KeyboardUsage;

use crate::{
    hid::HidKeyboard,
    keyboard::{HandleKey, KeyboardStateLike, SplitKeyboardLike},
};

#[macro_export]
macro_rules! do_on_key_state_ignore_masked {
    ($old:ident, $new:ident, $on_pressed:tt, $on_released:tt) => {
        match ($old, $new) {
            (::dxkb_common::LogicalKeyState::PressedMasked, _) | (_, ::dxkb_common::LogicalKeyState::PressedMasked) => {}
            (_, ::dxkb_common::LogicalKeyState::Released) => {
                $on_released;
            }
            (_, ::dxkb_common::LogicalKeyState::Pressed) => {
                $on_pressed;
            }
        }
    };
}

pub fn standard_key_handle<S, Kb: SplitKeyboardLike<S>>(
    kb: &mut Kb,
    key: KeyboardUsage,
    old_key_state: LogicalKeyState,
    new_key_state: LogicalKeyState,
) {
    do_on_key_state_ignore_masked!(
        old_key_state, new_key_state,
        { kb.hid_mut().press_key(key) }, {
        kb.hid_mut().release_key(key)
    });
}

pub fn consumer_control_key_handle<S, Kb: SplitKeyboardLike<S>>(
    kb: &mut Kb,
    key: Consumer,
    old_key_state: LogicalKeyState,
    new_key_state: LogicalKeyState,
) {
    do_on_key_state_ignore_masked!(
        old_key_state,
        new_key_state,
        { kb.hid_mut().press_consumer_control_key(key) },
        { kb.hid_mut().release_consumer_control_key(key) }
    );
}

pub fn function_key_handle<S: KeyboardStateLike, Kb: SplitKeyboardLike<S>>(
    kb: &mut Kb,
    key: &BuiltinFunctionKey,
    old_key_state: LogicalKeyState,
    new_key_state: LogicalKeyState,
) {
    match key {
        BuiltinFunctionKey::PushNextLayer => {
            do_on_key_state_ignore_masked!(
                old_key_state,
                new_key_state,
                {
                    let _ = kb.state_mut().push_next_layer();
                },
                {}
            );
        }
        BuiltinFunctionKey::PushLayer(new) => {
            do_on_key_state_ignore_masked!(
                old_key_state,
                new_key_state,
                {
                    let _ = kb.state_mut().push_layer_raw(*new);
                },
                {}
            );
        }
        BuiltinFunctionKey::PopLayer => {
            do_on_key_state_ignore_masked!(
                old_key_state,
                new_key_state,
                {
                    let _ = kb.state_mut().pop_layer_raw();
                },
                {}
            );
        }
        BuiltinFunctionKey::PushNextLayerTransient => {
            do_on_key_state_ignore_masked!(
                old_key_state,
                new_key_state,
                {
                    let _ = kb.state_mut().push_next_layer();
                },
                {
                    let _ = kb.state_mut().pop_layer_raw();
                }
            );
        }
        BuiltinFunctionKey::PushLayerTransient(new) => {
            do_on_key_state_ignore_masked!(
                old_key_state,
                new_key_state,
                {
                    let _ = kb.state_mut().push_layer_raw(*new);
                },
                {
                    let _ = kb.state_mut().pop_layer_raw();
                }
            );
        }
        BuiltinFunctionKey::SetLayer(new) => {
            do_on_key_state_ignore_masked!(
                old_key_state,
                new_key_state,
                {
                    let _ = kb.state_mut().request_layer_raw(*new);
                },
                {

                }
            );
        }
        BuiltinFunctionKey::SetRelativeLayer(offset) => {
            do_on_key_state_ignore_masked!(
                old_key_state,
                new_key_state,
                {
                    let state = kb.state_mut();
                    let current = state.requested_layer_raw();
                    let _ = state.request_layer_raw(current.saturating_add_signed(*offset));
                },
                {

                }
            );
        }
        BuiltinFunctionKey::SetRelativeLayerTransient(offset) => {
            do_on_key_state_ignore_masked!(
                old_key_state,
                new_key_state,
                {
                    let state = kb.state_mut();
                    let current = state.requested_layer_raw();
                    let _ = state.request_layer_raw(current.saturating_add_signed(*offset));
                },
                {
                    let state = kb.state_mut();
                    let current = state.requested_layer_raw();
                    let _ = state.request_layer_raw(current.saturating_sub_signed(*offset));
                }
            );
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum BuiltinFunctionKey {
    /// Pushes the current layer onto the layer stack and requests the next
    /// layer (current + 1) to be active. When released, does nothing.
    PushNextLayer,

    /// Pushes the current layer onto the layer stack and requests the given
    /// layer to become the active one. When released, does nothing.
    PushLayer(u8),

    /// Pops the most recent layer out of the stack and requests such layer to
    /// become active. When released, does nothing.
    PopLayer,

    /// Pushes the current layer onto the layer stack and requests the next
    /// layer (current + 1) to be active. When released, pops the layer back, so
    /// the previous one becomes active again.
    PushNextLayerTransient,

    /// Pushes the current layer onto the layer stack and requests the given
    /// layer to become the active one. When released, pops the layer back, so
    /// the previous one becomes active again.
    PushLayerTransient(u8),

    /// Requests the given layer to become the active one, without modifying the
    /// layer stack. When released, does nothing.
    SetLayer(u8),

    /// Requests the current layer plus the given offset to become the active
    /// one, without modifying the layer stack. When released, does nothing.
    SetRelativeLayer(i8),

    /// Requests the current layer plus the given offset to become the active
    /// one, without modifying the layer stack. When released, does the opposite
    /// and requests the current layer minus the given offset to become active
    /// again.
    SetRelativeLayerTransient(i8),
}

// TODO after the inclusion of the consumer control keys, the size of this enum
// has increased to 4 bytes, which is probably too much. The definition of the
// keys for a standard 104 keys keyboard will reach 416 bytes. It doesn't seem
// possible to make the rust enum optimization work for such amount of nested
// enums, without changing the layout of the underlying enums, so alternatives
// should be considered (e. g collapsing all the nested enums into the same one
// and manually using the not used variants, etc).
#[derive(Clone, PartialEq, Eq)]
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
        old_state: LogicalKeyState,
        new_state: LogicalKeyState,
    ) {
        match self {
            DefaultKey::NoOp => {}
            DefaultKey::Standard(keyboard_usage) => {
                standard_key_handle(kb, *keyboard_usage, old_state, new_state);
            }
            DefaultKey::Function(builtin_function_key) => {
                function_key_handle(kb, builtin_function_key, old_state, new_state);
            }
            DefaultKey::ConsumerControl(key) => {
                consumer_control_key_handle(kb, *key, old_state, new_state);
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
    (LPshNxt) => {
        $crate::keys::BuiltinFunctionKey::PushNextLayer
    };
    (LPsh($layer:literal)) => {
        $crate::keys::BuiltinFunctionKey::PushLayer($layer)
    };
    (LPop) => {
        $crate::keys::BuiltinFunctionKey::PopLayer
    };
    (LTPshNxt) => {
        $crate::keys::BuiltinFunctionKey::PushNextLayerTransient
    };
    (LTPsh($layer:literal)) => {
        $crate::keys::BuiltinFunctionKey::PushLayerTransient(
            $layer,
        )
    };
    (LSet($layer:literal)) => {
        $crate::keys::BuiltinFunctionKey::SetLayer(
            $layer,
        )
    };
    (LRelSet(+$layer:literal)) => {
        $crate::keys::BuiltinFunctionKey::SetRelativeLayer(
            $layer,
        )
    };
    (LRelSet(-$layer:literal)) => {
        $crate::keys::BuiltinFunctionKey::SetRelativeLayer(
            -$layer,
        )
    };
    (LTRelSet(+$layer:literal)) => {
        $crate::keys::BuiltinFunctionKey::SetRelativeLayerTransient(
            $layer,
        )
    };
    (LTRelSet(-$layer:literal)) => {
        $crate::keys::BuiltinFunctionKey::SetRelativeLayerTransient(
            -$layer,
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
