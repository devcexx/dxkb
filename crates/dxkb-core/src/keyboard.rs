use core::marker::PhantomData;

use dxkb_peripheral::key_matrix::{KeyMatrix, KeyMatrixLike};
use dxkb_split_link::SplitBusLike;
use heapless::Vec;
use ringbuffer::{ConstGenericRingBuffer, RingBuffer};
use serde::{Deserialize, Serialize};
use usb_device::{bus::{UsbBus, UsbBusAllocator}, device::{StringDescriptors, UsbDevice, UsbDeviceBuilder, UsbVidPid}, LangID};
use usbd_hid::{descriptor::{KeyboardReport, KeyboardUsage, SerializedDescriptor}, hid_class::{HIDClass, HidClassSettings, HidCountryCode, HidProtocol, HidSubClass, ProtocolModeConfig}};
use dxkb_common::{dev_info, dev_warn, util::{BitMatrix, BitMatrixLayout, BoundedIndex, ColBitMatrixLayout}, KeyState};

use crate::keys::FunctionKey;

// TODO Eventually I might want to implement something more (e.g
// something like a keyboard whose SplitBusLink Msg is of type
// Impl<KeyboardSplitLinkMessage>) so the caller can define its own
// more custom protocol.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SplitKeyboardLinkMessage {
    MatrixKeyDown { row: u8, col: u8 },
    MatrixKeyUp { row: u8, col: u8 },
}

/// Represents the possible sides of a split keyboard as enum variants
#[derive(Clone, Copy, Debug)]
pub enum SplitKeyboardSide {
    Left,
    Right
}

impl SplitKeyboardSide {
    pub const fn opposite(self) -> Self {
        match self {
            SplitKeyboardSide::Left => SplitKeyboardSide::Right,
            SplitKeyboardSide::Right => SplitKeyboardSide::Left,
        }
    }
}

/// Represents the side of a split keyboard, to be used as a type.
pub trait SplitKeyboardSideType {
    type Opposite: SplitKeyboardSideType;

    const SIDE: SplitKeyboardSide;
    const OPPOSITE: SplitKeyboardSide = Self::Opposite::SIDE;
}

/// The left side of a split keyboard.
pub struct Left;

/// The right side of a split keyboard.
pub struct Right;

impl SplitKeyboardSideType for Left {
    type Opposite = Right;
    const SIDE: SplitKeyboardSide = SplitKeyboardSide::Left;
}

impl SplitKeyboardSideType for Right {
    type Opposite = Right;
    const SIDE: SplitKeyboardSide = SplitKeyboardSide::Right;
}

pub trait SplitKeyboardOps {
    fn layout_push_layer(&mut self, new_layer: u8);
    fn layout_pop_layer(&mut self) -> Option<u8>;
    fn layout_push_next_layer(&mut self) -> Option<u8>;
    fn layout_push_prev_layer(&mut self) -> Option<u8>;
}

pub struct SplitKeyboard<'usb,
    const LLAYERS: u8,
    const LROWS: u8,
    const LCOLS: u8,
    const MROWS: u8,
    const MCOLS: u8,
    Side: SplitKeyboardSideType,
    USB: UsbBus,
    LayoutConfig: SplitLayoutConfig,
    Matrix: KeyMatrixLike<MROWS, MCOLS>,
    SplitBus: SplitBusLike<SplitKeyboardLinkMessage>
    > where
ColBitMatrixLayout<LCOLS>: BitMatrixLayout,
[(); LLAYERS as usize]:,
[(); LCOLS as usize]:,
[(); LROWS as usize]: {

        usb_device: UsbDevice<'usb, USB>,
        kbd_hid: HIDClass<'usb, USB>,
        matrix: Matrix,
        layout: SplitKeyboardLayout<LayoutConfig, LLAYERS, LROWS, LCOLS>,
        split_bus: SplitBus,
        master: bool,
        _side: PhantomData<Side>,
        _layout_config: PhantomData<LayoutConfig>
}

