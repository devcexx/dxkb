//! Defines methods for handling a UART line in which the Rx line is
//! handled by a DMA stream that writes the incoming data forever into
//! a ring buffer.

use core::cell::UnsafeCell;
use core::fmt::Debug;
use core::mem;
use cortex_m::interrupt::Mutex;
use dxkb_common::dev_trace;
use dxkb_common::{
    bus::{BusPollError, BusRead, BusTransferError, BusWrite}, dev_info, dev_warn
};
use enumflags2::{BitFlag, BitFlags};
use ringbuffer::{ConstGenericRingBuffer, RingBuffer};
use stm32f4xx_hal::gpio::{Edge, ExtiPin};
use stm32f4xx_hal::pac::EXTI;
use stm32f4xx_hal::syscfg::SysCfg;
use stm32f4xx_hal::Ptr;
use stm32f4xx_hal::dma::{DmaChannel, DmaEvent, DmaFlag, MemoryToPeripheral};
use stm32f4xx_hal::dma::traits::DMASet;
use stm32f4xx_hal::pac::usart1::RegisterBlock;
use stm32f4xx_hal::serial::{CFlag, Flag, Instance, RxISR};
use stm32f4xx_hal::{
    ClearFlags, Listen, ReadFlags,
    dma::{
        self, ChannelX, PeripheralToMemory,
        config::{BurstMode, FifoThreshold},
        traits::{Channel, Direction, PeriAddress, Stream, StreamISR},
    },
    gpio::PushPull,
    rcc::Clocks,
    serial::{
        Config, Event, Serial,
        config::{DmaConfig, StopBits},
    },
    time::U32Ext,
};

/// Runs a interrupt-free code block, taking a mutable reference from
/// the given [`core::cell::UnsafeCell`] at the beginning of the free
/// block, making sure the creation of these mutable references only
/// happens once in the lifetime of the free block. it is * UNSAFE *
/// to call this macro from itself, so please don't do it.
#[macro_export]
macro_rules! free_with_muts {
    ($($ident:ident <- $ref:expr),*, || $block:block) => {
        cortex_m::interrupt::free(|cs| {
            $(
               let $ident = unsafe { &mut *($ref).borrow(cs).get() };
            )*

            $block
        })
    };
}

#[derive(Debug)]
enum CloseCurrentFrameError {
    CurrentFrameTooBig,
    NoSpaceLeft,
}

#[derive(Debug, Clone)]
struct RbSection {
    len: u16,
    kind: RbSectionKind,
}

#[derive(Debug, Clone)]
enum RbSectionKind {
    Discarded,
    Frame,
}

// TODO Rename
struct DmaRingBufferWriteSide<const BUF_LEN: usize, const MAX_FRAME_COUNT: usize> {
    /// A ring buffer that will hold the size in bytes of each frame
    /// received and pending to be polled in the reception
    /// buffer. This is used for prevent the need to copy the just
    /// read frame during, or just right after the interrupt. Just
    /// letting the application decide when to read it.
    frames: ConstGenericRingBuffer<RbSection, MAX_FRAME_COUNT>,

    /// The index in the reception buffer in which the current frame
    /// is being stored.
    current_frame_begin_off: usize,
}

impl<const BUF_LEN: usize, const MAX_FRAME_COUNT: usize>
    DmaRingBufferWriteSide<BUF_LEN, MAX_FRAME_COUNT>
{
    fn current_frame_length(ndt: u16, begin_off: usize) -> usize {
        let last_frame_off = BUF_LEN - ndt as usize;
        if last_frame_off >= begin_off {
            last_frame_off - begin_off
        } else {
            BUF_LEN - begin_off + last_frame_off
        }
    }

    fn push_rb_frame<F: FnOnce(u16) -> RbSection>(
        &mut self,
        ndt: u16,
        ctor: F,
    ) -> Result<(), CloseCurrentFrameError> {
        let cur_frame_len = Self::current_frame_length(ndt, self.current_frame_begin_off);
        if cur_frame_len == 0 {
            return Ok(());
        }

        if cur_frame_len > 65535 {
            return Err(CloseCurrentFrameError::CurrentFrameTooBig);
        }

        if self.frames.is_full() {
            Err(CloseCurrentFrameError::NoSpaceLeft)
        } else {
            self.frames.enqueue(ctor(cur_frame_len as u16));
            // Store where the next frame will start.
            self.current_frame_begin_off = (self.current_frame_begin_off + cur_frame_len) % BUF_LEN;
            Ok(())
        }
    }

    #[inline(always)]
    fn close_current_frame(&mut self, ndt: u16) -> Result<(), CloseCurrentFrameError> {
        self.push_rb_frame(ndt, |len| RbSection {
            len,
            kind: RbSectionKind::Frame,
        })
    }

    #[inline(always)]
    pub fn discard_current_frame(&mut self, ndt: u16) -> Result<(), CloseCurrentFrameError> {
        self.push_rb_frame(ndt, |len| RbSection {
            len,
            kind: RbSectionKind::Discarded,
        })
    }
}

