use core::marker::PhantomData;

use dxkb_common::{
    KeyState, dev_error, dev_info, dev_trace, dev_warn,
    util::{BitMatrix, BitMatrixLayout, BoundedU8, ColBitMatrixLayout, ConstCond, IsTrue},
};
use dxkb_peripheral::key_matrix::KeyMatrixLike;
use dxkb_split_link::SplitBusLike;
use heapless::Vec;
use serde::{Deserialize, Serialize};
use stm32f4xx_hal::{
    gpio::{PinPull, Pull},
    hal::digital::InputPin,
};
use usbd_hid::descriptor::KeyboardReport;

// Re-export it to be used for macros without needing to reference the usbd-hid crate.
pub use usbd_hid::descriptor::KeyboardUsage;

use crate::hid::HidKeyboard;

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
    pin: P,
}

impl<P: InputPin + PinPull> PinMasterSense<P> {
    pub fn new(mut pin: P) -> Self {
        pin.set_internal_resistor(Pull::Down);

        Self { pin }
    }
}

impl<P: InputPin> MasterCheck for PinMasterSense<P> {
    fn is_current_master(&mut self) -> bool {
        // TODO I don't expect this input to have any capacitance, so maybe it is subject to noise or something. Should I read it multiple times?
        self.pin.is_high().unwrap_or(false)
    }
}

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

pub trait SplitKeyboardLike<State> {
    type User;
    type Hid: HidKeyboard;

    fn state_mut(&mut self) -> &mut State;
    fn hid_mut(&mut self) -> &mut Self::Hid;
}

pub struct SplitKeyboard<
    const LLAYERS: u8,
    const LROWS: u8,
    const LCOLS: u8,
    const MROWS: u8,
    const MCOLS: u8,
    Side: SplitKeyboardSideType,
    Hid: HidKeyboard,
    LayoutConfig: SplitLayoutConfig,
    Key: HandleKey,
    Matrix: KeyMatrixLike<MROWS, MCOLS>,
    MasterTester: MasterCheck,
    SplitBus: SplitBusLike<SplitKeyboardLinkMessage>,
    User,
> where
    ColBitMatrixLayout<LCOLS>: BitMatrixLayout,
    [(); LLAYERS as usize]:,
    [(); LCOLS as usize]:,
    [(); LROWS as usize]:,
    ConstCond<{ LLAYERS > 0 }>: IsTrue,
{
    matrix: Matrix,
    layout: SplitKeyboardLayout<LayoutConfig, Key, LLAYERS, LROWS, LCOLS>,
    state: KeyboardState<Key, LLAYERS, LROWS, LCOLS>,
    pub split_bus: SplitBus,
    master_tester: MasterTester,
    is_master: bool,

    hid: Hid,

    _side: PhantomData<Side>,
    _layout_config: PhantomData<LayoutConfig>,
    _user: PhantomData<User>,
}

impl<
    const LLAYERS: u8,
    const LROWS: u8,
    const LCOLS: u8,
    const MROWS: u8,
    const MCOLS: u8,
    CurSide,
    Hid,
    LayoutConfig,
    Key,
    Matrix,
    MasterTester,
    SplitBus,
    User,
>
    SplitKeyboard<
        LLAYERS,
        LROWS,
        LCOLS,
        MROWS,
        MCOLS,
        CurSide,
        Hid,
        LayoutConfig,
        Key,
        Matrix,
        MasterTester,
        SplitBus,
        User,
    >