impl<'usb,
    const LLAYERS: u8,
    const LROWS: u8,
    const LCOLS: u8,
    const MROWS: u8,
    const MCOLS: u8,
    CurSide,
    USB,
    LayoutConfig,
    Matrix,
    SplitBus
    > SplitKeyboard<'usb, LLAYERS, LROWS, LCOLS, MROWS, MCOLS, CurSide, USB, LayoutConfig, Matrix, SplitBus>
where CurSide: SideLayoutOffset<LayoutConfig>,
CurSide::Opposite: SideLayoutOffset<LayoutConfig>,
USB: UsbBus,
LayoutConfig: SplitLayoutConfig,
ColBitMatrixLayout<LCOLS>: BitMatrixLayout,
Matrix: KeyMatrixLike<MROWS, MCOLS>,
SplitBus: SplitBusLike<SplitKeyboardLinkMessage>,
[(); LLAYERS as usize]:,
[(); LCOLS as usize]:,
[(); LROWS as usize]:
 {
     const fn assert_config_ok() {
         assert!(LROWS >= MROWS, "Layout rows cannot be smaller than the number of rows in the current side matrix");
         assert!(LCOLS >= MCOLS, "Layout cols cannot be smaller than the number of cols in the current side matrix");
     }

     pub fn new(usb_allocator: &'usb UsbBusAllocator<USB>, layout: SplitKeyboardLayout<LayoutConfig, LLAYERS, LROWS, LCOLS>, matrix: Matrix, split_bus: SplitBus, master: bool) -> Self {
         const {
             Self::assert_config_ok()
         }

         // TODO Unhardcode settings and strings


         let kbd_hid = HIDClass::new_ep_in_with_settings(
             &usb_allocator,
             &KeyboardReport::desc(),
             1,
             HidClassSettings {
                 subclass: HidSubClass::NoSubClass,
                 protocol: HidProtocol::Keyboard,
                 config: ProtocolModeConfig::DefaultBehavior,
                 locale: HidCountryCode::Spanish,
             },
         );


        let usb_dev = UsbDeviceBuilder::new(usb_allocator, UsbVidPid(0x16c0, 0x27db))
            .device_class(0x3) // HID Device
            .device_sub_class(HidSubClass::NoSubClass as u8) // No subclass
            .device_protocol(HidProtocol::Generic as u8)
            .usb_rev(usb_device::device::UsbRev::Usb200)

            .strings(&[StringDescriptors::new(LangID::ES)
                .serial_number("0")
                .manufacturer("Dobetito")
                .product("DXKB Lily58L")])
            .unwrap()
            .supports_remote_wakeup(true)
            .build();

        Self {
            usb_device: usb_dev,
            kbd_hid,
            matrix,
            layout,
            split_bus,
            master,
            _side: PhantomData,
            _layout_config: PhantomData
        }
     }

     fn split_link_transfer_msg(split_bus: &mut SplitBus, msg: SplitKeyboardLinkMessage) {
         if let Err(e) = split_bus.transfer(msg) {
             dev_warn!("Couldn't transfer message through split link: {:?}", e);
         }
     }

     // TODO NKRO
     fn update_usb_key_pressed_report(usb_keys_pressed_report: &mut Vec<u8, 6>, new_key: KeyboardUsage) {
         if usb_keys_pressed_report.is_full() {
             usb_keys_pressed_report.clear();
             let _ = usb_keys_pressed_report.push(KeyboardUsage::KeyboardErrorRollOver as u8);
         } else if usb_keys_pressed_report.first() == Some(&(KeyboardUsage::KeyboardErrorRollOver as u8)) {
             // Too many pressed keys already, ignore.
         } else {
             usb_keys_pressed_report.push(new_key as u8);
         }
     }

     fn layout_update_key_state<Side: SideLayoutOffset<LayoutConfig>>(&mut self, usb_keys_pressed_report: &mut Vec<u8, 6>, row: u8, col: u8, current_state: KeyState) -> bool {
         let (real_row, real_col) = self.layout.get_real_key_coordinate::<Side>(row, col);
         let changed = self.layout.set_real_key_state(real_row, real_col, current_state);
         if changed {
             match self.layout.get_key_definition(real_row, real_col) {
                 LayoutKey::Standard(key) => {
                     if current_state == KeyState::Pressed {
                         Self::update_usb_key_pressed_report(usb_keys_pressed_report, key);
                     }
                 },
                 LayoutKey::Function(function_key) => {
                     match current_state {
                         KeyState::Released => {
                             function_key.on_release(self);
                         }
                         KeyState::Pressed => {
                             function_key.on_press(self);
                         },
                    }

                 },
            }
         }

         changed
     }

     fn poll_master(&mut self) {
         let matrix_changed = self.matrix.scan_matrix();
         let mut keys_changed = false;
         let mut usb_keys_pressed_report = Vec::<u8, 6>::new();
         if matrix_changed {
             // TODO There has to be a better way to implement
             // this. Maybe eventually I can just copy the bitmatrix
             // to the layout bit matrix, making sure we only override
             // the bits from the current side of the keyboard. For
             // now following a naive implementation.
             for row in 0..MROWS {
                 for col in 0..MCOLS {

                     keys_changed |= self.layout_update_key_state::<CurSide>(&mut usb_keys_pressed_report, row, col, self.matrix.get_key_state(row, col));
                 }
             }
         }


         // TODO This doesn't compile

         // TODO I probably need to define an interface in the split
         // bus that allows to pick only the next N messages, so the
         // caller can have a buffer where store some data before
         // processing it. In this case, I'm not able to use mut self
         // inside the closure because it has been partially borrowed
         // as mut because of self.split_bus. That could help to fix
         // that.

         // self.split_bus.poll(|msg| {
         //     match msg {
         //         SplitKeyboardLinkMessage::MatrixKeyDown { row, col } => {
         //             keys_changed |= self.layout_update_key_state::<CurSide::Opposite>(&mut usb_keys_pressed_report, *row, *col, KeyState::Pressed);
         //         },
         //         SplitKeyboardLinkMessage::MatrixKeyUp { row, col } => {
         //             keys_changed |= self.layout_update_key_state::<CurSide::Opposite>(&mut usb_keys_pressed_report, *row, *col, KeyState::Released);
         //         },
         //     }
         // });

         let mut pressed_keys_idx: [u8; 6] = [0u8; 6];


         if self.usb_device.poll(&mut [&mut self.kbd_hid]) {


             // TODO Is there any way to only write the data we need
             // to send when the host requests for it? so we can
             // always provide it the most up-to-date information.

             // TODO Maybe is a good idea to generate this report
             // on-the-fly instead of iterate over the whole matrix
             // again to check which ones are pressed?
             let mut report = KeyboardReport::default();

             // TODO NKRO support
             if self.layout.get_number_keys_pressed() > 6 {
                 report.keycodes[0] = KeyboardUsage::KeyboardErrorRollOver as u8;
             } else {
                 // let mut found: usize = 0;
                 // 'out: for col in 0..4 {
                 //     for row in 0..4 {
                 //         if found >= self.layout.get_number_keys_pressed() as usize {
                 //             break 'out;
                 //         }

                 //         if self.matrix.get_key_state(row, col) == KeyState::Pressed {
                 //             report.keycodes[found] = self.layout.get_key_definition(row, col);
                 //             next_index += 1;
                 //         }
                 //     }
                 // }
             }



         }

     }

     fn poll_slave(&mut self) {
         self.matrix.scan_matrix_act(|row, col, state| {
             match state {
                 KeyState::Released => {
                     Self::split_link_transfer_msg(&mut self.split_bus, SplitKeyboardLinkMessage::MatrixKeyUp { row, col });
                 },
                 KeyState::Pressed => {
                     Self::split_link_transfer_msg(&mut self.split_bus, SplitKeyboardLinkMessage::MatrixKeyDown { row, col });
                 },
            }
         });

         self.split_bus.poll(|msg| {
             match msg {
                 SplitKeyboardLinkMessage::MatrixKeyDown { row: _, col:  _ } => {
                     dev_warn!("Unexpected MatrixKeyDown message received while in slave mode");
                 },
                 SplitKeyboardLinkMessage::MatrixKeyUp { row: _, col:  _ } => {
                     dev_warn!("Unexpected MatrixKeyDown message received while in slave mode");
                 },
            }
         });
     }
 }