pub struct DmaRingBuffer<const BUF_LEN: usize, const MAX_RB_FRAG_COUNT: usize> {
    /// The circular buffer to which the DMA will write the received bytes.
    buf: [u8; BUF_LEN],

    /// The offset from which the next frame will be read from `buf`
    /// from the user side.
    read_off: Mutex<UnsafeCell<usize>>,

    // Using an unsafe cell here for avoiding the overload of a
    // RefCell on an interrupt context. Use it with care.
    write_side: Mutex<UnsafeCell<DmaRingBufferWriteSide<BUF_LEN, MAX_RB_FRAG_COUNT>>>,
}

impl<const BUF_LEN: usize, const MAX_FRAME_COUNT: usize> DmaRingBuffer<BUF_LEN, MAX_FRAME_COUNT> {
    pub const fn new() -> Self {
        Self {
            buf: [0u8; BUF_LEN],
            read_off: Mutex::new(UnsafeCell::new(0)),
            write_side: Mutex::new(UnsafeCell::new(DmaRingBufferWriteSide {
                frames: ConstGenericRingBuffer::new(),
                current_frame_begin_off: 0,
            })),
        }
    }

    #[inline]
    fn copy_next_read_buffer_bytes(&self, read_off: usize, buf: &mut [u8]) {
        // buf cannot be greater than the internal circular buffer size!

        if read_off + buf.len() >= BUF_LEN {
            let first_chunk_len = BUF_LEN - read_off;
            buf[0..first_chunk_len].copy_from_slice(&self.buf[read_off..BUF_LEN]);

            let leftover_len = buf.len() - first_chunk_len;
            buf[first_chunk_len..].copy_from_slice(&self.buf[0..leftover_len]);
        } else {
            buf.copy_from_slice(&self.buf[read_off..read_off + buf.len()]);
        }
    }

    pub fn poll_next(&self, buf: &mut [u8]) -> Result<u16, BusPollError> {
        loop {
            let (read_off, section) = free_with_muts!(
            side <- self.write_side,
            read_off <- self.read_off,

            || {
                let prev_off = *read_off;

                let frame = side.frames.dequeue();
                if let Some(frame) = frame {
                    *read_off = (*read_off + frame.len as usize) % BUF_LEN;
                    Some((prev_off, frame))
                } else {
                    None
                }
            })
            .ok_or(BusPollError::WouldBlock)?;

            if section.len as usize > buf.len() {
                dev_warn!(
                    "Discarded frame that is greater than the rx buffer ({} > {})",
                    section.len,
                    buf.len()
                );
                // In this case, we drop the frame since it is
                // unlikely that the caller will be able to handle it
                // in a next call.
                return Err(BusPollError::BufferOverflow);
            }

            match section.kind {
                RbSectionKind::Discarded => {
                    dev_trace!("Polled discard frame: {}", section.len);
                }
                RbSectionKind::Frame => {
                    dev_trace!("Polled frame: {}", section.len);
                    self.copy_next_read_buffer_bytes(read_off, &mut buf[0..section.len as usize]);
                    return Ok(section.len);
                }
            }
        }
    }
}

pub struct UsartConfig {
    half_duplex: bool,
    baud_rate: u32,
    dma_tx: bool,
    dma_rx: bool
}

pub trait UsartExt {
    fn setup(&self, config: &UsartConfig, clocks: &Clocks);
    fn clear_idle_interrupt(&self);
    fn flags(&self) -> BitFlags<Flag>;
    fn set_interrupt_mask(&self, intrs: impl Into<BitFlags<Event>>);
    fn tx_set_enabled(&self, enabled: bool);
    fn rx_set_enabled(&self, enabled: bool);
    fn set_error_interrupt_enable(&self, enable: bool);

}

pub trait HandleExtiIntr {
    fn handle_exti_intr(&mut self);
}

pub trait HandleDmaIntr {
    fn handle_dma_intr(&mut self);
}

pub trait UartLineModeInit {
    type Mode;

    fn init(self, rx_buf: &[u8], clocks: &Clocks) -> Self::Mode;
}

pub trait UartLineMode where Serial<Self::Usart, u8>: Listen<Event = Event> + ReadFlags<Flag = Flag> + ClearFlags<Flag = CFlag> {
    type Usart: Instance<RB = RegisterBlock>;
    type DmaTxStream: Stream + StreamISR;
    type DmaRxStream: Stream + StreamISR;

    fn usart(&self) -> &Self::Usart;

    fn dma_tx_stream(&self) -> &Self::DmaTxStream;
    fn dma_rx_stream(&self) -> &Self::DmaRxStream;
    fn handle_usart_intr(&mut self, flags: BitFlags<Flag>);

    fn transfer(&mut self, data_buf: &[u8], tx_buf: &mut [u8]) -> Result<(), BusTransferError>;
    fn is_tx_busy(&self) -> bool;

//    fn intr_handle_tx_completed(&sef);


//    type PINS<Otype>;

//    fn configure_mode<U: Instance + Ptr<RB = RegisterBlock> + 'static, O>(&self, usart: &mut U, pins: &mut Self::PINS<O>);
}

