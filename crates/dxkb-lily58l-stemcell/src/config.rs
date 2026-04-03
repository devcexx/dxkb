use dxkb_core::{hid::ReportHidKeyboard, keyboard::{Left, Right, PinMasterSense, SplitKeyboard, SplitKeyboardLayout, SplitKeyboardLinkMessage, SplitLayoutConfig}, keys::DefaultKey};
use dxkb_peripheral::{clock::DWTClock, key_matrix::{DebouncerEagerPerKey, KeyMatrix, RowScan}, uart_dma_rb::{HalfDuplex, UartDmaRb}};
use dxkb_split_link::{DefaultSplitLinkTimings, SplitBus};
use stm32f4xx_hal::{dma::{Stream5, Stream6, Stream7}, gpio::{DynamicPin, Input, Output, Pin, PushPull}, otg_fs::USB, pac::{DMA1, DMA2, USART1, USART2}, signature::Uid};
use synopsys_usb_otg::UsbBus;

// The total layers of the layout.
const LAYERS: u8 = 4;

// The dimensions of each side of the keyboard.
const SIDE_ROWS: u8 = 5;
const SIDE_COLS: u8 = 6;

// The total dimensions of the keyboard, including both sides.
const LAYOUT_ROWS: u8 = SIDE_ROWS;
const LAYOUT_COLS: u8 = 2 * SIDE_COLS;

const DEBOUNCE_MILLIS: u8 = 20;

pub type KeyMatrixRowPins = (
    DynamicPin<'B', 3>,
    DynamicPin<'B', 4>,
    DynamicPin<'B', 5>,
    DynamicPin<'B', 8>,
    DynamicPin<'B', 9>,
);

#[cfg(feature = "side-left")]
pub type KeyMatrixColPins = (
    DynamicPin<'B', 1>,
    DynamicPin<'B', 0>,
    DynamicPin<'A', 5>,
    DynamicPin<'A', 6>,
    DynamicPin<'A', 7>,
    DynamicPin<'A', 4>,
);

#[cfg(feature = "side-right")]
pub type KeyMatrixColPins = (
    DynamicPin<'A', 4>,
    DynamicPin<'A', 7>,
    DynamicPin<'A', 6>,
    DynamicPin<'A', 5>,
    DynamicPin<'B', 0>,
    DynamicPin<'B', 1>,
);



// TODO
pub type UsbBusSensePin = Pin<'A', 9>;

pub type SplitBusTxRxPin = Pin<'A', 2>;
pub type SplitBusUsartPort = USART2;
pub type SplitBusDmaPeripheral = DMA1;

pub type SplitBusTxDmaStream = Stream6<SplitBusDmaPeripheral>;
pub type SplitBusRxDmaStream = Stream5<SplitBusDmaPeripheral>;

pub type SplitBusUsart = UartDmaRb<HalfDuplex<SplitBusUsartPort, SplitBusTxDmaStream, SplitBusRxDmaStream, 4, 4>, 256, 256, 128>;
pub type TSplitBus = SplitBus<SplitKeyboardLinkMessage, DefaultSplitLinkTimings, SplitBusUsart, DWTClock, 32>;

pub type TKeyMatrixDebounce = DebouncerEagerPerKey<SIDE_ROWS, SIDE_COLS, DEBOUNCE_MILLIS>;
pub type TKeyMatrix = KeyMatrix<
    SIDE_ROWS,
    SIDE_COLS,
    KeyMatrixRowPins,
    KeyMatrixColPins,
    RowScan,
    TKeyMatrixDebounce,
    ()
>;

pub type TLayout = SplitKeyboardLayout<KeyboardLayoutConfig, CustomKey, LAYERS, LAYOUT_ROWS, LAYOUT_COLS>;

pub type TKeyboard<'b> = SplitKeyboard<
    LAYERS,
    LAYOUT_ROWS,
    LAYOUT_COLS,
    SIDE_ROWS,
    SIDE_COLS,
    DWTClock,
    CurrentSide,
    ReportHidKeyboard<'b, UsbBus<USB>>,
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


#[derive(Clone, PartialEq, Eq)]
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
        $crate::CustomKey::Default(dxkb_core::default_key_from_alias!(f:LTRelSet(+1)))
    };
    (u:LFn) => {
        $crate::CustomKey::Default(dxkb_core::default_key_from_alias!(f:LTRelSet(+2)))
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
