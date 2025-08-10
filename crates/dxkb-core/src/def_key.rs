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
    Function(BuiltinFunctionKey), // TODO Add Function keys
                                  // TODO Add keys for System Control.
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