impl<T> UsartExt for T where T: Instance + Ptr<RB = RegisterBlock> {
    fn setup(&self, config: &UsartConfig, clocks: &Clocks) {
        unsafe {
            // Enable peripheral.
            T::enable_unchecked();
            T::reset_unchecked();
        }

        let pclk_freq = T::clock(clocks).raw();
        let (over8, div) = calculate_brr(pclk_freq, config.baud_rate);

        self.brr().write(|w| unsafe { w.bits(div) });

        self.cr2().write(|w| {
            w.stop().stop1() // 1 Stop bit
        });

        self.cr3().write(|w| {
            w.hdsel().bit(config.half_duplex)
        });

        self.cr1().write(|w| {
            w.ue().set_bit()
                .over8().bit(over8)
                .m().clear_bit() // 8 Bit
                .pce().clear_bit() // Hardware parity control disabled
                .ps().clear_bit() // Disable parity bit
        });

        if config.dma_rx && config.dma_tx {
            self.cr3().modify(|_, w| {
                w.dmar().enabled();
                w.dmat().enabled()
            });
        } else if config.dma_rx {
            self.cr3().modify(|_, w| {
                w.dmar().enabled()
            });
        } else if config.dma_tx {
            self.cr3().modify(|_, w| {
                w.dmat().enabled()
            });
        }
    }

    fn clear_idle_interrupt(&self) {
        let _ = self.sr().read();
        let _ = self.dr().read();
    }

    fn flags(&self) -> BitFlags<Flag> {
        unsafe {
            BitFlags::from_bits_unchecked(self.sr().read().bits())
        }
    }

    fn tx_set_enabled(&self, enabled: bool) {
        self.cr1().modify(|_, w| {
            w.te().bit(enabled)
        });
    }

    fn rx_set_enabled(&self, enabled: bool) {
        self.cr1().modify(|_, w| {
            w.re().bit(enabled)
        });
    }
//     Idle = 1 << 4,
//     /// RXNE interrupt enable
//     RxNotEmpty = 1 << 5,
//     /// Transmission complete interrupt enable
//     TransmissionComplete = 1 << 6,
//     /// TXE interrupt enable
//     TxEmpty = 1 << 7,
//     /// PE interrupt enable
//     ParityError = 1 << 8,
// }

    fn set_interrupt_mask(&self, intrs: impl Into<BitFlags<Event>>) {
        self.cr1().modify(|r, w| unsafe {
            let mut bits = r.bits();
            bits &= !(BitFlags::<Event>::all().bits());
            bits |= intrs.into().bits();
            w.bits(bits)
        });
    }

    fn set_error_interrupt_enable(&self, enable: bool) {
        self.cr3().modify(|_, w| {
            w.eie().bit(enable)
        });
    }
}


fn calculate_brr(pclk_freq: u32, baud: u32) -> (bool, u32) {
    // The frequency to calculate USARTDIV is this:
    //
    // (Taken from STM32F411xC/E Reference Manual,
    // Section 19.3.4, Equation 1)
    //
    // 16 bit oversample: OVER8 = 0
    // 8 bit oversample:  OVER8 = 1
    //
    // USARTDIV =          (pclk)
    //            ------------------------
    //            8 x (2 - OVER8) x (baud)
    //
    // BUT, the USARTDIV has 4 "fractional" bits, which effectively
    // means that we need to "correct" the equation as follows:
    //
    // USARTDIV =      (pclk) * 16
    //            ------------------------
    //            8 x (2 - OVER8) x (baud)
    //
    // When OVER8 is enabled, we can only use the lowest three
    // fractional bits, so we'll need to shift those last four bits
    // right one bit

    // Calculate correct baudrate divisor on the fly
    if (pclk_freq / 16) >= baud {
        // We have the ability to oversample to 16 bits, take
        // advantage of it.
        //
        // We also add `baud / 2` to the `pclk_freq` to ensure
        // rounding of values to the closest scale, rather than the
        // floored behavior of normal integer division.
        let div = (pclk_freq + (baud / 2)) / baud;
        (false, div)
    } else if (pclk_freq / 8) >= baud {
        // We are close enough to pclk where we can only
        // oversample 8.
        //
        // See note above regarding `baud` and rounding.
        let div = ((pclk_freq * 2) + (baud / 2)) / baud;

        // Ensure the the fractional bits (only 3) are
        // right-aligned.
        let frac = div & 0xF;
        let div = (div & !0xF) | (frac >> 1);
        (true, div)
    } else {
        // TODO Better errors?
        panic!("Invalid config");
    }
}

fn setup_dma_for_rx<S: StreamISR + Stream>(s: &mut S, numtx: u16, ch: DmaChannel, buf: *const u8, peri_addr: u32) {
    unsafe { s.disable() };
    s.set_number_of_transfers(numtx);
    s.set_channel(ch);
    s.set_direction(PeripheralToMemory::direction());
    s.set_memory_address(buf as u32);
    s.set_peripheral_address(peri_addr);
    s.clear_all_flags();
    s.set_priority(dma::config::Priority::High);
    unsafe {
        s.set_memory_size(dma::DmaDataSize::Byte);
        s.set_peripheral_size(dma::DmaDataSize::Byte);
    }
    s.set_memory_increment(true);
    s.set_peripheral_increment(false);

    s.set_double_buffer(false);
    s.set_circular_mode(true);
    s
        .set_fifo_threshold(FifoThreshold::QuarterFull);
    s.set_fifo_enable(false);
    s.set_memory_burst(BurstMode::NoBurst);
    s.set_peripheral_burst(BurstMode::NoBurst);
}