impl<'usb,
    const LLAYERS: u8,
    const LROWS: u8,
    const LCOLS: u8,
    const MROWS: u8,
    const MCOLS: u8,
    CurSide,
    USB,
    LayoutConfig,
    Matrix,
    SplitBus
    > SplitKeyboardOps for SplitKeyboard<'usb, LLAYERS, LROWS, LCOLS, MROWS, MCOLS, CurSide, USB, LayoutConfig, Matrix, SplitBus>
where CurSide: SideLayoutOffset<LayoutConfig>,
CurSide::Opposite: SideLayoutOffset<LayoutConfig>,
USB: UsbBus,
LayoutConfig: SplitLayoutConfig,
ColBitMatrixLayout<LCOLS>: BitMatrixLayout,
Matrix: KeyMatrixLike<MROWS, MCOLS>,
SplitBus: SplitBusLike<SplitKeyboardLinkMessage>,
[(); LLAYERS as usize]:,
[(); LCOLS as usize]:,
[(); LROWS as usize]:

 {
    fn layout_push_layer(&mut self, new_layer: u8) {
        self.layout.push_layer(new_layer);
    }

     fn layout_pop_layer(&mut self) -> Option<u8> {
         self.layout.pop_layer()
    }

     fn layout_push_next_layer(&mut self) -> Option<u8> {
         if self.layout.current_layer.index() >= LLAYERS as usize {
             return None;
         }

         let new_layer = self.layout.current_layer.index() as u8 + 1;
         self.layout.push_layer(new_layer);
         Some(new_layer)
    }

    fn layout_push_prev_layer(&mut self) -> Option<u8> {
        if self.layout.current_layer.index() == 0 {
            return None;
        }

        let new_layer = self.layout.current_layer.index() as u8 - 1;
        self.layout.push_layer(new_layer);
        Some(new_layer)
    }
}

