use core::{any::Any, marker::PhantomData, mem::MaybeUninit};

use dxkb_common::{
    KeyState, dev_debug, dev_error, dev_info, dev_trace, dev_warn,
    util::{BitMatrix, BitMatrixLayout, BoundedIndex, ColBitMatrixLayout},
};
use dxkb_peripheral::key_matrix::{KeyMatrix, KeyMatrixLike};
use dxkb_split_link::SplitBusLike;
use heapless::Vec;
use ringbuffer::{ConstGenericRingBuffer, RingBuffer};
use serde::{Deserialize, Serialize};
use stm32f4xx_hal::{gpio::{Input, Pin, PinMode, PinPull, Pull}, hal::digital::InputPin};
use usb_device::{
    LangID,
    bus::{UsbBus, UsbBusAllocator},
    device::{StringDescriptors, UsbDevice, UsbDeviceBuilder, UsbVidPid},
};
use usbd_hid::{
    UsbError,
    descriptor::{KeyboardReport, KeyboardUsage, SerializedDescriptor},
    hid_class::{
        HIDClass, HidClassSettings, HidCountryCode, HidProtocol, HidSubClass, ProtocolModeConfig,
    },
};

pub trait MasterCheck {
    fn is_current_master(&mut self) -> bool;
}

pub struct AlwaysMaster;
pub struct AlwaysSlave;

impl MasterCheck for AlwaysMaster {
    fn is_current_master(&mut self) -> bool {
        return true;
    }
}

impl MasterCheck for AlwaysSlave {
    fn is_current_master(&mut self) -> bool {
        return false;
    }
}

pub struct PinMasterSense<P: InputPin> {
    pin: P
}

impl<P: InputPin + PinPull> PinMasterSense<P> {
    pub fn new(mut pin: P) -> Self {
        pin.set_internal_resistor(Pull::Down);

        Self {
            pin
        }
    }
}

impl<P: InputPin> MasterCheck for PinMasterSense<P> {
    fn is_current_master(&mut self) -> bool {
        // TODO I don't expect this input to have any capacitance, so maybe it is subject to noise or something. Should I read it multiple times?
        self.pin.is_high().unwrap_or(false)
    }
}


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
    Right,
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
    type Opposite = Left;
    const SIDE: SplitKeyboardSide = SplitKeyboardSide::Right;
}

pub trait SplitKeyboardOps {
    fn layout_push_layer(&mut self, new_layer: u8);
    fn layout_pop_layer(&mut self) -> Option<u8>;
    fn layout_push_next_layer(&mut self) -> Option<u8>;
    fn layout_push_prev_layer(&mut self) -> Option<u8>;
}

pub struct SplitKeyboard<
    'usb,
    const LLAYERS: u8,
    const LROWS: u8,
    const LCOLS: u8,
    const MROWS: u8,
    const MCOLS: u8,
    Side: SplitKeyboardSideType,
    USB: UsbBus,
    LayoutConfig: SplitLayoutConfig,
    Matrix: KeyMatrixLike<MROWS, MCOLS>,
    MasterTester: MasterCheck,
    SplitBus: SplitBusLike<SplitKeyboardLinkMessage>,
> where
    ColBitMatrixLayout<LCOLS>: BitMatrixLayout,
    [(); LLAYERS as usize]:,
    [(); LCOLS as usize]:,
    [(); LROWS as usize]:,
{
    usb_device: UsbDevice<'usb, USB>,
    kbd_hid: HIDClass<'usb, USB>,
    matrix: Matrix,
    layout: SplitKeyboardLayout<LayoutConfig, LLAYERS, LROWS, LCOLS>,
    pub split_bus: SplitBus,
    master_tester: MasterTester,
    is_master: bool,

    /// Holds the in keyboard report pending to be sent, that have
    /// previously failed to be sent because the usb device was busy.
    pending_in_keyb_report: Option<KeyboardReport>,
    _side: PhantomData<Side>,
    _layout_config: PhantomData<LayoutConfig>,
}

