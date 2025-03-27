use crate::keyboard::SplitKeyboardOps;

pub trait FunctionKey {
    fn on_press(&self, keyb: &mut dyn SplitKeyboardOps);
    fn on_release(&self, keyb: &mut dyn SplitKeyboardOps);
}

pub struct PshLayer<const NEW_LAYER: u8>;

impl<const NEW_LAYER: u8> FunctionKey for PshLayer<NEW_LAYER> {
    fn on_press(&self, keyb: &mut dyn SplitKeyboardOps) {
        keyb.layout_push_layer(NEW_LAYER);
    }

    fn on_release(&self, _keyb: &mut dyn SplitKeyboardOps) {

    }
}

pub struct PshNextLayer;

impl FunctionKey for PshNextLayer {
    fn on_press(&self, keyb: &mut dyn SplitKeyboardOps) {
        keyb.layout_push_next_layer();
    }

    fn on_release(&self, _keyb: &mut dyn SplitKeyboardOps) {

    }
}