#[derive(Clone, Copy)]
pub enum LayoutKey {
    Standard(usbd_hid::descriptor::KeyboardUsage),

    // TODO This makes "hard" to define custom function keys, because
    // it is hard to pass a custom context to it. Is there a better way of doing this?
    Function(&'static dyn FunctionKey),
}

pub trait SplitLayoutConfig {
    /// The offset from which matrix of the right side of the keyboard
    /// starts. For example, if the key (col=0, row=3) in the right
    /// side of the keyboard is pressed, that would translate to
    /// (col=[`SPLIT_RIGHT_COL_OFFSET`] + 0, 3) in the layout.
    const SPLIT_RIGHT_COL_OFFSET: u8;
}

pub trait SideLayoutOffset<Config: SplitLayoutConfig>: SplitKeyboardSideType {
    const SIDE_COL_OFFSET: u8;
}

impl<Config: SplitLayoutConfig> SideLayoutOffset<Config> for Left {
    const SIDE_COL_OFFSET: u8 = 0;
}

impl<Config: SplitLayoutConfig> SideLayoutOffset<Config> for Right {
    const SIDE_COL_OFFSET: u8 = Config::SPLIT_RIGHT_COL_OFFSET;
}

pub struct LayoutLayer<const ROWS: u8, const COLS: u8> where [(); COLS as usize]:, [(); ROWS as usize]: {
    keys: [[LayoutKey; COLS as usize]; ROWS as usize]
}

impl<const ROWS: u8, const COLS: u8> LayoutLayer<ROWS, COLS> where [(); COLS as usize]:, [(); ROWS as usize]: {
    fn get_key_definition(&self, row: u8, col: u8) -> LayoutKey {
        self.keys[row as usize][col as usize]
    }
}

pub struct SplitKeyboardLayout<C: SplitLayoutConfig, const LAYERS: u8, const ROWS: u8, const COLS: u8> where ColBitMatrixLayout<COLS>: BitMatrixLayout, [(); LAYERS as usize]:, [(); COLS as usize]:, [(); ROWS as usize]: {
    _config: PhantomData<C>,
    layers: [LayoutLayer<ROWS, COLS>; LAYERS as usize],
    // Since ringbuffer doesn't implement any kind of "pop" operation
    // for dropping the most recently added element, for now I'm using
    // a Vec as alternative. If the stack gets out of memory, it will
    // smash the tip of the stack to make room for the new element.
    layers_stack: Vec<u8, 8>,
    key_states: BitMatrix<{ROWS as usize}, COLS>,
    current_layer: BoundedIndex<{LAYERS as usize}>,
    pressed_keys_count: u16
}


impl<C: SplitLayoutConfig, const LAYERS: u8, const ROWS: u8, const COLS: u8> SplitKeyboardLayout<C, LAYERS, ROWS, COLS>  where ColBitMatrixLayout<COLS>: BitMatrixLayout, [(); LAYERS as usize]:, [(); COLS as usize]:, [(); ROWS as usize]: {
    const fn assert_config_ok() {
        assert!(C::SPLIT_RIGHT_COL_OFFSET < COLS, "Invalid layout config: Split column offset must be less than the number of columns");
        assert!(LAYERS > 0, "There must be at least 1 layer in the layout!");
    }