fn setup_dma_for_tx<S: StreamISR + Stream>(s: &mut S, ch: DmaChannel, peri_addr: u32) {
    unsafe {
        s.disable();
    }

    s.set_channel(ch);
    s.set_direction(MemoryToPeripheral::direction());
    s.set_peripheral_address(peri_addr);
    s.clear_all_flags();
    s.set_priority(dma::config::Priority::High);
    unsafe {
        s.set_memory_size(dma::DmaDataSize::Byte);
        s.set_peripheral_size(dma::DmaDataSize::Byte);
    }
    s.set_memory_increment(true);
    s.set_peripheral_increment(false);

    s.set_double_buffer(false);
    s.set_circular_mode(false);
    s.set_fifo_threshold(FifoThreshold::QuarterFull);
    s.set_fifo_enable(false);
    s.set_memory_burst(BurstMode::NoBurst);
    s.set_peripheral_burst(BurstMode::NoBurst);
}

fn usart_clear_idle_interrupt<U: Instance + Ptr<RB = RegisterBlock>>(usart: &U) {
    let _ = usart.sr().read();
    let _ = usart.dr().read();
}

fn usart_get_flags<U: Instance + Ptr<RB = RegisterBlock>>(usart: &U) -> BitFlags<Flag> {
    unsafe {
        Flag::from_bits_unchecked(usart.sr().read().bits())
    }
}

pub const fn assert_max_rx_buf_size_valid(value: usize) {
    assert!(
        value <= 65535,
        "RX buf size cannot be bigger than 65535 bytes"
    );
}

pub struct FullDuplexInitializer<Usart: Instance + Ptr<RB = RegisterBlock> + 'static,
TxStream: Stream + StreamISR + 'static,
RxStream: Stream + StreamISR + 'static,
const DMA_TX_CH: u8,
const DMA_RX_CH: u8> {
    usart: Usart,
    tx_pin: Usart::Tx<PushPull>,
    rx_pin: Usart::Rx<PushPull>,
    tx_stream: TxStream,
    rx_stream: RxStream,
}

impl<
    Usart: Instance + Ptr<RB = RegisterBlock> + 'static,
    TxStream: Stream + StreamISR + 'static,
    RxStream: Stream + StreamISR + 'static,
    const DMA_TX_CH: u8,
    const DMA_RX_CH: u8
> FullDuplexInitializer<Usart, TxStream, RxStream, DMA_TX_CH, DMA_RX_CH>
    where
        ChannelX<DMA_TX_CH>: Channel,
        ChannelX<DMA_RX_CH>: Channel,
        Serial<Usart, u8>: Listen<Event = Event> + ReadFlags<Flag = Flag> + ClearFlags<Flag = CFlag>,
        Usart: DMASet<TxStream, DMA_TX_CH, MemoryToPeripheral>,
        Usart: DMASet<RxStream, DMA_RX_CH, PeripheralToMemory>
{
    pub fn new(
        usart: Usart,
        pins: (
            impl Into<Usart::Tx<PushPull>>,
            impl Into<Usart::Rx<PushPull>>,
        ),
        tx_stream: TxStream,
        rx_stream: RxStream
    ) -> Self {
        Self {
            usart,
            tx_pin: pins.0.into(),
            rx_pin: pins.1.into(),
            tx_stream,
            rx_stream,
        }
    }
}

impl<
    Usart: Instance + Ptr<RB = RegisterBlock> + 'static,
    TxStream: Stream + StreamISR + 'static,
    RxStream: Stream + StreamISR + 'static,
    const DMA_TX_CH: u8,
    const DMA_RX_CH: u8
> UartLineModeInit for FullDuplexInitializer<Usart, TxStream, RxStream, DMA_TX_CH, DMA_RX_CH>
    where
        ChannelX<DMA_TX_CH>: Channel,
        ChannelX<DMA_RX_CH>: Channel,
        Serial<Usart, u8>: Listen<Event = Event> + ReadFlags<Flag = Flag> + ClearFlags<Flag = CFlag>,
        Usart: DMASet<TxStream, DMA_TX_CH, MemoryToPeripheral>,
        Usart: DMASet<RxStream, DMA_RX_CH, PeripheralToMemory>
{
    type Mode = FullDuplex<Usart, TxStream, RxStream, DMA_TX_CH, DMA_RX_CH>;
    fn init(self, rx_buf: &[u8], clocks: &Clocks) -> Self::Mode {
        let mut serial: Serial<Usart, u8> = Serial::new(
            unsafe { mem::transmute_copy(&self.usart) },
            (self.tx_pin, self.rx_pin),
            Config::default()
                .baudrate(2000000.bps())
                .parity_none()
                .stopbits(StopBits::STOP1)
                .wordlength_8()
                .dma(DmaConfig::TxRx),
            clocks,
        )
        .unwrap();

        serial.listen_only(Event::Idle);
        serial.clear_idle_interrupt();

        let (tx, rx) = serial.split();
        let rx_peri_addr: u32 = rx.address();
        let tx_peri_addr: u32 = tx.address();

        // Enable error interrupt generation
        self.usart.cr3().modify(|_, w| w.eie().enabled());

        let mut ret = Self::Mode {
            serial: rx.join(tx),
            tx_stream: self.tx_stream,
            rx_stream: self.rx_stream,
            usart: self.usart,
        };

        setup_dma_for_tx(&mut ret.tx_stream, <ChannelX<DMA_TX_CH> as Channel>::VALUE, tx_peri_addr);
        setup_dma_for_rx(&mut ret.rx_stream, rx_buf.len() as u16, <ChannelX<DMA_RX_CH> as Channel>::VALUE, rx_buf.as_ptr(), rx_peri_addr);
        unsafe {
            ret.rx_stream.enable();
        }
        dev_info!("DMA RX enabled on USART line, full-duplex.");
        ret
    }
}