where
    CurSide: SideLayoutOffset<LayoutConfig>,
    CurSide::Opposite: SideLayoutOffset<LayoutConfig>,
    Hid: HidKeyboard,
    LayoutConfig: SplitLayoutConfig,
    Key: HandleKey<User = User>,
    ColBitMatrixLayout<LCOLS>: BitMatrixLayout,
    Matrix: KeyMatrixLike<MROWS, MCOLS>,
    MasterTester: MasterCheck,
    SplitBus: SplitBusLike<SplitKeyboardLinkMessage>,
    [(); LLAYERS as usize]:,
    [(); LCOLS as usize]:,
    [(); LROWS as usize]:,
    ConstCond<{ LLAYERS > 0 }>: IsTrue,
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
        hid: Hid,
        layout: SplitKeyboardLayout<LayoutConfig, Key, LLAYERS, LROWS, LCOLS>,
        matrix: Matrix,
        split_bus: SplitBus,
        master_tester: MasterTester,
    ) -> Self {
        const { Self::assert_config_ok() }
        Self {
            hid,
            matrix,
            layout,
            state: KeyboardState::new(),
            split_bus,
            master_tester,
            is_master: false,
            _side: PhantomData,
            _layout_config: PhantomData,
            _user: PhantomData,
        }
    }

    fn split_link_transfer_msg(split_bus: &mut SplitBus, msg: SplitKeyboardLinkMessage) {
        if let Err(e) = split_bus.transfer(msg) {
            dev_warn!("Couldn't transfer message through split link: {:?}", e);
        }
    }

    fn layout_update_key_state<Side: SideLayoutOffset<LayoutConfig>>(
        &mut self,
        row: u8,
        col: u8,
        current_state: KeyState,
        user: &mut User,
    ) {
        let (real_row, real_col) = self.layout.get_real_key_coordinate::<Side>(row, col);
        let changed = self
            .state
            .set_real_key_state(real_row, real_col, current_state);

        if changed {
            let key: Key = self
                .layout
                .get_key_definition(self.state.current_layer, real_row, real_col)
                .clone();
            key.handle_key_state_change::<_, Self>(self, user, current_state);
        }
    }

    fn poll_master(&mut self, user: &mut User) {
        let matrix_changed = self.matrix.scan_matrix();
        if matrix_changed {
            // TODO There has to be a better way to implement
            // this. Maybe eventually I can just copy the bitmatrix
            // to the layout bit matrix, making sure we only override
            // the bits from the current side of the keyboard. For
            // now following a naive implementation.
            for row in 0..MROWS {
                for col in 0..MCOLS {
                    self.layout_update_key_state::<CurSide>(
                        row,
                        col,
                        self.matrix.get_key_state(row, col),
                        user,
                    );
                }
            }
        }

        let mut incoming_split_msgs = Vec::<SplitKeyboardLinkMessage, 16>::new();
        self.split_bus.poll_into_vec(&mut incoming_split_msgs);
        for msg in incoming_split_msgs {
            match msg {
                SplitKeyboardLinkMessage::MatrixKeyDown { row, col } => {
                    self.layout_update_key_state::<CurSide::Opposite>(
                        row,
                        col,
                        KeyState::Pressed,
                        user,
                    );
                }
                SplitKeyboardLinkMessage::MatrixKeyUp { row, col } => {
                    self.layout_update_key_state::<CurSide::Opposite>(
                        row,
                        col,
                        KeyState::Released,
                        user,
                    );
                }
            }
        }

        if let Err(e) = self.hid.poll() {
            dev_error!("Usb stalled: {:?}", e);
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

    pub fn poll(&mut self, user: &mut User) {
        self.check_master();

        if self.is_master {
            self.poll_master(user);
        } else {
            self.poll_slave();
        }
    }
}

const ROLLED_OVER_KEYBOARD_REPORT: KeyboardReport = KeyboardReport {
    modifier: 0,
    reserved: 0,
    leds: 0,
    keycodes: [KeyboardUsage::KeyboardErrorRollOver as u8; 6],
};

pub struct KeyboardReportHolder {
    report: KeyboardReport,
    rollover: bool,
    dirty: bool,
}

impl KeyboardReportHolder {
    const fn new() -> Self {
        KeyboardReportHolder {
            report: KeyboardReport {
                modifier: 0,
                reserved: 0,
                leds: 0,
                keycodes: [0u8; 6],
            },
            rollover: false,
            dirty: false,
        }
    }

    pub fn rolled_over(&self) -> bool {
        self.rollover
    }

    pub fn report(&self) -> &KeyboardReport {
        if self.rollover {
            &ROLLED_OVER_KEYBOARD_REPORT
        } else {
            &self.report
        }
    }

    pub fn is_dirty(&self) -> bool {
        return self.dirty;
    }

    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    pub fn add_key(&mut self, key: KeyboardUsage) {
        if self.rollover {
            return;
        }

        if self.report.keycodes.contains(&(key as u8)) {
            return;
        }

        if let Some(free_slot) = self.report.keycodes.iter().position(|e| *e == 0) {
            self.report.keycodes[free_slot] = key as u8;
            self.dirty = true;
            return;
        }

        self.rollover = true;
        self.dirty = true;
    }

    pub fn rm_key(&mut self, key: KeyboardUsage) {
        if self.rollover {
            return;
        }

        if let Some(occ) = self.report.keycodes.iter().position(|e| *e == key as u8) {
            self.report.keycodes[occ] = 0;
            self.dirty = true;
        }
    }

    pub fn reset_keycodes(&mut self) {
        self.report.keycodes.fill(0);
        self.rollover = false;
    }
}

impl<
    const LLAYERS: u8,
    const LROWS: u8,
    const LCOLS: u8,
    const MROWS: u8,
    const MCOLS: u8,
    CurSide,
    Hid,
    LayoutConfig,
    Key,
    Matrix,
    MasterTester,
    SplitBus,
    User,
> SplitKeyboardLike<KeyboardState<Key, LLAYERS, LROWS, LCOLS>>
    for SplitKeyboard<
        LLAYERS,
        LROWS,
        LCOLS,
        MROWS,
        MCOLS,
        CurSide,
        Hid,
        LayoutConfig,
        Key,
        Matrix,
        MasterTester,
        SplitBus,
        User,
    >
where
    CurSide: SideLayoutOffset<LayoutConfig>,
    CurSide::Opposite: SideLayoutOffset<LayoutConfig>,
    Hid: HidKeyboard,
    LayoutConfig: SplitLayoutConfig,
    Key: HandleKey<User = User>,
    ColBitMatrixLayout<LCOLS>: BitMatrixLayout,
    Matrix: KeyMatrixLike<MROWS, MCOLS>,
    MasterTester: MasterCheck,
    SplitBus: SplitBusLike<SplitKeyboardLinkMessage>,
    [(); LLAYERS as usize]:,
    [(); LCOLS as usize]:,
    [(); LROWS as usize]:,
    ConstCond<{ LLAYERS > 0 }>: IsTrue,
{
    type User = User;
    type Hid = Hid;

    fn state_mut(&mut self) -> &mut KeyboardState<Key, LLAYERS, LROWS, LCOLS> {
        &mut self.state
    }

    fn hid_mut(&mut self) -> &mut Hid {
        &mut self.hid
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
pub struct LayerRow<Key, const COLS: u8>
where
    [(); COLS as usize]:,
{
    row: [Key; COLS as usize],
}

impl<Key, const COLS: u8> LayerRow<Key, COLS>
where
    [(); COLS as usize]:,
{
    pub const fn new(row: [Key; COLS as usize]) -> Self {
        Self { row }
    }

    pub fn new_from(row: [impl Into<Key>; COLS as usize]) -> Self {
        Self {
            row: row.map(|e| e.into()),
        }
    }
}

pub struct LayoutLayer<Key, const ROWS: u8, const COLS: u8>
where
    [(); COLS as usize]:,
    [(); ROWS as usize]:,
{
    keys: [LayerRow<Key, COLS>; ROWS as usize],
}

impl<Key, const ROWS: u8, const COLS: u8> LayoutLayer<Key, ROWS, COLS>
where
    [(); COLS as usize]:,
    [(); ROWS as usize]:,
{
    pub const fn new(keys: [LayerRow<Key, COLS>; ROWS as usize]) -> Self {
        Self { keys }
    }
}

impl<Key, const ROWS: u8, const COLS: u8> LayoutLayer<Key, ROWS, COLS>
where
    [(); COLS as usize]:,
    [(); ROWS as usize]:,
{
    fn get_key_definition(&self, row: u8, col: u8) -> &Key {
        &self.keys[row as usize].row[col as usize]
    }
}

/// Represents a type that can handle changes in the keyboard key states.
pub trait HandleKey: Sized + Clone {
    type User;

    /// Called when a key changes its state in the matrix. This function should
    /// run any action or mutation over the keyboard state. This function MUST
    /// NOT update the keyboard HID report. For that, use
    /// [`update_keyboard_report`].
    fn handle_key_state_change<S: KeyboardStateLike, Kb: SplitKeyboardLike<S>>(
        &self,
        kb: &mut Kb,
        user: &mut Self::User,
        key_state: KeyState,
    );

    // TODO Maybe have a function like this to separate the key state change from the keyboard report update.
    //  By doing that we might be able to create mechanisms for exiting from a rollover condition.
    // fn update_keyboard_report<S, Kb: SplitKeyboardLike<S>>(&self, kb: &mut Kb, user: &mut Kb::User, key_state: KeyState);
}

pub trait KeyboardStateLike {
    fn push_layer_raw(&mut self, new_layer: u8) -> bool;
    fn pop_layer_raw(&mut self) -> Option<u8>;
    fn push_next_layer(&mut self) -> bool;
}

pub struct KeyboardState<K: HandleKey, const LAYERS: u8, const ROWS: u8, const COLS: u8>
where
    ColBitMatrixLayout<COLS>: BitMatrixLayout,
    [(); ROWS as usize]:,
    [(); LAYERS as usize]:,
    [(); COLS as usize]:,
    [(); ROWS as usize]:,
    ConstCond<{ LAYERS > 0 }>: IsTrue,
{
    // Since ringbuffer doesn't implement any kind of "pop" operation
    // for dropping the most recently added element, for now I'm using
    // a Vec as alternative. If the stack gets out of memory, it will
    // smash the tip of the stack to make room for the new element.
    layers_stack: Vec<BoundedU8<LAYERS>, 8>,

    // TODO We could have a list of keys pressed here, that indicates the exact
    // keys that are pressed, and prevent any duplicated press if a given key is
    // bounded more than once to multiple keys in the matrix. However, this would
    // be costly in terms of memory and access time, so I'm leaving just the
    // matrix for now.
    /// The matrix holding the states of each key. This matrix holds not only
    /// the local pressed keys, like the key matrix controller, but also the
    /// states of the remote peer side.
    matrix_state: BitMatrix<{ ROWS as usize }, COLS>,

    /// The current layer selected, that will receive the keyboard events
    current_layer: BoundedU8<LAYERS>,

    /// The number of logical keys pressed right now.
    pressed_key_count: u8,
    _phantom: PhantomData<K>,
}

impl<K: HandleKey, const LAYERS: u8, const ROWS: u8, const COLS: u8>
    KeyboardState<K, LAYERS, ROWS, COLS>
where
    ColBitMatrixLayout<COLS>: BitMatrixLayout,
    [(); LAYERS as usize]:,
    [(); COLS as usize]:,
    [(); ROWS as usize]:,
    ConstCond<{ LAYERS > 0 }>: IsTrue,
{
    pub const fn new() -> Self {
        Self {
            layers_stack: Vec::new(),
            matrix_state: BitMatrix::<{ ROWS as usize }, COLS>::new(),
            current_layer: BoundedU8::ZERO,
            _phantom: PhantomData,
            pressed_key_count: 0,
        }
    }

    #[inline(always)]
    fn set_real_key_state(&mut self, row: u8, col: u8, state: KeyState) -> bool {
        let changed = self
            .matrix_state
            .set_value(row as usize, col, state.to_bool());

        if changed {
            match state {
                KeyState::Released => self.pressed_key_count -= 1,
                KeyState::Pressed => self.pressed_key_count += 1,
            }
        }

        changed
    }

    fn push_layer(&mut self, new_layer: BoundedU8<LAYERS>) -> bool {
        let len = self.layers_stack.len();
        if let Err(_) = self.layers_stack.push(new_layer) {
            self.layers_stack[len - 1] = new_layer;
        }
        self.current_layer = new_layer;
        true
    }

    fn pop_layer(&mut self) -> Option<BoundedU8<LAYERS>> {
        if let Some(head) = self.layers_stack.pop() {
            dev_trace!("Popped layer: {}", head);
            self.current_layer = head;
            Some(head)
        } else {
            dev_warn!("No layers to pop were available");
            None
        }
    }
}

impl<K: HandleKey, const LAYERS: u8, const ROWS: u8, const COLS: u8> KeyboardStateLike
    for KeyboardState<K, LAYERS, ROWS, COLS>
where
    ColBitMatrixLayout<COLS>: BitMatrixLayout,
    [(); LAYERS as usize]:,
    [(); COLS as usize]:,
    [(); ROWS as usize]:,
    ConstCond<{ LAYERS > 0 }>: IsTrue,
{
    fn push_layer_raw(&mut self, new_layer: u8) -> bool {
        let Some(layer_index) = BoundedU8::from_value(new_layer) else {
            dev_warn!("Requested new layer out of bounds: {}", new_layer);
            return false; // Out of bounds
        };

        self.push_layer(layer_index);
        true
    }

    fn pop_layer_raw(&mut self) -> Option<u8> {
        self.pop_layer().map(|x| x.index())
    }

    fn push_next_layer(&mut self) -> bool {
        if let Some(next) = self.current_layer.increment() {
            self.push_layer(next);
            true
        } else {
            false
        }
    }
}

pub struct SplitKeyboardLayout<
    C: SplitLayoutConfig,
    Key,
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
    layers: [LayoutLayer<Key, ROWS, COLS>; LAYERS as usize],
}

impl<C: SplitLayoutConfig, Key, const LAYERS: u8, const ROWS: u8, const COLS: u8>
    SplitKeyboardLayout<C, Key, LAYERS, ROWS, COLS>
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

    pub const fn new(layers: [LayoutLayer<Key, ROWS, COLS>; LAYERS as usize]) -> Self {
        const { Self::assert_config_ok() };

        Self {
            _config: PhantomData,
            layers,
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
    fn get_key_definition(&self, layer: BoundedU8<LAYERS>, real_row: u8, real_col: u8) -> &Key {
        self.layers[layer.index() as usize].get_key_definition(real_row, real_col)
    }
}
