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
#![feature(macro_metavar_expr_concat)]
#![feature(generic_const_items, min_generic_const_args, generic_const_args)]

mod keys;

use core::alloc;
use core::any::type_name;
use core::mem::MaybeUninit;
use core::ptr::addr_of_mut;
use dxkb_common::util::RingBuffer;
use dxkb_common::util::matrix::MatrixShape;
use dxkb_core::debug::{DebugHidFeature, NopDebugRead};
use dxkb_core::hid::HidKeyboard;

use dxkb_common::bus::{BusPollError, BusTransferError, NullBus};
use dxkb_common::dev_info;
use dxkb_core::hid::ReportHidKeyboard;
use dxkb_core::keyboard::{
    KeyboardShape, KeyboardUsage, LayoutShape, SplitKeyboard, SplitKeyboardLayout, SplitKeyboardLike, SplitKeyboardLinkMessage, SplitLayoutConfig, TKeyboardShape
};
use dxkb_core::keys::DefaultKey;
use dxkb_core::log::RingBufferLogger;
use dxkb_main::{CurrentSide, MasterCheckType, make_usb_master_checker};
use dxkb_peripheral::clock::DWTClock;
use dxkb_peripheral::key_matrix::{
    DebouncerEagerPerKey, KeyMatrix, RowScan
};
use dxkb_common::bus::BusWrite;
use dxkb_common::bus::BusRead;
use dxkb_peripheral::BootloaderUtil;
use keys::{CustomKey, CustomKeyContext};
use log::{info, Log, Record};

use cortex_m_rt::entry;
use dxkb_peripheral::uart_dma_rb::{DmaRingBuffer, FullDuplex, FullDuplexInitializer, HalfDuplex, HalfDuplexInitializer, UartDmaRb};
use dxkb_split_link::{SplitBus, TestingTimings};
use dxkb_core::usb::{UsbFeature, UsbFeatureSet};
use ringbuffer::ConstGenericRingBuffer;
use stm32f4xx_hal::dma::{Stream5, Stream7};
use stm32f4xx_hal::gpio::alt::sys;
use stm32f4xx_hal::gpio::{DynamicPin, Input, Output, Pin, PushPull};
use stm32f4xx_hal::pac::{Interrupt, DMA2, DWT, EXTI, USART1};
use stm32f4xx_hal::rcc::Clocks;
use stm32f4xx_hal::signature::Uid;
use stm32f4xx_hal::syscfg::SysCfg;
use stm32f4xx_hal::{
    dma::StreamsTuple,
    interrupt,
    otg_fs::USB,
    pac::{self, NVIC},
    prelude::*,
    rcc::RccExt,
};
use synopsys_usb_otg::UsbBus;
use usb_device::LangID;
use usb_device::bus::UsbBusAllocator;
use usb_device::device::{StringDescriptors, UsbDevice, UsbDeviceBuilder, UsbRev, UsbVidPid};
use dxkb_core::usb_itm_panic_handler::{self, UsbItmPanicHandlerConfig};

// The size of a side of the keyboard
type SideShape = MatrixShape<3, 5>;

// The shape of the keyboard layout. Includes the total size of both sides, and
// the number of layers in the layout.
type KbLayoutShape = LayoutShape<MatrixShape<3, 10>, 2>;

// The complete shape of the keyboard, including the layout shape and current side shape.
type TestKeyboardShape = KeyboardShape<KbLayoutShape, SideShape>;

type KeyMatrixRowPins = (
    DynamicPin<'B', 10>,
    DynamicPin<'B', 2>,
    DynamicPin<'B', 1>,
);

type KeyMatrixColPins = (
    DynamicPin<'A', 6>,
    DynamicPin<'A', 5>,
    DynamicPin<'A', 4>,
    DynamicPin<'A', 3>,
    DynamicPin<'A', 2>,
);

// Pin that will be used to test whether the current controller is
// receiving power from the USB bus:
//  - On STeMCell, this is already done in the board by wiring a connection from the USB
//  BUS to the A9 pin, through a voltage divider.
//  - On a development controller, this pin might not be available (on a BlackPill, it is not). For testing,
//    you might need to manually pull that pin, or pick other detection mechanism.
type UsbBusSensePin = Pin<'A', 9>;

// Pins for the Tx/Rx of the split bus. Note that this needs to be
// configured alongside the SplitBusUsart and SplitBusDmaPeripheral.
type SplitBusTxPin = Pin<'B', 6>;
type SplitBusRxPin = Pin<'B', 7>;