/// A serial line in full-duplex mode. This mode requires two wires, tx and rx,
/// cross-connected between the current device and the peer.
pub struct FullDuplex<
    Usart: Instance + Ptr<RB = RegisterBlock> + 'static,
    TxStream: Stream + StreamISR + 'static,
    RxStream: Stream + StreamISR + 'static,
    const DMA_TX_CH: u8,
    const DMA_RX_CH: u8
> {
    serial: Serial<Usart>,
    usart: Usart,
    tx_stream: TxStream,
    rx_stream: RxStream,
}

impl<
    Usart: Instance + Ptr<RB = RegisterBlock> + 'static,
    TxStream: Stream + StreamISR + 'static,
    RxStream: Stream + StreamISR + 'static,
    const DMA_TX_CH: u8,
    const DMA_RX_CH: u8
> UartLineMode for FullDuplex<Usart, TxStream, RxStream, DMA_TX_CH, DMA_RX_CH>
where
    ChannelX<DMA_TX_CH>: Channel,
    ChannelX<DMA_RX_CH>: Channel,
    Serial<Usart, u8>: Listen<Event = Event> + ReadFlags<Flag = Flag> + ClearFlags<Flag = CFlag>,
    Usart: DMASet<TxStream, DMA_TX_CH, MemoryToPeripheral>,
    Usart: DMASet<RxStream, DMA_RX_CH, PeripheralToMemory>
{
    type Usart = Usart;
    type DmaTxStream = TxStream;
    type DmaRxStream = RxStream;

    fn usart(&self) -> &Usart {
        &self.usart
    }

    fn dma_tx_stream(&self) -> &Self::DmaTxStream {
        &self.tx_stream
    }

    fn dma_rx_stream(&self) -> &Self::DmaRxStream {
        &self.rx_stream
    }

    fn transfer(&mut self, data_buf: &[u8], tx_buf: &mut [u8]) -> Result<(), BusTransferError> {
        if self.is_tx_busy() {
            return Err(BusTransferError::WouldBlock);
        }

        self.tx_stream.clear_all_flags();
        // TODO Handle buffer overflow error

        self.tx_stream.set_memory_address(tx_buf.as_ptr() as u32);
        self.tx_stream.set_number_of_transfers(data_buf.len() as u16);
        tx_buf[0..data_buf.len()].copy_from_slice(data_buf);
        unsafe {
            self.tx_stream.enable();
        }
        Ok(())

    }

    fn is_tx_busy(&self) -> bool {
        // TODO The line should be considered busy after the tx has ended + the
        // time for the next IDLE signal to be sent, so the peer can detect the
        // previous frame termination.
        self.tx_stream.is_enabled()
    }

    #[inline(always)]
    fn handle_usart_intr(&mut self, _flags: BitFlags<Flag>) {
        // Nothing to do
    }
}

pub struct HalfDuplexInitializer<Usart: Instance + Ptr<RB = RegisterBlock> + 'static,
TxStream: Stream + StreamISR + 'static,
RxStream: Stream + StreamISR + 'static,
const DMA_TX_CH: u8,
const DMA_RX_CH: u8
> {
    usart: Usart,
    txrx_pin: Usart::Tx<PushPull>,
    tx_stream: TxStream,
    rx_stream: RxStream,
}

impl<
    Usart: Instance + Ptr<RB = RegisterBlock> + 'static,
    TxStream: Stream + StreamISR + 'static,
    RxStream: Stream + StreamISR + 'static,
    const DMA_TX_CH: u8,
    const DMA_RX_CH: u8
