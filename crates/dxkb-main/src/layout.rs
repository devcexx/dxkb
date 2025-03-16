enum KeyboardButton {
    StandardKey(usbd_hid::descriptor::KeyboardUsage),
}

pub struct SplitKeyboardLayout<const ROWS: usize, const COLS: usize> {



    // While the key matrix should map to a physical peripheral, the
    // layout is a greater abstraction that should take into
    // consideration already the status of each key (so the status
    // should be stored here), regardless on which part of the split
    // keyboard we are. Of course also, the definition of each key should also appear here.
    layout: [KeyboardButton; 64]
}