type KeyMatrixDebounce = DebouncerEagerPerKey<SideShape, 20>;
type KeyMatrixT = KeyMatrix<
    SideShape,
    KeyMatrixRowPins,
    KeyMatrixColPins,
    RowScan,
    KeyMatrixDebounce
>;

type SplitBusUsart = USART1;
type SplitBusDmaPeripheral = DMA2;

type SplitBusTxDmaStream = Stream7<SplitBusDmaPeripheral>;
type SplitBusRxDmaStream = Stream5<SplitBusDmaPeripheral>;

type SplitBusUart = UartDmaRb<FullDuplex<SplitBusUsart, SplitBusTxDmaStream, SplitBusRxDmaStream, 4, 4>, 256, 256, 128>;
type SplitBusT = SplitBus<SplitKeyboardLinkMessage, TestingTimings, SplitBusUart, DWTClock, 32>;

type LayoutT =
    SplitKeyboardLayout<KeyboardLayoutConfig, CustomKey, <TestKeyboardShape as TKeyboardShape>::LayoutShape>;
type KeyboardT<Hid> = SplitKeyboard<
    TestKeyboardShape,
    DWTClock,
    CurrentSide,
    Hid,
    KeyboardLayoutConfig,
    CustomKey,
    KeyMatrixT,
    MasterCheckType<UsbBusSensePin>,
    SplitBusT,
    CustomKeyContext,
>;

static mut EP_MEMORY: [u32; 1024] = [0; 1024];
static mut SPLIT_BUS_DMA_RX_BUF: DmaRingBuffer<256, 128> = DmaRingBuffer::new();
static mut SPLIT_BUS_DMA_TX_BUF: [u8; 256] = [0u8; 256];
static mut KEYBOARD: MaybeUninit<KeyboardT<ReportHidKeyboard<UsbBus<USB>>>> = MaybeUninit::uninit();
static mut USB_ALLOC: MaybeUninit<UsbBusAllocator<UsbBus<USB>>> = MaybeUninit::uninit();
static mut USB_DEVICE: MaybeUninit<UsbDevice<UsbBus<USB>>> = MaybeUninit::uninit();
static mut USB_DEBUG_HANDLER: MaybeUninit<DebugHidFeature<UsbBus<USB>, &'static RingBufferLogger<1024>>> = MaybeUninit::uninit();

static mut HID_LOGGER: RingBufferLogger<1024> = RingBufferLogger::new(log::Level::Trace, RingBuffer::new(), true);

struct KeyboardLayoutConfig;
impl SplitLayoutConfig for KeyboardLayoutConfig {
    const SPLIT_RIGHT_COL_OFFSET: u8 = 5;
}

fn get_device_id() -> u128 {
    let mut uid = [0u8; 16];

    unsafe {
        core::ptr::copy_nonoverlapping(core::mem::transmute(Uid::get()), uid.as_mut_ptr(), size_of::<Uid>());
    };

    u128::from_le_bytes(uid)
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
        FullDuplexInitializer::new(usart,
            (tx, rx),
            dma.7,
            dma.5
        ),
        unsafe { &mut SPLIT_BUS_DMA_TX_BUF },
        unsafe { &mut SPLIT_BUS_DMA_RX_BUF },
        &clocks,
    );

    SplitBus::new(uart_dma, clock, get_device_id())
}

fn init_key_matrix(rows: KeyMatrixRowPins, cols: KeyMatrixColPins, clocks: &Clocks) -> KeyMatrixT {
    let debouncer: KeyMatrixDebounce = KeyMatrixDebounce::new();
    KeyMatrixT::new(
        clocks.sysclk(),
        rows,
        cols,
        debouncer,
    )
}

#[rustfmt::skip]
fn build_keyboard_layout() -> LayoutT {

    LayoutT::new(
        dxkb_proc_macros::layers!(
            alias_resolver: custom_key_from_alias,
            layers: [
                {
                    name: "base",
                    rows: [
                        ['Q', 'W', 'E', 'R', 'T', 'Y', 'U', 'I', 'O', 'P'],
                        ['A', 'S', 'D', 'F', 'G', 'H', 'J', 'K', 'L', ';'],
                        [u:Plus, 'X', 'C', 'V', f:LTRelSet(+1), 'N', 'M', ',', '.', f:LTPsh(1)],
                    ]
                },

                {
                    name: "test1",
                    rows: [
                        ['0', '1', '2', '3', Caps, 'Y', 'U', 'I', 'O', 'P'],
                        ['A', 'S', 'D', 'F', 'G', 'H', 'J', 'K', 'L', ';'],
                        [u:Plus, 'X', 'C', 'V', f:LTRelSet(+1), 'N', 'M', ',', '.', f:LTPsh(1)],
                    ]
                }
            ]
        )
    )
}