> HalfDuplexInitializer<Usart, TxStream, RxStream, DMA_TX_CH, DMA_RX_CH>
    where
        ChannelX<DMA_TX_CH>: Channel,
        ChannelX<DMA_RX_CH>: Channel,
        Serial<Usart, u8>: Listen<Event = Event> + ReadFlags<Flag = Flag> + ClearFlags<Flag = CFlag>,
        Usart: DMASet<TxStream, DMA_TX_CH, MemoryToPeripheral>,
        Usart: DMASet<RxStream, DMA_RX_CH, PeripheralToMemory>,
        Usart::Tx<PushPull>: ExtiPin
{
    // TODO We should not require the RX pin for anything I guess, but since I'm
    // for now using the Serial API from the STM32 hal, it requires a pin.
    pub fn new(
        usart: Usart,
        txrx_pin: impl Into<Usart::Tx<PushPull>>,
        tx_stream: TxStream,
        rx_stream: RxStream,
        syscfg: &mut SysCfg,
        exti: &mut EXTI
    ) -> Self {
        let mut txrx_pin = txrx_pin.into();
        txrx_pin.make_interrupt_source(syscfg);

        // FIXME We should make the EXTI interrupt to be disabled while
        // transmission. I didn't want to take full ownership of the EXTI
        // peripheral in this module, but maybe I need.
        txrx_pin.enable_interrupt(exti);
        txrx_pin.trigger_on_edge(exti, Edge::Falling);

        Self {
            usart,
            txrx_pin: txrx_pin.into(),
            tx_stream,
            rx_stream,
        }
    }
}

impl<
    Usart: Instance + Ptr<RB = RegisterBlock> + 'static,
    TxStream: Stream + StreamISR + 'static,
    RxStream: Stream + StreamISR + 'static,
    const DMA_TX_CH: u8,
    const DMA_RX_CH: u8
> UartLineModeInit for HalfDuplexInitializer<Usart, TxStream, RxStream, DMA_TX_CH, DMA_RX_CH>
    where
        ChannelX<DMA_TX_CH>: Channel,
        ChannelX<DMA_RX_CH>: Channel,
        Serial<Usart, u8>: Listen<Event = Event> + ReadFlags<Flag = Flag> + ClearFlags<Flag = CFlag>,
        Usart: DMASet<TxStream, DMA_TX_CH, MemoryToPeripheral>,
        Usart: DMASet<RxStream, DMA_RX_CH, PeripheralToMemory>
{
    type Mode = HalfDuplex<Usart, TxStream, RxStream, DMA_TX_CH, DMA_RX_CH>;
    fn init(mut self, rx_buf: &[u8], clocks: &Clocks) -> Self::Mode {
        self.usart.setup(&UsartConfig {
            half_duplex: true,
            baud_rate: 2000000,
            dma_tx: true,
            dma_rx: true
        }, clocks);

        self.usart.set_interrupt_mask(Event::Idle);
        self.usart.set_error_interrupt_enable(true);

        setup_dma_for_tx(&mut self.tx_stream, <ChannelX<DMA_TX_CH> as Channel>::VALUE, Usart::peri_address());
        setup_dma_for_rx(&mut self.rx_stream, rx_buf.len() as u16, <ChannelX<DMA_RX_CH> as Channel>::VALUE, rx_buf.as_ptr(), Usart::peri_address());

        self.tx_stream.listen_only(DmaEvent::TransferComplete);
        self.usart.tx_set_enabled(true);
        self.usart.rx_set_enabled(true);

        unsafe {
            self.rx_stream.enable();
        }


        let ret = Self::Mode {
            usart: self.usart,
            txrx_pin: self.txrx_pin,
            tx_stream: self.tx_stream,
            rx_stream: self.rx_stream,
            cts: Mutex::new(UnsafeCell::new(true)),
        };

        dev_info!("DMA RX enabled on USART line, half-duplex.");
        ret
    }
}

/// A serial line in half-duplex mode. This mode enables bi-directional
/// communication between the current device and the peer, over a single wire.
/// When this mode is set, the RX pin becomes internally connected to the TX
/// pin, and the latter is the only one used for communication. The driver will
/// take care of toggling the reception on the line while we're transmitting,
/// for preventing receiving an echo message. For transmitting, the driver
/// maintains a software-based "Clear To Send" flag, that is cleared when
/// activity on the line is detected, through a GPIO EXTI pin interrupt, and set
/// again when an idle character is detected. Note however that this line
/// arbitration method is fallible, and may lead to race conditions if both
/// devices decide to start a transmission exactly at the same moment. This
/// driver does not implement anything for preventing that situation, which
/// needs to be manually resolved at transport level.
pub struct HalfDuplex<Usart: Instance + Ptr<RB = RegisterBlock> + 'static,
TxStream: Stream + StreamISR + 'static,
RxStream: Stream + StreamISR + 'static,
const DMA_TX_CH: u8,
const DMA_RX_CH: u8
> {
    usart: Usart,
    txrx_pin: Usart::Tx<PushPull>,
    tx_stream: TxStream,
    rx_stream: RxStream,
    cts: Mutex<UnsafeCell<bool>>
}

impl<
    Usart: Instance + Ptr<RB = RegisterBlock> + 'static,
    TxStream: Stream + StreamISR + 'static,
    RxStream: Stream + StreamISR + 'static,
    const DMA_TX_CH: u8,
    const DMA_RX_CH: u8
