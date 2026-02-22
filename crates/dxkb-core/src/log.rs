use core::cell::RefCell;

use cortex_m::interrupt::{free, Mutex};
use dxkb_common::util::RingBuffer;
use log::{Level, Log, SetLoggerError};
use core::fmt::Write;

struct WriterRingBuffer<const SIZE: usize> {
    buf: RingBuffer<u8, SIZE>
}

impl<const SIZE: usize> core::fmt::Write for WriterRingBuffer<SIZE> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        if bytes.len() > self.buf.capacity() {
            self.buf.write(&bytes[..self.buf.capacity()]);
        } else {
            self.buf.write(bytes);
        }
        Ok(())
    }
}

pub struct RingBufferLogger<const SIZE: usize> {
    log_level: Level,
    buf: Mutex<RefCell<WriterRingBuffer<SIZE>>>
}

impl<const SIZE: usize> RingBufferLogger<SIZE> {
    pub const fn new(level: Level, buf: RingBuffer<u8, SIZE>) -> Self {
        Self {
            log_level: level,
            buf: Mutex::new(RefCell::new(WriterRingBuffer {
                buf
            }))
        }
    }

    pub fn install(logger: &'static Self) -> Result<(), SetLoggerError> {
        free(|_| log::set_logger(logger))?;
        log::set_max_level(logger.log_level.to_level_filter());
        Ok(())
    }

    pub fn drop_pending_bytes(&self, count: usize) {
        free(|cs| {
            self.buf.borrow(cs).borrow_mut().buf.drop_first(count);
        })
    }

    pub fn read_pending_bytes<const N: usize>(&self, buf: &mut [u8; N]) -> usize {
        free(|cs| {
            self.buf.borrow(cs).borrow().buf.read(buf)
        })
    }
}

impl<const SIZE: usize> Log for RingBufferLogger<SIZE> {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() >= self.log_level
    }

    fn log(&self, record: &log::Record) {
        free(|cs| {
            write!(
                self.buf.borrow(cs).borrow_mut(),
                "{:<5} [{}] {}\n",
                record.level(),
                record.target(),
                record.args()
            ).unwrap();
        });
    }

    fn flush(&self) {

    }
}
