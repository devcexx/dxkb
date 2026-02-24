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
#![feature(macro_metavar_expr_concat)]

mod config;
mod layout;

use config::*;

use cortex_m::interrupt::free;
use dxkb_common::{dev_info, util::RingBuffer};
use dxkb_core::{do_on_key_state, hid::HidKeyboard, keyboard::{HandleKey, KeyboardUsage, PinMasterSense}, log::RingBufferLogger};
use core::any::type_name;
use core::mem::MaybeUninit;
use core::ptr::addr_of_mut;
use dxkb_core::hid::{BasicKeyboardSettings, ReportHidKeyboard};
use dxkb_core::keyboard::SplitKeyboardLike;

use dxkb_peripheral::{clock::DWTClock, uart_dma_rb::HalfDuplexInitializer, BootloaderUtil, InterruptReceiver};
use dxkb_peripheral::key_matrix::IntoInputPinsWithSamePort;

#[allow(unused_imports)]
use panic_itm as _;

use cortex_m_rt::entry;
use dxkb_peripheral::uart_dma_rb::{DmaRingBuffer, UartDmaRb};
use dxkb_split_link::SplitBus;
use stm32f4xx_hal::{pac::{EXTI, USART1}, syscfg::SysCfg};
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
use usb_device::{class::UsbClass, LangID};
use usb_device::bus::UsbBusAllocator;
use usb_device::device::{StringDescriptors, UsbVidPid};

static mut EP_MEMORY: [u32; 1024] = [0; 1024];
static mut SPLIT_BUS_DMA_RX_BUF: DmaRingBuffer<256, 128> = DmaRingBuffer::new();
static mut SPLIT_BUS_DMA_TX_BUF: [u8; 256] = [0u8; 256];

static mut KEYBOARD: MaybeUninit<TKeyboard> = MaybeUninit::uninit();
static mut USB_ALLOC: MaybeUninit<UsbBusAllocator<UsbBus<USB>>> = MaybeUninit::uninit();

static mut HID_LOGGER: RingBufferLogger<1024> = RingBufferLogger::new(log::Level::Trace, RingBuffer::new());

impl HandleKey for CustomKey {
    type User = KeyboardContext;

    fn handle_key_state_change<S: dxkb_core::keyboard::KeyboardStateLike, Kb: dxkb_core::keyboard::SplitKeyboardLike<S>>(
        &self,
        kb: &mut Kb,
        user: &mut Self::User,
        key_state: dxkb_common::KeyState,
    ) {
        match self {
            CustomKey::Default(default_key) => default_key.handle_key_state_change(kb, &mut (), key_state),
            CustomKey::Plus => {
                let hid = kb.hid_mut();
                do_on_key_state!(key_state,
                    {
                        let _ = hid.press_key(KeyboardUsage::KeyboardLeftShift);
                        user.plus_pending_press = true;
                    },
                    {
                        let _ = hid.release_key(KeyboardUsage::KeyboardEqualPlus);
                        let _ = hid.release_key(KeyboardUsage::KeyboardLeftShift);
                    }
                );
            },
        }

    }
}

fn init_split_bus(
    usart: SplitBusUsartPort,
    dma: SplitBusDmaPeripheral,
    txrx_pin: SplitBusTxRxPin,
    clock: DWTClock,
    clocks: &Clocks,
    syscfg: &mut SysCfg,
    exti: &mut EXTI
) -> TSplitBus {
    let dma = StreamsTuple::new(dma);
    let uart_dma = UartDmaRb::init(
        HalfDuplexInitializer::new(usart, txrx_pin, dma.6, dma.5, syscfg, exti),
        unsafe { &mut SPLIT_BUS_DMA_TX_BUF },
        unsafe { &mut SPLIT_BUS_DMA_RX_BUF },
        &clocks,
    );

    SplitBus::new(uart_dma, clock, get_device_id())
}