impl<
    'usb,
    const LLAYERS: u8,
    const LROWS: u8,
    const LCOLS: u8,
    const MROWS: u8,
    const MCOLS: u8,
    CurSide,
    USB,
    LayoutConfig,
    Matrix,
    MasterTester,
    SplitBus,
>
    SplitKeyboard<
        'usb,
        LLAYERS,
        LROWS,
        LCOLS,
        MROWS,
        MCOLS,
        CurSide,
        USB,
        LayoutConfig,
        Matrix,
        MasterTester,
        SplitBus,
    >
where
    CurSide: SideLayoutOffset<LayoutConfig>,
    CurSide::Opposite: SideLayoutOffset<LayoutConfig>,
    USB: UsbBus,
    LayoutConfig: SplitLayoutConfig,
    ColBitMatrixLayout<LCOLS>: BitMatrixLayout,
    Matrix: KeyMatrixLike<MROWS, MCOLS>,
MasterTester: MasterCheck,
    SplitBus: SplitBusLike<SplitKeyboardLinkMessage>,
    [(); LLAYERS as usize]:,
    [(); LCOLS as usize]:,
    [(); LROWS as usize]:,
{
    const fn assert_config_ok() {
        assert!(
            LROWS >= MROWS,
            "Layout rows cannot be smaller than the number of rows in the current side matrix"
        );
        assert!(
            LCOLS >= MCOLS,
            "Layout cols cannot be smaller than the number of cols in the current side matrix"
        );
    }

    pub fn new(
        usb_allocator: &'usb UsbBusAllocator<USB>,
        layout: SplitKeyboardLayout<LayoutConfig, LLAYERS, LROWS, LCOLS>,
        matrix: Matrix,
        split_bus: SplitBus,
        master_tester: MasterTester
    ) -> Self {
        const { Self::assert_config_ok() }

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
            master_tester,
            is_master: false,
            pending_in_keyb_report: None,
            _side: PhantomData,
            _layout_config: PhantomData,
        }
    }

    fn split_link_transfer_msg(split_bus: &mut SplitBus, msg: SplitKeyboardLinkMessage) {
        if let Err(e) = split_bus.transfer(msg) {
            dev_warn!("Couldn't transfer message through split link: {:?}", e);
        }
    }

    // TODO NKRO
    fn update_usb_key_pressed_report(
        usb_keys_pressed_report: &mut Vec<u8, 6>,
        new_key: KeyboardUsage,
    ) {
        if usb_keys_pressed_report.is_full() {
            usb_keys_pressed_report.clear();
            let _ = usb_keys_pressed_report.push(KeyboardUsage::KeyboardErrorRollOver as u8);
        } else if usb_keys_pressed_report.first()
            == Some(&(KeyboardUsage::KeyboardErrorRollOver as u8))
        {
            // Too many pressed keys already, ignore.
        } else {
            let _ = usb_keys_pressed_report.push(new_key as u8);
        }
    }

    fn layout_update_key_state<Side: SideLayoutOffset<LayoutConfig>>(
        &mut self,
        usb_keys_pressed_report: &mut Vec<u8, 6>,
        row: u8,
        col: u8,
        current_state: KeyState,
    ) -> bool {
        let (real_row, real_col) = self.layout.get_real_key_coordinate::<Side>(row, col);
        let changed = self
            .layout
            .set_real_key_state(real_row, real_col, current_state);
        if changed {
            match self.layout.get_key_definition(real_row, real_col) {
                LayoutKey::Standard(key) => {
                    if current_state == KeyState::Pressed {
                        Self::update_usb_key_pressed_report(usb_keys_pressed_report, key);
                    }
                }
                LayoutKey::Function(function_key) => match current_state {
                    KeyState::Released => {
                        function_key.on_release(self);
                    }
                    KeyState::Pressed => {
                        function_key.on_press(self);
                    }
                },
            }
        }

        changed
    }

    #[inline(always)]
    fn handle_keyb_tx_result(&mut self, result: Result<usize, UsbError>) {
        match result {
            Ok(_) => {
                let _ = self.pending_in_keyb_report.take();
            }
            Err(UsbError::WouldBlock) => {
                // Do nothing, leave the report there until we
                // are able to send it, or if new changes
                // superseeds the last stored report.
            }
            Err(e) => {
                dev_error!("USB xfer error failed: {:?}", e);
            }
        }
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
                    keys_changed |= self.layout_update_key_state::<CurSide>(
                        &mut usb_keys_pressed_report,
                        row,
                        col,
                        self.matrix.get_key_state(row, col),
                    );
                }
            }
        }

        let mut incoming_split_msgs = Vec::<SplitKeyboardLinkMessage, 16>::new();
        self.split_bus.poll_into_vec(&mut incoming_split_msgs);
        for msg in incoming_split_msgs {
            match msg {
                SplitKeyboardLinkMessage::MatrixKeyDown { row, col } => {
                    keys_changed |= self.layout_update_key_state::<CurSide::Opposite>(
                        &mut usb_keys_pressed_report,
                        row,
                        col,
                        KeyState::Pressed,
                    );
                }
                SplitKeyboardLinkMessage::MatrixKeyUp { row, col } => {
                    keys_changed |= self.layout_update_key_state::<CurSide::Opposite>(
                        &mut usb_keys_pressed_report,
                        row,
                        col,
                        KeyState::Released,
                    );
                }
            }
        }

        if self.usb_device.poll(&mut [&mut self.kbd_hid]) {
            // TODO do something with this
            let mut reportbuf = [0u8; size_of::<KeyboardReport>()];
            if let Ok(report) = self.kbd_hid.pull_raw_report(&mut reportbuf) {
                dev_debug!("Report received: {:?}", report);
            }
        }

        if keys_changed {
            let report = self
                .pending_in_keyb_report
                .insert(KeyboardReport::default());
            report.keycodes[0..usb_keys_pressed_report.len()]
                .copy_from_slice(&usb_keys_pressed_report);

            // TODO This method writes to the USB FIFO once we have
            // data ready to be sent, but not when the host requests
            // it. This means that, when the host requests it, there
            // might be some outdated information in the Tx FIFO that
            // will be sent. Eventually could be interesting to flush
            // the TX FIFO before sending, by writing to the TXFFLSH
            // register.
            let res = self.kbd_hid.push_input(report);
            self.handle_keyb_tx_result(res);
        } else if let Some(report) = &self.pending_in_keyb_report {
            let res = self.kbd_hid.push_input(report);
            self.handle_keyb_tx_result(res);
        }
    }

    fn poll_slave(&mut self) {
        self.matrix.scan_matrix_act(|row, col, state| match state {
            KeyState::Released => {
                Self::split_link_transfer_msg(
                    &mut self.split_bus,
                    SplitKeyboardLinkMessage::MatrixKeyUp { row, col },
                );
            }
            KeyState::Pressed => {
                Self::split_link_transfer_msg(
                    &mut self.split_bus,
                    SplitKeyboardLinkMessage::MatrixKeyDown { row, col },
                );
            }
        });

        self.split_bus.poll(|msg| {
            match msg {
                SplitKeyboardLinkMessage::MatrixKeyDown { row: _, col: _ } => {
                    dev_warn!("Unexpected MatrixKeyDown message received while in slave mode");
                }
                SplitKeyboardLinkMessage::MatrixKeyUp { row: _, col: _ } => {
                    dev_warn!("Unexpected MatrixKeyUp message received while in slave mode");
                }
            }
            true
        });
    }

    fn check_master(&mut self) {
        let res = self.master_tester.is_current_master();
        if res != self.is_master {
            self.is_master = res;
            if res {
                dev_info!("Controller has been promoted to master");
            } else {
                dev_info!("Controller has been downgraded to slave");
            }
        }
    }

    pub fn poll(&mut self) {
        self.check_master();

        if self.is_master {
            self.poll_master();
        } else {
            self.poll_slave();
        }
    }
}