#[entry]
fn main() -> ! {
    main0()
}

fn main0() -> ! {
    unsafe {
        BootloaderUtil::handle_bootloader_enter_request();
    }

    let mut dp = pac::Peripherals::take().unwrap();
    let mut cortex = cortex_m::Peripherals::take().unwrap();

    let rcc = dp.RCC.constrain();

    let clocks = rcc
        .cfgr
        .use_hse(25.MHz())
        .sysclk(96.MHz())
        .pclk1(48.MHz())
        .pclk2(48.MHz())
        .freeze();

    let gpioa = dp.GPIOA.split();
    let gpiob = dp.GPIOB.split();

    RingBufferLogger::install(unsafe { &HID_LOGGER }).unwrap();

    dev_info!("Device startup. Device configuration:");
    dev_info!(" - Current Side: {:?}", type_name::<CurrentSide>());

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

    let mut usb_feature_kb = ReportHidKeyboard::alloc(
        usb_alloc,
        1
    );

    let mut usb_feature_debug = unsafe {
        USB_DEBUG_HANDLER.write(DebugHidFeature::new(usb_alloc, unsafe { &HID_LOGGER }))
    };

    let mut usb_dev = unsafe {
        USB_DEVICE.write(
            UsbDeviceBuilder::new(usb_alloc, UsbVidPid(0x16c0, 0x27db))
                .usb_rev(UsbRev::Usb200)
                .supports_remote_wakeup(true)
                .strings(&[StringDescriptors::new(LangID::ES)
                    .serial_number("0")
                    .manufacturer("devcexx")
                    .product("dxkb testkb_3x5")])
                .unwrap()
                .build()
        )
    };

    usb_itm_panic_handler::setup(UsbItmPanicHandlerConfig {
        usb_reset: &|| {
            unsafe {
                USB_DEVICE.assume_init_mut().force_reset();
            }
        },
        usb_debug_poll: &|| {
            unsafe {
                let feature = USB_DEBUG_HANDLER.assume_init_mut();
                let device = USB_DEVICE.assume_init_mut();
                let r = device.poll(&mut feature.endpoints_mut());
                feature.usb_poll(device, r);
            }
        },
        clk: clock.clone()
    });

    let matrix = init_key_matrix(
        (
            gpiob.pb10.into_dynamic(),
            gpiob.pb2.into_dynamic(),
            gpiob.pb1.into_dynamic(),
        ),
        (
            gpioa.pa6.into_dynamic(),
            gpioa.pa5.into_dynamic(),
            gpioa.pa4.into_dynamic(),
            gpioa.pa3.into_dynamic(),
            gpioa.pa2.into_dynamic(),
        ),
        &clocks,
    );

    let mut split_bus = init_split_bus(dp.USART1, dp.DMA2, gpiob.pb6, gpiob.pb7, clock.clone(), &clocks);
    let master_tester = make_usb_master_checker(gpioa.pa9.into_input());
    unsafe {
        KEYBOARD.write(KeyboardT::new(
            clock.clone(),
            usb_feature_kb,
            build_keyboard_layout(),
            matrix,
            split_bus,
            master_tester,
        ));
    }

    unsafe {
        // Go!
        NVIC::unmask(Interrupt::USART1);
        NVIC::unmask(Interrupt::DMA2_STREAM7);
        NVIC::unmask(Interrupt::EXTI9_5);
    }

    let mut key_context = CustomKeyContext::new();
    loop {
        let kb =
            unsafe {
                KEYBOARD.assume_init_mut()
            };

        if !kb.hid_mut().dirty() {
            // Apparently, for pressing a combination of a modifier key plus a
            // key, we need to do it in phases. First, we need to send an IN
            // report with the press of the modifier key and then, in another
            // one, the press of the key needs to happen (while keeping the
            // modifier pressed.). Otherwise it won't be catched by Linux at least.
            // For releasing, nothing special is needed apparently.
            if key_context.plus_pending_press {
                kb.hid_mut().press_key(KeyboardUsage::KeyboardEqualPlus);
                key_context.plus_pending_press = false;
            }
        }

        (kb.hid_mut(), &mut *usb_feature_debug).poll_all(&mut usb_dev);
        kb.poll(&mut key_context, &mut usb_dev);
    }
}

#[interrupt]
fn USART1() {
    unsafe {
        KEYBOARD
            .assume_init_mut()
            .split_bus
            .bus_mut()
            .handle_usart_intr();
    }
}