> UartLineMode for HalfDuplex<Usart, TxStream, RxStream, DMA_TX_CH, DMA_RX_CH>
where
    ChannelX<DMA_TX_CH>: Channel,
    ChannelX<DMA_RX_CH>: Channel,
    Serial<Usart, u8>: Listen<Event = Event> + ReadFlags<Flag = Flag> + ClearFlags<Flag = CFlag>,
    Usart: DMASet<TxStream, DMA_TX_CH, MemoryToPeripheral>,
    Usart: DMASet<RxStream, DMA_RX_CH, PeripheralToMemory>
{
    type Usart = Usart;
    type DmaTxStream = TxStream;
    type DmaRxStream = RxStream;

    fn usart(&self) -> &Usart {
        &self.usart
    }

    fn dma_tx_stream(&self) -> &Self::DmaTxStream {
        &self.tx_stream
    }

    fn dma_rx_stream(&self) -> &Self::DmaRxStream {
        &self.rx_stream
    }

    fn transfer(&mut self, data_buf: &[u8], tx_buf: &mut [u8]) -> Result<(), BusTransferError> {
        let cts = free_with_muts!(
            cts <- self.cts,
            || {
                let prev = *cts;
                *cts = false;
                return prev;
            }
        );

        if !cts || self.tx_stream.is_enabled() {
            return Err(BusTransferError::WouldBlock);
        }

        self.tx_stream.clear_all_flags();

        // TODO Handle buffer overflow error

        self.tx_stream.set_memory_address(tx_buf.as_ptr() as u32);
        self.tx_stream.set_number_of_transfers(data_buf.len() as u16);
        tx_buf[0..data_buf.len()].copy_from_slice(data_buf);

        self.usart.rx_set_enabled(false);
        unsafe {
            self.tx_stream.enable();
        }
        Ok(())
    }

    fn is_tx_busy(&self) -> bool {
        // TODO The line should be considered busy after the tx has ended + the
        // time for the next IDLE signal to be sent, so the peer can detect the
        // previous frame termination.
        let cts = free_with_muts!(
            cts <- self.cts,
            || {
               *cts
            }
        );

        !cts || self.tx_stream.is_enabled()
    }

    #[inline(always)]
    fn handle_usart_intr(&mut self, flags: BitFlags<Flag>) {
        if flags.contains(Flag::Idle) {
            // Set CTS to true when we detect that the line is idle.
            free_with_muts!(
                cts <- self.cts,
                || {
                    *cts = true;
                }
            );
        } else if flags.contains(Flag::TransmissionComplete) {
            // After the transfer is completed, set CTS and re-enable RX
            free_with_muts!(
                cts <- self.cts,
                || {
                    *cts = true;
                }
            );
            self.usart.rx_set_enabled(true);
            self.usart.set_interrupt_mask(Event::Idle);
        }
    }
}

impl<
    Usart: Instance + Ptr<RB = RegisterBlock> + 'static,
    TxStream: Stream + StreamISR + 'static,
    RxStream: Stream + StreamISR + 'static,
    const DMA_TX_CH: u8,
    const DMA_RX_CH: u8
> HandleExtiIntr for HalfDuplex<Usart, TxStream, RxStream, DMA_TX_CH, DMA_RX_CH>
where
    ChannelX<DMA_TX_CH>: Channel,
    ChannelX<DMA_RX_CH>: Channel,
    Serial<Usart, u8>: Listen<Event = Event> + ReadFlags<Flag = Flag> + ClearFlags<Flag = CFlag>,
    Usart: DMASet<TxStream, DMA_TX_CH, MemoryToPeripheral>,
    Usart: DMASet<RxStream, DMA_RX_CH, PeripheralToMemory>,
    Usart::Tx<PushPull>: ExtiPin
{
    #[inline(always)]
    fn handle_exti_intr(&mut self) {
        self.txrx_pin.clear_interrupt_pending_bit();
        free_with_muts!(
            cts <- self.cts,
            || {
                *cts = false;
            }
        );
    }
}

impl<
    Usart: Instance + Ptr<RB = RegisterBlock> + 'static,
    TxStream: Stream + StreamISR + 'static,
    RxStream: Stream + StreamISR + 'static,
    const DMA_TX_CH: u8,
    const DMA_RX_CH: u8
> HandleDmaIntr for HalfDuplex<Usart, TxStream, RxStream, DMA_TX_CH, DMA_RX_CH>
where
    ChannelX<DMA_TX_CH>: Channel,
    ChannelX<DMA_RX_CH>: Channel,
    Serial<Usart, u8>: Listen<Event = Event> + ReadFlags<Flag = Flag> + ClearFlags<Flag = CFlag>,
    Usart: DMASet<TxStream, DMA_TX_CH, MemoryToPeripheral>,
    Usart: DMASet<RxStream, DMA_RX_CH, PeripheralToMemory>
{
    #[inline(always)]
    fn handle_dma_intr(&mut self) {
        if self.tx_stream.flags().contains(DmaFlag::TransferComplete) {
            // So at this point the DMA has finished writing all the data into
            // the DR register from the USART. However, it is likely that it
            // hasn't been sent out by the hardware. Therefore, to re-enable
            // USART RX, we need to wait until the last byte has been fully sent
            // out. For that, we set here the proper interrupt mask to receive
            // the Transmission Complete interrupt, and there will be where we
            // re-enable RX.
            self.usart.set_interrupt_mask(Event::Idle | Event::TransmissionComplete);
        }

        self.tx_stream.clear_all_flags();
    }
}

