use dxkb_core::{hid::ReportHidKeyboard, keyboard::{Left, Right, PinMasterSense, SplitKeyboard, SplitKeyboardLayout, SplitKeyboardLinkMessage, SplitLayoutConfig}, keys::DefaultKey};
use dxkb_peripheral::{clock::DWTClock, key_matrix::{DebouncerEagerPerKey, KeyMatrix, PinsWithSamePort, RowScan}, uart_dma_rb::{HalfDuplex, UartDmaRb}};
use dxkb_split_link::{SplitBus, TestingTimings};
use stm32f4xx_hal::{dma::{Stream5, Stream6, Stream7}, gpio::{Input, Output, Pin, PushPull}, otg_fs::USB, pac::{DMA1, DMA2, USART1, USART2}, signature::Uid};
use synopsys_usb_otg::UsbBus;

// The total layers of the layout.
const LAYERS: u8 = 3;

// The dimensions of each side of the keyboard.
const SIDE_ROWS: u8 = 5;
const SIDE_COLS: u8 = 6;

// The total dimensions of the keyboard, including both sides.
const LAYOUT_ROWS: u8 = SIDE_ROWS;
const LAYOUT_COLS: u8 = 2 * SIDE_COLS;

const DEBOUNCE_MILLIS: u8 = 20;

pub type KeyMatrixRowPins = (
    Pin<'B', 3, Output<PushPull>>,
    Pin<'B', 4, Output<PushPull>>,
    Pin<'B', 5, Output<PushPull>>,
    Pin<'B', 8, Output<PushPull>>,
    Pin<'B', 9, Output<PushPull>>,
);

#[cfg(feature = "side-left")]
pub type KeyMatrixColPins = (
    Pin<'B', 1, Input>,
    Pin<'B', 0, Input>,
    Pin<'A', 5, Input>,
    Pin<'A', 6, Input>,
    Pin<'A', 7, Input>,
    Pin<'A', 4, Input>,
);

#[cfg(feature = "side-right")]
pub type KeyMatrixColPins = (
    Pin<'A', 4, Input>,
    Pin<'A', 7, Input>,
    Pin<'A', 6, Input>,
    Pin<'A', 5, Input>,
    Pin<'B', 0, Input>,
    Pin<'B', 1, Input>,
);



// TODO
pub type UsbBusSensePin = Pin<'A', 9>;

pub type SplitBusTxRxPin = Pin<'A', 2>;
pub type SplitBusUsartPort = USART2;
pub type SplitBusDmaPeripheral = DMA1;

pub type SplitBusTxDmaStream = Stream6<SplitBusDmaPeripheral>;
pub type SplitBusRxDmaStream = Stream5<SplitBusDmaPeripheral>;

pub type SplitBusUsart = UartDmaRb<HalfDuplex<SplitBusUsartPort, SplitBusTxDmaStream, SplitBusRxDmaStream, 4, 4>, 256, 256, 128>;
pub type TSplitBus = SplitBus<SplitKeyboardLinkMessage, TestingTimings, SplitBusUsart, DWTClock, 32>;

pub type TKeyMatrixDebounce = DebouncerEagerPerKey<SIDE_ROWS, SIDE_COLS, DEBOUNCE_MILLIS>;
pub type TKeyMatrix = KeyMatrix<
    SIDE_ROWS,
    SIDE_COLS,
    KeyMatrixRowPins,
    KeyMatrixColPins,
    RowScan,
    TKeyMatrixDebounce,
>;

pub type TLayout = SplitKeyboardLayout<KeyboardLayoutConfig, CustomKey, LAYERS, LAYOUT_ROWS, LAYOUT_COLS>;

pub type TKeyboard<'b> = SplitKeyboard<
    LAYERS,
    LAYOUT_ROWS,
    LAYOUT_COLS,
    SIDE_ROWS,
    SIDE_COLS,
    CurrentSide,
    ReportHidKeyboard<'b, UsbBus<USB>, 1024>,
    KeyboardLayoutConfig,
    CustomKey,
    TKeyMatrix,
    PinMasterSense<UsbBusSensePin>,
    TSplitBus,
    KeyboardContext,
>;

pub struct KeyboardLayoutConfig;
impl SplitLayoutConfig for KeyboardLayoutConfig {
    const SPLIT_RIGHT_COL_OFFSET: u8 = SIDE_COLS;
}


#[cfg(feature = "side-left")]
pub type CurrentSide = Left;

#[cfg(feature = "side-right")]
pub type CurrentSide = Right;

#[cfg(not(any(feature = "side-right", feature = "side-left")))]
pub type CurrentSide = Left;

#[cfg(all(feature = "side-right", feature = "side-left"))]
compile_error!("Only side-left or side-right features must be enabled at a time!");


#[derive(Clone)]
pub enum CustomKey {
    Default(DefaultKey),
    /// When pressed, presses both the LShift and the = key, so the plus symbol can be sent without any other keystroke.
    Plus
}

#[macro_export]
macro_rules! custom_key_from_alias {
    (u:Pls) => {
        $crate::CustomKey::Plus
    };

    (u:LEx) => {
        $crate::CustomKey::Default(dxkb_core::default_key_from_alias!(f:PshLyrT(1)))
    };
    (u:LFn) => {
        $crate::CustomKey::Default(dxkb_core::default_key_from_alias!(f:PshLyrT(2)))
    };

    ($($other:tt)*) => {
        $crate::CustomKey::Default(dxkb_core::default_key_from_alias!($($other)*))
    }
}

pub struct KeyboardContext {
    pub plus_pending_press: bool,
}

impl KeyboardContext {
    pub const fn new() -> Self {
        Self { plus_pending_press: false }
    }
}

pub fn get_device_id() -> u128 {
    let mut uid = [0u8; 16];

    unsafe {
        core::ptr::copy_nonoverlapping(core::mem::transmute(Uid::get()), uid.as_mut_ptr(), size_of::<Uid>());
    };

    u128::from_le_bytes(uid)
}
