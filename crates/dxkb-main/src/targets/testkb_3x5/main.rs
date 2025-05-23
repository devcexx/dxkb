// This DXKB version is experimental and subject to deep changes in
// the future. This version should include the bare minimum for
// running a split keyboard, including USB support, USB remote wake
// up, key matrix, layout definition, and a communication port between
// the two parts of the keyboard.

//TODO Test in the future to integrate with RTIC for better handling
//interrupts? https://github.com/rtic-rs/rtic

#![no_std]
#![no_main]
#![allow(incomplete_features)]
#![allow(static_mut_refs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::bare_urls)]
#![feature(generic_const_exprs)]

use core::mem::MaybeUninit;
use core::ptr::addr_of_mut;

use cortex_m::interrupt::CriticalSection;
use dxkb_common::dev_info;
use dxkb_core::keyboard::{
    LayerRow, LayoutLayer, Left, SplitKeyboard, SplitKeyboardLayout, SplitKeyboardLinkMessage,
    SplitLayoutConfig,
};
use dxkb_core::keys;
use dxkb_peripheral::clock::DWTClock;
use dxkb_peripheral::key_matrix::{
    ColumnScan, DebouncerEagerPerKey, IntoInputPinsWithSamePort, KeyMatrix, PinsWithSamePort,
    RowScan,
};

#[allow(unused_imports)]
use panic_itm as _;

use cortex_m_rt::entry;
use dxkb_peripheral::uart_dma_rb::{DmaRingBuffer, UartDmaRb};
use dxkb_split_link::{DefaultSplitLinkTimings, SplitBus, TestingTimings};
use stm32f4xx_hal::dma::{Stream5, Stream7};
use stm32f4xx_hal::gpio::{Input, Output, Pin, PushPull};
use stm32f4xx_hal::pac::{DMA2, Interrupt, USART1};
use stm32f4xx_hal::rcc::Clocks;
use stm32f4xx_hal::{
    dma::StreamsTuple,
    interrupt,
    otg_fs::USB,
    pac::{self, NVIC},
    prelude::*,
    rcc::RccExt,
};
use synopsys_usb_otg::UsbBus;
use usb_device::bus::UsbBusAllocator;
use usbd_hid::descriptor::KeyboardUsage;

const LAYERS: u8 = 1;
const ROWS: u8 = 3;
const COLS: u8 = 5;

type KeyMatrixRowPins = (
    Pin<'B', 10, Output<PushPull>>,
    Pin<'B', 2, Output<PushPull>>,
    Pin<'B', 1, Output<PushPull>>,
);

type KeyMatrixColPins = (
    Pin<'A', 6, Input>,
    Pin<'A', 5, Input>,
    Pin<'A', 4, Input>,
    Pin<'A', 3, Input>,
    Pin<'A', 2, Input>,
);

type KeyMatrixDebounce = DebouncerEagerPerKey<ROWS, COLS, 20>;
type KeyMatrixT = KeyMatrix<
    ROWS,
    COLS,
    KeyMatrixRowPins,
    PinsWithSamePort<KeyMatrixColPins>,
    RowScan,
    KeyMatrixDebounce,
>;

type SplitBusUsart = USART1;
type SplitBusTxPin = Pin<'B', 6>;
type SplitBusRxPin = Pin<'B', 7>;
type SplitBusDmaPeripheral = DMA2;

type SplitBusTxDmaStream = Stream7<SplitBusDmaPeripheral>;
type SplitBusRxDmaStream = Stream5<SplitBusDmaPeripheral>;

type SplitBusUart =
    UartDmaRb<SplitBusUsart, SplitBusTxDmaStream, SplitBusRxDmaStream, 4, 4, 256, 128>;
type SplitBusT = SplitBus<SplitKeyboardLinkMessage, TestingTimings, SplitBusUart, DWTClock, 32>;

type LayoutT = SplitKeyboardLayout<KeyboardLayoutConfig, LAYERS, ROWS, COLS>;
type KeyboardT<'usb, USB> = SplitKeyboard<
    'usb,
    LAYERS,
    ROWS,
    COLS,
    ROWS,
    COLS,
    Left,
    USB,
    KeyboardLayoutConfig,
    KeyMatrixT,
    SplitBusT,
>;

static mut EP_MEMORY: [u32; 1024] = [0; 1024];
static mut SPLIT_BUS_DMA_RX_BUF: DmaRingBuffer<256, 128> = DmaRingBuffer::new();
static mut SPLIT_BUS_DMA_TX_BUF: [u8; 256] = [0u8; 256];
static mut KEYBOARD: MaybeUninit<KeyboardT<UsbBus<USB>>> = MaybeUninit::uninit();
static mut USB_ALLOC: MaybeUninit<UsbBusAllocator<UsbBus<USB>>> = MaybeUninit::uninit();

struct KeyboardLayoutConfig;
impl SplitLayoutConfig for KeyboardLayoutConfig {
    const SPLIT_RIGHT_COL_OFFSET: u8 = 0;
}

