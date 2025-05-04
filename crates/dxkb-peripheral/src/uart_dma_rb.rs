//! Defines methods for handling a UART line in which the Rx line is
//! handled by a DMA stream that writes the incoming data forever into
//! a ring buffer.

use core::cell::UnsafeCell;
use core::fmt::Debug;
use core::mem;
use cortex_m::interrupt::Mutex;
use dxkb_common::{
    bus::{BusPollError, BusRead, BusTransferError, BusWrite},
    dev_info, dev_warn,
};
use ringbuffer::{ConstGenericRingBuffer, RingBuffer};
use stm32f4xx_hal::Ptr;
use stm32f4xx_hal::dma::MemoryToPeripheral;
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
    ($($ident:ident <- $ref:expr),*, |$cs:ident| $block:block) => {
        cortex_m::interrupt::free(|cs| {
            $(
               let $ident = unsafe { &mut *($ref).borrow(cs).get() };
            )*

            let $cs = cs;

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

            |_cs| {
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
                    dev_info!("Polled discard frame: {}", section.len);
                }
                RbSectionKind::Frame => {
                    dev_info!("Polled frame: {}", section.len);
                    self.copy_next_read_buffer_bytes(read_off, &mut buf[0..section.len as usize]);
                    return Ok(section.len);
                }
            }
        }
    }
}

// TODO Why do i need so much statics just for the rx_buf to have a static ref?
pub struct UartDmaRb<
    Usart: Instance + Ptr<RB = RegisterBlock> + 'static,
    TxStream: Stream + StreamISR + 'static,
    RxStream: Stream + StreamISR + 'static,
    const TX_CH: u8,
    const RX_CH: u8,
    const MAX_RX_BUF: usize,
    const MAX_RB_FRAG_COUNT: usize,
> {
    serial: Serial<Usart, u8>,
    tx_stream: TxStream,
    rx_stream: RxStream,
    rx_buf: &'static mut DmaRingBuffer<MAX_RX_BUF, MAX_RB_FRAG_COUNT>,
    tx_buf: &'static mut [u8; MAX_RX_BUF], // TODO Create a MAX_TX_BUF
}

impl<
    Usart,
    TxStream,
    RxStream,
    const TX_CH: u8,
    const RX_CH: u8,
    const MAX_RX_BUF: usize,
    const MAX_RB_FRAG_COUNT: usize,
> UartDmaRb<Usart, TxStream, RxStream, TX_CH, RX_CH, MAX_RX_BUF, MAX_RB_FRAG_COUNT>
where
    Usart: Instance<RB = RegisterBlock>,
    TxStream: StreamISR + Stream,
    RxStream: StreamISR + Stream,
    ChannelX<TX_CH>: Channel,
    ChannelX<RX_CH>: Channel,
    Serial<Usart, u8>: Listen<Event = Event> + ReadFlags<Flag = Flag> + ClearFlags<Flag = CFlag>,
    Usart: DMASet<TxStream, TX_CH, MemoryToPeripheral>,
    Usart: DMASet<RxStream, RX_CH, PeripheralToMemory>,
{
    pub const fn assert_max_rx_buf_size_valid(value: usize) {
        assert!(
            value <= 65535,
            "Rx buf size cannot be bigger than 65535 bytes"
        );
    }

    pub fn init(
        usart: Usart,
        pins: (
            impl Into<Usart::Tx<PushPull>>,
            impl Into<Usart::Rx<PushPull>>,
        ),
        tx_stream: TxStream,
        rx_stream: RxStream,
        tx_buf: &'static mut [u8; MAX_RX_BUF],
        rx_buf: &'static mut DmaRingBuffer<MAX_RX_BUF, MAX_RB_FRAG_COUNT>,
        clocks: &Clocks,
    ) -> Self {
        const { Self::assert_max_rx_buf_size_valid(MAX_RX_BUF) };

        let mut serial: Serial<Usart, u8> = Serial::new(
            unsafe { mem::transmute_copy(&usart) },
            pins,
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

        let (tx, rx) = serial.split();
        let rx_peri_addr: u32 = rx.address();
        let tx_peri_addr: u32 = tx.address();

        // Enable error interrupt generation
        usart.cr3().modify(|_, w| w.eie().enabled());

        let mut ret = Self {
            serial: rx.join(tx),
            tx_stream,
            rx_stream,
            rx_buf,
            tx_buf,
        };
        ret.config_tx_stream(tx_peri_addr);
        ret.init_rx_dma_xfer(rx_peri_addr);
        dev_info!("DMA RX enabled on USART line");
        ret
    }

    fn init_rx_dma_xfer(&mut self, peri_addr: u32) {
        unsafe { self.rx_stream.disable() };
        self.rx_stream.set_number_of_transfers(MAX_RX_BUF as u16);
        self.rx_stream
            .set_channel(<ChannelX<RX_CH> as Channel>::VALUE);
        self.rx_stream
            .set_direction(PeripheralToMemory::direction());
        self.rx_stream
            .set_memory_address(self.rx_buf.buf.as_ptr() as u32);
        self.rx_stream.set_peripheral_address(peri_addr);
        self.rx_stream.clear_all_flags();
        self.rx_stream.set_priority(dma::config::Priority::High);
        unsafe {
            self.rx_stream.set_memory_size(dma::DmaDataSize::Byte);
            self.rx_stream.set_peripheral_size(dma::DmaDataSize::Byte);
        }
        self.rx_stream.set_memory_increment(true);
        self.rx_stream.set_peripheral_increment(false);

        self.rx_stream.set_double_buffer(false);
        self.rx_stream.set_circular_mode(true);
        self.rx_stream
            .set_fifo_threshold(FifoThreshold::QuarterFull);
        self.rx_stream.set_fifo_enable(false);
        self.rx_stream.set_memory_burst(BurstMode::NoBurst);
        self.rx_stream.set_peripheral_burst(BurstMode::NoBurst);

        unsafe {
            self.rx_stream.enable();
        }
    }

    fn config_tx_stream(&mut self, peri_addr: u32) {
        unsafe {
            self.tx_stream.disable();
        }

        self.tx_stream
            .set_channel(<ChannelX<TX_CH> as Channel>::VALUE);
        self.tx_stream
            .set_direction(MemoryToPeripheral::direction());
        self.tx_stream.set_peripheral_address(peri_addr);
        self.tx_stream.clear_all_flags();
        self.tx_stream.set_priority(dma::config::Priority::High);
        unsafe {
            self.tx_stream.set_memory_size(dma::DmaDataSize::Byte);
            self.tx_stream.set_peripheral_size(dma::DmaDataSize::Byte);
        }
        self.tx_stream.set_memory_increment(true);
        self.tx_stream.set_peripheral_increment(false);

        self.tx_stream.set_double_buffer(false);
        self.tx_stream.set_circular_mode(false);
        self.tx_stream
            .set_fifo_threshold(FifoThreshold::QuarterFull);
        self.tx_stream.set_fifo_enable(false);
        self.tx_stream.set_memory_burst(BurstMode::NoBurst);
        self.tx_stream.set_peripheral_burst(BurstMode::NoBurst);
    }

    #[inline(always)]
    pub fn handle_usart_intr(&self) {
        let ndt = self.rx_stream.number_of_transfers();

        let flags = self.serial.flags();
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
                |_cs| {
                    write_side.discard_current_frame(ndt).unwrap();
                }
            );
        } else if flags.contains(Flag::Idle) {
            // We consider the current frame has terminated. We push
            // the final length of the just read frame to the ring
            // buffer and we reset everything for reading the next

            free_with_muts!(
                write_side <- self.rx_buf.write_side,
                |_cs| {
                    write_side.close_current_frame(ndt).unwrap();
                }
            );
        }

        // This also clears all the error flags
        self.serial.clear_idle_interrupt();
    }
}