impl<
    'usb,
    const LLAYERS: u8,
    const LROWS: u8,
    const LCOLS: u8,
    const MROWS: u8,
    const MCOLS: u8,
    CurSide,
    USB,
    LayoutConfig,
    Matrix,
    MasterTester,
    SplitBus,
> SplitKeyboardOps
    for SplitKeyboard<
        'usb,
        LLAYERS,
        LROWS,
        LCOLS,
        MROWS,
        MCOLS,
        CurSide,
        USB,
        LayoutConfig,
        Matrix,
        MasterTester,
        SplitBus,
    >
where
    CurSide: SideLayoutOffset<LayoutConfig>,
    CurSide::Opposite: SideLayoutOffset<LayoutConfig>,
    USB: UsbBus,
    LayoutConfig: SplitLayoutConfig,
    ColBitMatrixLayout<LCOLS>: BitMatrixLayout,
    Matrix: KeyMatrixLike<MROWS, MCOLS>,
MasterTester: MasterCheck,
    SplitBus: SplitBusLike<SplitKeyboardLinkMessage>,
    [(); LLAYERS as usize]:,
    [(); LCOLS as usize]:,
    [(); LROWS as usize]:,
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
    Standard(KeyboardUsage),

    // TODO This makes "hard" to define custom function keys, because
    // it is hard to pass a custom context to it. Is there a better way of doing this?
    Function(&'static dyn FunctionKey),

    // TODO Add keys for System Control.
}

