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
#![feature(generic_const_exprs)] // I'm sorry, I just want to do some basic math with const types.

use core::arch::asm;
use core::mem::MaybeUninit;
use core::ptr::addr_of_mut;

use cortex_m::delay::Delay;
use dxkb_common::dev_info;
use dxkb_peripheral::clock::DWTClock;
use dxkb_peripheral::key_matrix::{
    ColumnScan, DebouncerEagerPerKey, IntoInputPinsWithSamePort, KeyMatrix,
};

mod layout;

#[allow(unused_imports)]
use panic_itm as _;

use cortex_m_rt::entry;
use dxkb_peripheral::uart_dma_rb::{DmaRingBuffer, UartDmaRb};
use dxkb_split_link::{SplitBus, TestingTimings};
use stm32f4xx_hal::dma::{Stream5, Stream7};
use stm32f4xx_hal::gpio::{Output, Pin};
use stm32f4xx_hal::pac::{DMA2, Interrupt};
use stm32f4xx_hal::{
    dma::StreamsTuple,
    interrupt,
    otg_fs::USB,
    pac::{self, DWT, NVIC, OTG_FS_DEVICE},
    prelude::*,
    rcc::RccExt,
};
use synopsys_usb_otg::UsbBus;
use usb_device::{
    LangID,
    device::{StringDescriptors, UsbDeviceBuilder, UsbDeviceState, UsbVidPid},
};
use usbd_hid::descriptor::{KeyboardReport, KeyboardUsage, SerializedDescriptor};
use usbd_hid::hid_class::{
    HIDClass, HidClassSettings, HidCountryCode, HidProtocol, HidSubClass, ProtocolModeConfig,
};

type UartBus = UartDmaRb<pac::USART1, Stream7<DMA2>, Stream5<DMA2>, 4, 4, 256, 128>;

static mut EP_MEMORY: [u32; 1024] = [0; 1024];
static mut SPLIT_BUS_BUF: DmaRingBuffer<256, 128> = DmaRingBuffer::new();
static mut DMA_UART_TX_BUF: [u8; 256] = [0u8; 256];
static mut SPLIT_BUS: MaybeUninit<SplitBus<u8, TestingTimings, UartBus, DWTClock, 32>> =
    MaybeUninit::uninit();
static mut INTR_PIN: Pin<'B', 8, Output> = unsafe { core::mem::zeroed() };

