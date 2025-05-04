use crate::keyboard::SplitKeyboardOps;

pub trait FunctionKey {
    fn on_press(&self, keyb: &mut dyn SplitKeyboardOps);
    fn on_release(&self, keyb: &mut dyn SplitKeyboardOps);
}

pub struct KeyPshLayer<const NEW_LAYER: u8>;
struct KeyPshNextLayer;
struct KeyPshNextLayerTransient;
struct KeyPopLayer;

pub const KEY_PSH_NEXT_LAYER: &'static dyn FunctionKey = &KeyPshNextLayer;
pub const KEY_PSH_NEXT_LAYER_TRANSIENT: &'static dyn FunctionKey = &KeyPshNextLayerTransient;
pub const KEY_POP_LAYER: &'static dyn FunctionKey = &KeyPopLayer;

impl<const NEW_LAYER: u8> FunctionKey for KeyPshLayer<NEW_LAYER> {
    fn on_press(&self, keyb: &mut dyn SplitKeyboardOps) {
        keyb.layout_push_layer(NEW_LAYER);
    }

    fn on_release(&self, _keyb: &mut dyn SplitKeyboardOps) {}
}

impl FunctionKey for KeyPshNextLayer {
    fn on_press(&self, keyb: &mut dyn SplitKeyboardOps) {
        keyb.layout_push_next_layer();
    }

    fn on_release(&self, _keyb: &mut dyn SplitKeyboardOps) {}
}

impl FunctionKey for KeyPshNextLayerTransient {
    fn on_press(&self, keyb: &mut dyn SplitKeyboardOps) {
        keyb.layout_push_next_layer();
    }

    fn on_release(&self, keyb: &mut dyn SplitKeyboardOps) {
        keyb.layout_pop_layer();
    }
}

impl FunctionKey for KeyPopLayer {
    fn on_press(&self, keyb: &mut dyn SplitKeyboardOps) {
        keyb.layout_pop_layer();
    }

    fn on_release(&self, _keyb: &mut dyn SplitKeyboardOps) {}
}