impl From<KeyboardUsage> for LayoutKey {
    fn from(value: KeyboardUsage) -> Self {
        LayoutKey::Standard(value)
    }
}

impl From<&'static dyn FunctionKey> for LayoutKey {
    fn from(value: &'static dyn FunctionKey) -> Self {
        LayoutKey::Function(value)
    }
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

#[repr(transparent)]
pub struct LayerRow<const COLS: u8>
where
    [(); COLS as usize]:,
{
    row: [LayoutKey; COLS as usize],
}

impl<const COLS: u8> LayerRow<COLS>
where
    [(); COLS as usize]:,
{
    pub const fn new(row: [LayoutKey; COLS as usize]) -> Self {
        Self { row }
    }

    pub fn new_from(row: [impl Into<LayoutKey>; COLS as usize]) -> Self {
        Self {
            row: row.map(|e| e.into()),
        }
    }
}

pub struct LayoutLayer<const ROWS: u8, const COLS: u8>
where
    [(); COLS as usize]:,
    [(); ROWS as usize]:,
{
    keys: [LayerRow<COLS>; ROWS as usize],
}

impl<const ROWS: u8, const COLS: u8> LayoutLayer<ROWS, COLS>
where
    [(); COLS as usize]:,
    [(); ROWS as usize]:,
{
    pub const fn new(keys: [LayerRow<COLS>; ROWS as usize]) -> Self {
        Self { keys }
    }
}

impl<const ROWS: u8, const COLS: u8> LayoutLayer<ROWS, COLS>
where
    [(); COLS as usize]:,
    [(); ROWS as usize]:,
{
    fn get_key_definition(&self, row: u8, col: u8) -> LayoutKey {
        self.keys[row as usize].row[col as usize]
    }
}

pub struct SplitKeyboardLayout<
    C: SplitLayoutConfig,
    const LAYERS: u8,
    const ROWS: u8,
    const COLS: u8,