#[entry]
fn main() -> ! {
    layout::do_something();

    main0()
}

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
    let gpioc = dp.GPIOC.split();

    let mut suspend_led = gpioc.pc13.into_push_pull_output();

    itm_logger::init_with_level(log::Level::Trace).unwrap();
    dev_info!("Device startup");

    const ROWS: u8 = 4;
    const COLS: u8 = 4;

    let mut delay = Delay::new(cortex.SYST, 96_000_000);

    let debouncer = DebouncerEagerPerKey::<ROWS, COLS, 50>::new();
    let rows = (
        gpioa.pa1.into_input(),
        gpioa.pa2.into_input(),
        gpioa.pa3.into_input(),
        gpioa.pa4.into_input(),
    )
        .into_input_pins_with_same_port();

    let cols = (
        gpioa.pa5.into_push_pull_output(),
        gpioa.pa6.into_push_pull_output(),
        gpioa.pa7.into_push_pull_output(),
        gpiob.pb0.into_push_pull_output(),
    );

    let mut matrix: KeyMatrix<ROWS, COLS, _, _, ColumnScan, _> =
        KeyMatrix::new(clocks.sysclk(), rows, cols, debouncer);

    #[rustfmt::skip]
    let layout = [
        KeyboardUsage::Keyboard1Exclamation, KeyboardUsage::Keyboard2At, KeyboardUsage::Keyboard3Hash, KeyboardUsage::Keyboard4Dollar,
        KeyboardUsage::KeyboardQq, KeyboardUsage::KeyboardWw, KeyboardUsage::KeyboardEe, KeyboardUsage::KeyboardRr,
        KeyboardUsage::KeyboardAa, KeyboardUsage::KeyboardSs, KeyboardUsage::KeyboardDd, KeyboardUsage::KeyboardFf,
        KeyboardUsage::KeyboardZz, KeyboardUsage::KeyboardXx, KeyboardUsage::KeyboardCc, KeyboardUsage::KeyboardVv,
    ];

    let usb = USB {
        usb_global: dp.OTG_FS_GLOBAL,
        usb_device: dp.OTG_FS_DEVICE,
        usb_pwrclk: dp.OTG_FS_PWRCLK,
        pin_dm: gpioa.pa11.into(),
        pin_dp: gpioa.pa12.into(),
        hclk: clocks.hclk(),
    };

    let rx = gpiob.pb7.into_alternate();
    let tx = gpiob.pb6.into_alternate();

    let dma2 = StreamsTuple::new(dp.DMA2);
    let dma2_2 = StreamsTuple::new(unsafe { DMA2::steal() });

    //SplitBus::init(dp.USART2, todo!(), dma2.7, dma2.5, &clocks);
    // Worked with 2995200 baud
    // let usart1 = Serial::new(
    //     dp.USART1,
    //     (tx, rx),
    //     Config::default()
    //         .baudrate(9600.bps())
    //         .parity_none()
    //         .stopbits(StopBits::STOP1)
    //         .dma(DmaConfig::TxRx),
    //     &clocks).unwrap();

    unsafe {
        //        NVIC::unmask(Interrupt::DMA2_STREAM7);
        //        NVIC::unmask(Interrupt::DMA2_STREAM4);
    }

    // loop {
    //     let mut last_ndr = 999;
    //     unsafe {
    //         usart_dma.write_dma(b"HOLA", None).unwrap();
    //     }
    //     while !dma2_2.7.is_transfer_complete() {
    //         let cur_ndr = dma2_2.7.number_of_transfers();
    //         if last_ndr != cur_ndr {
    //             dev_info!("N: {}", cur_ndr);
    //             last_ndr = cur_ndr;
    //         }
    //     }
    //     suspend_led.toggle();
    //     delay.delay_ms(1000);
    //     usart_dma.handle_dma_interrupt();
    //     usart_dma.handle_dma_interrupt();
    // }

    let bus_allocator = UsbBus::new(usb, unsafe { addr_of_mut!(EP_MEMORY).as_mut().unwrap() });

    // TODO NKRO support
    // TODO HID standard requires GET_REPORT to be supported by
    // keyboards. However, usbd-hid is not providing support for this
    // right now. Shall I try to support it?

    // TODO Make USART NVIC interrupts a priority in which they cannot be preempted by any other interrupt.

    let mut prev_usb_status: Option<UsbDeviceState> = None;

    let mut last_led_change_ticks = DWT::cycle_count();

    let uart_dma = UartDmaRb::init(
        dp.USART1,
        (tx, rx),
        dma2.7,
        dma2.5,
        unsafe { &mut DMA_UART_TX_BUF },
        unsafe { &mut SPLIT_BUS_BUF },
        &clocks,
    );

    let clock = DWTClock::new(&clocks, &mut cortex.DCB, &mut cortex.DWT);

    let split_bus = unsafe {
        INTR_PIN = gpiob.pb8.into_push_pull_output();
        INTR_PIN.set_high();

        let sb = SPLIT_BUS.write(SplitBus::new(uart_dma, clock.clone()));

        //        NVIC::unmask(Interrupt::USART1);
        sb
    };

    loop {
        // let mut count = 0;
        // split_bus.poll(|frame| {
        //     count+=1;

        //     dev_info!("Received frame: {:?}", frame);
        // });

        // let cur_usb_status = Some(usb_dev.state());
        // if prev_usb_status != cur_usb_status {
        //     dev_info!("USB device status changed to {:?}; Remote wakeup? {}", usb_dev.state(), usb_dev.remote_wakeup_enabled());
        //     prev_usb_status = Some(usb_dev.state());
        // }

        // let changed = matrix.scan_matrix();
        // if usb_dev.state() == UsbDeviceState::Suspend {
        //     if usb_dev.remote_wakeup_enabled() {
        //         // Just a led animation for testing
        //         let elapsed_ms = (DWT::cycle_count() - last_led_change_ticks) as u64 * 1000 / clocks.hclk().raw() as u64;
        //         if elapsed_ms > 300 {
        //             last_led_change_ticks = DWT::cycle_count();
        //             suspend_led.toggle();
        //         }

        //         if changed {
        //             suspend_led.set_high();
        //             let dv = unsafe {
        //                 OTG_FS_DEVICE::steal()
        //             };
        //             dv.dctl().modify(|_, w| {
        //                 w.rwusig().set_bit()
        //             });
        //             delay.delay_ms(5); // According to datasheet, bit needs to be turned on between 1 and 15 ms.
        //             dv.dctl().modify(|_, w| {
        //                 w.rwusig().clear_bit()
        //             });
        //         }
        //     } else {
        //         suspend_led.set_high();
        //     }

        // } else {
        //     suspend_led.set_high();
        //     let mut report = KeyboardReport::default();

        //     let mut next_index = 0;
        //     'out: for col in 0..4 {
        //         for row in 0..4 {
        //             if next_index >= 6 {
        //                 break 'out;
        //             }

        //             if matrix.get_key_state(row, col) == KeyState::Pressed {
        //                 report.keycodes[next_index] = layout[row as usize * 4 + col as usize] as u8;
        //                 next_index += 1;
        //             }
        //         }
        //     }

        //    // let _ = kbd_hid.push_input(&report);
        // }
    }
}

#[interrupt]
fn USART1() {
    unsafe {
        INTR_PIN.set_low();
        asm!(
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
        );
        INTR_PIN.set_high();
    }
    let split_bus = unsafe { SPLIT_BUS.assume_init_mut() };

    split_bus.bus().handle_usart_intr();
    unsafe {
        INTR_PIN.set_low();
        asm!(
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
        );
        INTR_PIN.set_high();
    }
}