pub struct UartDmaRb<
    Mode,
    const DMA_TX_BUF_SZ: usize,
    const DMA_RX_BUF_SZ: usize,
    const DMA_RX_FRAME_CNT: usize
> {
    mode: Mode,
    rx_buf: &'static mut DmaRingBuffer<DMA_RX_BUF_SZ, DMA_RX_FRAME_CNT>,
    tx_buf: &'static mut [u8; DMA_TX_BUF_SZ],
}

impl<
    Mode,
    const DMA_TX_BUF_SZ: usize,
    const DMA_RX_BUF_SZ: usize,
    const DMA_RX_FRAME_CNT: usize
> UartDmaRb<Mode, DMA_TX_BUF_SZ, DMA_RX_BUF_SZ, DMA_RX_FRAME_CNT>
where Mode: UartLineMode
{
    pub fn init<I: UartLineModeInit<Mode = Mode>>(
        mode_initializer: I,
        tx_buf: &'static mut [u8; DMA_TX_BUF_SZ],
        rx_buf: &'static mut DmaRingBuffer<DMA_RX_BUF_SZ, DMA_RX_FRAME_CNT>,
        clocks: &Clocks
    ) -> Self {
        let mode = mode_initializer.init(
            &rx_buf.buf,
            &clocks
        );

        Self {
            mode,
            tx_buf,
            rx_buf
        }
    }


    #[inline(always)]
    pub fn handle_usart_intr(&mut self) {
        let ndt = self.mode.dma_rx_stream().number_of_transfers();

        let flags = usart_get_flags(self.mode.usart());

        let error = flags.intersects(Flag::FramingError | Flag::Noise | Flag::Overrun);

        // As for now, any error produced by these functions indicates
        // a desync between the DMA and the state of the ring buffer,
        // so for now any error that happened here we consider it fatal.

        // TODO instead of panicking, recover from these errors by
        // disabling, resetting and re-enabling DMA transfer.
        if error {
            // Something weird error have happened while reading the
            // current frame. We're completely discarding it.
            free_with_muts!(
                write_side <- self.rx_buf.write_side,
                || {
                    write_side.discard_current_frame(ndt).unwrap();
                }
            );
        } else if flags.contains(Flag::Idle) {
            // We consider the current frame has terminated. We push
            // the final length of the just read frame to the ring
            // buffer and we reset everything for reading the next

            free_with_muts!(
                write_side <- self.rx_buf.write_side,
                || {
                    write_side.close_current_frame(ndt).unwrap();
                }
            );
        }

        self.mode.handle_usart_intr(flags);

        // This also clears all the error flags
        usart_clear_idle_interrupt(self.mode.usart());
    }
}

impl<
    Mode,
    const DMA_TX_BUF_SZ: usize,
    const DMA_RX_BUF_SZ: usize,
    const DMA_RX_FRAME_CNT: usize
> UartDmaRb<Mode, DMA_TX_BUF_SZ, DMA_RX_BUF_SZ, DMA_RX_FRAME_CNT>
where Mode: UartLineMode + HandleExtiIntr
{
    #[inline(always)]
    pub fn handle_exti_intr(&mut self) {
        self.mode.handle_exti_intr();
    }
}

impl<
    Mode,
    const DMA_TX_BUF_SZ: usize,
    const DMA_RX_BUF_SZ: usize,
    const DMA_RX_FRAME_CNT: usize
> UartDmaRb<Mode, DMA_TX_BUF_SZ, DMA_RX_BUF_SZ, DMA_RX_FRAME_CNT>
where Mode: UartLineMode + HandleDmaIntr
{
    #[inline(always)]
    pub fn handle_dma_intr(&mut self) {
        self.mode.handle_dma_intr();
    }
}


impl<
    Mode,
    const DMA_TX_BUF_SZ: usize,
    const DMA_RX_BUF_SZ: usize,
    const DMA_RX_FRAME_CNT: usize
> BusWrite for UartDmaRb<Mode, DMA_TX_BUF_SZ, DMA_RX_BUF_SZ, DMA_RX_FRAME_CNT>
where
    Mode: UartLineMode
{
    #[inline(always)]
    fn transfer(&mut self, buf: &[u8]) -> Result<(), BusTransferError> {
        self.mode.transfer(buf, self.tx_buf)
    }

    fn is_tx_busy(&self) -> bool {
        self.mode.is_tx_busy()
    }
}

impl<
    Mode,
    const DMA_TX_BUF_SZ: usize,
    const DMA_RX_BUF_SZ: usize,
    const DMA_RX_FRAME_CNT: usize
> BusRead for UartDmaRb<Mode, DMA_TX_BUF_SZ, DMA_RX_BUF_SZ, DMA_RX_FRAME_CNT>
where
    Mode: UartLineMode
{
    fn poll_next(&self, buf: &mut [u8]) -> Result<u16, BusPollError> {
        self.rx_buf.poll_next(buf)
    }
}