fn init_split_bus(
    usart: USART1,
    dma: SplitBusDmaPeripheral,
    tx_pin: SplitBusTxPin,
    rx_pin: SplitBusRxPin,
    clock: DWTClock,
    clocks: &Clocks,
) -> SplitBusT {
    let rx = rx_pin.into_alternate();
    let tx = tx_pin.into_alternate();

    let dma = StreamsTuple::new(dma);
    let uart_dma = UartDmaRb::init(
        usart,
        (tx, rx),
        dma.7,
        dma.5,
        unsafe { &mut SPLIT_BUS_DMA_TX_BUF },
        unsafe { &mut SPLIT_BUS_DMA_RX_BUF },
        &clocks,
    );

    SplitBus::new(uart_dma, clock)
}

fn init_key_matrix(rows: KeyMatrixRowPins, cols: KeyMatrixColPins, clocks: &Clocks) -> KeyMatrixT {
    let debouncer: KeyMatrixDebounce = KeyMatrixDebounce::new();
    KeyMatrixT::new(
        clocks.sysclk(),
        rows,
        cols.into_input_pins_with_same_port(),
        debouncer,
    )
}

#[rustfmt::skip]
fn build_keyboard_layout() -> LayoutT {
    LayoutT::new([
        LayoutLayer::new([
            LayerRow::new_from([KeyboardUsage::KeyboardQq,                           KeyboardUsage::KeyboardWw,         KeyboardUsage::KeyboardEe,           KeyboardUsage::KeyboardRr, KeyboardUsage::KeyboardTt]),
            LayerRow::new_from([KeyboardUsage::KeyboardAa,                           KeyboardUsage::KeyboardSs,         KeyboardUsage::KeyboardDd,           KeyboardUsage::KeyboardFf, KeyboardUsage::KeyboardGg]),
            LayerRow::new_from([KeyboardUsage::KeyboardZz,                           KeyboardUsage::KeyboardXx,         KeyboardUsage::KeyboardCc,           KeyboardUsage::KeyboardVv, KeyboardUsage::KeyboardBb]),
        ])
    ])
}

#[entry]
fn main() -> ! {
    main0()
}

// TODO replace by automatic master detection.
#[cfg(feature = "master")]
const MASTER: bool = true;
#[cfg(not(feature = "master"))]
const MASTER: bool = false;

fn main0() -> ! {
    let dp = pac::Peripherals::take().unwrap();
    let mut cortex = cortex_m::Peripherals::take().unwrap();

    let rcc = dp.RCC.constrain();

    let clocks = rcc
        .cfgr
        .use_hse(25.MHz())
        .sysclk(96.MHz())
        .pclk1(48.MHz())
        .pclk2(48.MHz())
        .require_pll48clk()
        .freeze();

    let gpioa = dp.GPIOA.split();
    let gpiob = dp.GPIOB.split();

    itm_logger::init_with_level(log::Level::Trace).unwrap();
    dev_info!("Device startup");

    let clock = DWTClock::new(&clocks, &mut cortex.DCB, &mut cortex.DWT);

    let usb = USB {
        usb_global: dp.OTG_FS_GLOBAL,
        usb_device: dp.OTG_FS_DEVICE,
        usb_pwrclk: dp.OTG_FS_PWRCLK,
        pin_dm: gpioa.pa11.into(),
        pin_dp: gpioa.pa12.into(),
        hclk: clocks.hclk(),
    };
    let usb_alloc = unsafe {
        USB_ALLOC.write(UsbBus::new(usb, unsafe {
            addr_of_mut!(EP_MEMORY).as_mut().unwrap()
        }))
    };

    let matrix = init_key_matrix(
        (
            gpiob.pb10.into_push_pull_output(),
            gpiob.pb2.into_push_pull_output(),
            gpiob.pb1.into_push_pull_output(),
        ),
        (
            gpioa.pa6.into_pull_up_input(),
            gpioa.pa5.into_pull_up_input(),
            gpioa.pa4.into_pull_up_input(),
            gpioa.pa3.into_pull_up_input(),
            gpioa.pa2.into_pull_up_input(),
        ),
        &clocks,
    );

    let split_bus = init_split_bus(dp.USART1, dp.DMA2, gpiob.pb6, gpiob.pb7, clock, &clocks);
    unsafe {
        KEYBOARD.write(KeyboardT::new(
            usb_alloc,
            build_keyboard_layout(),
            matrix,
            split_bus,
            MASTER,
        ));
    }

    unsafe {
        // Go!
        NVIC::unmask(Interrupt::USART1);
    }

    loop {
        unsafe {
            KEYBOARD.assume_init_mut().poll();
        }
    }
}

#[interrupt]
fn USART1() {
    unsafe {
        KEYBOARD
            .assume_init_ref()
            .split_bus
            .bus()
            .handle_usart_intr();
    }
}