impl<
    Usart,
    TxStream,
    RxStream,
    const TX_CH: u8,
    const RX_CH: u8,
    const MAX_RX_BUF: usize,
    const MAX_RB_FRAG_COUNT: usize,
> BusWrite for UartDmaRb<Usart, TxStream, RxStream, TX_CH, RX_CH, MAX_RX_BUF, MAX_RB_FRAG_COUNT>
where
    Usart: Instance<RB = RegisterBlock>,
    TxStream: StreamISR + Stream,
    RxStream: StreamISR + Stream,
    ChannelX<TX_CH>: Channel,
    ChannelX<RX_CH>: Channel,
    Serial<Usart, u8>: Listen<Event = Event> + ReadFlags<Flag = Flag> + ClearFlags<Flag = CFlag>,
    Usart: DMASet<TxStream, TX_CH, MemoryToPeripheral>,
    Usart: DMASet<RxStream, RX_CH, PeripheralToMemory>,
{
    fn transfer(&mut self, buf: &[u8]) -> Result<(), BusTransferError> {
        if self.tx_stream.is_enabled() {
            return Err(BusTransferError::WouldBlock);
        }

        self.tx_stream.clear_all_flags();
        // TODO Handle buffer overflow error

        self.tx_stream
            .set_memory_address(self.tx_buf.as_ptr() as u32);
        self.tx_stream.set_number_of_transfers(buf.len() as u16);
        self.tx_buf[0..buf.len()].copy_from_slice(buf);
        unsafe {
            self.tx_stream.enable();
        }
        Ok(())
    }

    fn is_tx_busy(&self) -> bool {
        self.tx_stream.is_enabled()
    }
}

impl<
    Usart,
    TxStream,
    RxStream,
    const TX_CH: u8,
    const RX_CH: u8,
    const MAX_RX_BUF: usize,
    const MAX_RB_FRAG_COUNT: usize,
> BusRead for UartDmaRb<Usart, TxStream, RxStream, TX_CH, RX_CH, MAX_RX_BUF, MAX_RB_FRAG_COUNT>
where
    Usart: Instance<RB = RegisterBlock>,
    TxStream: StreamISR + Stream,
    RxStream: StreamISR + Stream,
    ChannelX<TX_CH>: Channel,
    ChannelX<RX_CH>: Channel,
    Serial<Usart, u8>: Listen<Event = Event> + ReadFlags<Flag = Flag> + ClearFlags<Flag = CFlag>,
    Usart: DMASet<TxStream, TX_CH, MemoryToPeripheral>,
    Usart: DMASet<RxStream, RX_CH, PeripheralToMemory>,
{
    fn poll_next(&self, buf: &mut [u8]) -> Result<u16, BusPollError> {
        self.rx_buf.poll_next(buf)
    }
}