> where
    ColBitMatrixLayout<COLS>: BitMatrixLayout,
    [(); LAYERS as usize]:,
    [(); COLS as usize]:,
    [(); ROWS as usize]:,
{
    _config: PhantomData<C>,
    layers: [LayoutLayer<ROWS, COLS>; LAYERS as usize],
    // Since ringbuffer doesn't implement any kind of "pop" operation
    // for dropping the most recently added element, for now I'm using
    // a Vec as alternative. If the stack gets out of memory, it will
    // smash the tip of the stack to make room for the new element.
    layers_stack: Vec<u8, 8>,
    key_states: BitMatrix<{ ROWS as usize }, COLS>,
    current_layer: BoundedIndex<{ LAYERS as usize }>,
    pressed_keys_count: u16,
}

impl<C: SplitLayoutConfig, const LAYERS: u8, const ROWS: u8, const COLS: u8>
    SplitKeyboardLayout<C, LAYERS, ROWS, COLS>
where
    ColBitMatrixLayout<COLS>: BitMatrixLayout,
    [(); LAYERS as usize]:,
    [(); COLS as usize]:,
    [(); ROWS as usize]:,
{
    const fn assert_config_ok() {
        assert!(
            C::SPLIT_RIGHT_COL_OFFSET < COLS,
            "Invalid layout config: Split column offset must be less than the number of columns"
        );
        assert!(LAYERS > 0, "There must be at least 1 layer in the layout!");
    }

    pub const fn new(layers: [LayoutLayer<ROWS, COLS>; LAYERS as usize]) -> Self {
        const { Self::assert_config_ok() };

        Self {
            _config: PhantomData,
            layers,
            layers_stack: Vec::new(),
            key_states: BitMatrix::new(),
            current_layer: BoundedIndex::from_const::<0>(),
            pressed_keys_count: 0,
        }
    }

    // TODO Better differentiation between real key coordinates (which
    // the (0,0) is at the top, left, of the left side) and side
    // dependent coords, which the (0, 0) is at the top, left of the
    // current side. (Maybe at type level?)
    #[inline(always)]
    fn get_real_key_coordinate<Side>(&self, row: u8, col: u8) -> (u8, u8)
    where
        Side: SideLayoutOffset<C>,
    {
        (row, col + Side::SIDE_COL_OFFSET)
    }

    #[inline(always)]
    fn set_real_key_state(&mut self, row: u8, col: u8, state: KeyState) -> bool {
        let changed = self
            .key_states
            .set_value(row as usize, col, state.to_bool());

        if changed {
            match state {
                KeyState::Released => self.pressed_keys_count -= 1,
                KeyState::Pressed => self.pressed_keys_count += 1,
            }
        }

        changed
    }

    #[inline(always)]
    fn set_key_state<Side>(&mut self, row: u8, col: u8, state: KeyState) -> bool
    where
        Side: SideLayoutOffset<C>,
    {
        let (real_row, real_col) = self.get_real_key_coordinate::<Side>(row, col);
        self.set_real_key_state(real_row, real_col, state)
    }

    #[inline(always)]
    fn get_key_definition(&self, row: u8, col: u8) -> LayoutKey {
        self.layers[self.current_layer].get_key_definition(row, col)
    }

    fn get_number_keys_pressed(&self) -> u16 {
        self.pressed_keys_count
    }

    fn push_layer(&mut self, new_layer: u8) -> bool {
        let len = self.layers_stack.len();
        let Some(layer_index) = BoundedIndex::from_value(new_layer as usize) else {
            dev_warn!("Requested new layer out of bounds: {}", new_layer);
            return false; // Out of bounds
        };

        if let Err(_) = self.layers_stack.push(new_layer) {
            self.layers_stack[len - 1] = new_layer;
        }
        self.current_layer = layer_index;
        true
    }

    fn pop_layer(&mut self) -> Option<u8> {
        if let Some(head) = self.layers_stack.pop() {
            dev_trace!("Popped layer: {}", head);
            self.current_layer = BoundedIndex::from_value(head as usize).unwrap();
            Some(head)
        } else {
            dev_warn!("No layers to pop were available");
            None
        }
    }
}