    pub fn new(layers: [LayoutLayer<ROWS, COLS>; LAYERS as usize]) -> Self {
        const {
            Self::assert_config_ok()
        };

        Self {
            _config: PhantomData,
            layers,
            layers_stack: Vec::new(),
            key_states: BitMatrix::new(),
            current_layer: BoundedIndex::from_const::<0>(),
            pressed_keys_count: 0
        }
    }

    // TODO Better differentiation between real key coordinates (which
    // the (0,0) is at the top, left, of the left side) and side
    // dependent coords, which the (0, 0) is at the top, left of the
    // current side. (Maybe at type level?)
    #[inline(always)]
    fn get_real_key_coordinate<Side>(&self, row: u8, col: u8)  -> (u8, u8) where Side: SideLayoutOffset<C> {
        (row, col + Side::SIDE_COL_OFFSET)
    }



    #[inline(always)]
    fn set_real_key_state(&mut self, row: u8, col: u8, state: KeyState) -> bool {
        let changed = self.key_states.set_value(row as usize, col, state.to_bool());

        if changed {
            match state {
                KeyState::Released => self.pressed_keys_count -= 1,
                KeyState::Pressed => self.pressed_keys_count += 1,
            }
        }

        changed
    }


    #[inline(always)]
    fn set_key_state<Side>(&mut self, row: u8, col: u8, state: KeyState) -> bool where Side: SideLayoutOffset<C> {
        let (real_row, real_col) = self.get_real_key_coordinate::<Side>(row, col);
        self.set_real_key_state(real_row, real_col, state)
    }

    #[inline(always)]
    fn get_key_definition(&self, row: u8, col: u8) -> LayoutKey {
        self.layers[self.current_layer].keys[row as usize][col as usize]
    }

    fn get_number_keys_pressed(&self) -> u16 {
        self.pressed_keys_count
    }

    fn push_layer(&mut self, new_layer: u8) {
        let len = self.layers_stack.len();
        if let Err(_) = self.layers_stack.push(new_layer) {
            self.layers_stack[len - 1] = new_layer;
        }
        self.current_layer = BoundedIndex::from_value(new_layer as usize).unwrap();
    }

    fn pop_layer(&mut self) -> Option<u8> {
        if let Some(head) = self.layers_stack.pop() {
            self.current_layer = BoundedIndex::from_value(head as usize).unwrap();
            Some(head)
        } else {
            None
        }
    }

}