fn init_key_matrix(rows: KeyMatrixRowPins, cols: KeyMatrixColPins, clocks: &Clocks) -> TKeyMatrix {
    let debouncer: TKeyMatrixDebounce = TKeyMatrixDebounce::new();
    TKeyMatrix::new(
        clocks.sysclk(),
        rows,
        cols,
        debouncer,
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
        .use_hse(8.MHz())
        .sysclk(96.MHz())
        .pclk1(48.MHz())
        .pclk2(48.MHz())
        .require_pll48clk()
        .freeze();

    let gpioa = dp.GPIOA.split();
    let gpiob = dp.GPIOB.split();

    //    itm_logger::init_with_level(log::Level::Trace).unwrap();
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
        USB_ALLOC.write(UsbBus::new(usb, addr_of_mut!(EP_MEMORY).as_mut().unwrap()))
    };

    let hid = ReportHidKeyboard::alloc(
        usb_alloc,
        &BasicKeyboardSettings {
            vid_pid: UsbVidPid(0x16c0, 0x27db),
            string_descriptors: &[StringDescriptors::new(LangID::ES)
                .serial_number("0")
                .manufacturer("devcexx")
                .product("STeMCell Lily58L")],
            poll_ms: 1,
        },
        unsafe {
            &HID_LOGGER
        },
    );

    let matrix = init_key_matrix(
        (
            gpiob.pb3.into_push_pull_output(),
            gpiob.pb4.into_push_pull_output(),
            gpiob.pb5.into_push_pull_output(),
            gpiob.pb8.into_push_pull_output(),
            gpiob.pb9.into_push_pull_output(),
        ),
        #[cfg(feature = "side-left")]
        (
            gpiob.pb1.into_pull_up_input(),
            gpiob.pb0.into_pull_up_input(),
            gpioa.pa5.into_pull_up_input(),
            gpioa.pa6.into_pull_up_input(),
            gpioa.pa7.into_pull_up_input(),
            gpioa.pa4.into_pull_up_input(),
        ),
        #[cfg(feature = "side-right")]
        (
            gpioa.pa4.into_pull_up_input(),
            gpioa.pa7.into_pull_up_input(),
            gpioa.pa6.into_pull_up_input(),
            gpioa.pa5.into_pull_up_input(),
            gpiob.pb0.into_pull_up_input(),
            gpiob.pb1.into_pull_up_input(),
        ),
        &clocks,
    );

    let split_bus = init_split_bus(dp.USART2, dp.DMA1, gpioa.pa2, clock, &clocks, &mut dp.SYSCFG.constrain(), &mut dp.EXTI);
    let master_tester = PinMasterSense::new(gpioa.pa9.into_pull_down_input());
    unsafe {
        KEYBOARD.write(TKeyboard::new(
            hid,
            layout::LAYOUT,
            matrix,
            split_bus,
            master_tester,
        ));
    }

    unsafe {
        // Go!
        free(|_cs| {
            NVIC::unmask(SplitBusUsartPort::INTERRUPT);
            NVIC::unmask(SplitBusTxDmaStream::INTERRUPT);
            NVIC::unmask(SplitBusTxRxPin::INTERRUPT);
        });
    }

    let mut kb_context = KeyboardContext::new();
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
            if kb_context.plus_pending_press {
                let _ = kb.hid_mut().press_key(KeyboardUsage::KeyboardEqualPlus);
                kb_context.plus_pending_press = false;
            }
        }

        kb.poll(&mut kb_context);
    }
}



#[interrupt]
fn USART2() {
    unsafe {
        KEYBOARD
            .assume_init_mut()
            .split_bus
            .bus_mut()
            .handle_usart_intr();
    }
}

#[interrupt]
fn DMA1_STREAM6() {
 unsafe {
     KEYBOARD
         .assume_init_mut()
         .split_bus
         .bus_mut()
         .handle_dma_intr();
 }
}

#[interrupt]
fn EXTI2() {
 unsafe {
     KEYBOARD
         .assume_init_mut()
         .split_bus
         .bus_mut()
         .handle_exti_intr();
 }
}
