#![allow(incomplete_features)]
#![feature(generic_const_exprs)]

use std::{cell::RefCell, collections::LinkedList, io::ErrorKind, sync::{Arc, Mutex}, thread::{self, panicking}, time::{Duration, Instant}};

use clap::Parser;
use dxkb_common::{__log::{info, LevelFilter}, bus::{BusPollError, BusRead, BusWrite}, clock::Clock, dev_info};
use dxkb_split_link::SplitBus;
use nix::time::ClockId;
use serialport::{DataBits, Parity, SerialPort, SerialPortBuilder, StopBits};

struct LinuxMonoticClock {}
impl Clock for LinuxMonoticClock {
    fn current_cycle(&self) -> u32 {
        0
    }

    fn current_nanos(&self) -> u64 {
        let time = nix::time::clock_gettime(ClockId::CLOCK_MONOTONIC).unwrap();
        (time.tv_sec() * 1_000_000_000 + time.tv_nsec()) as u64
    }
}

#[derive(Parser, Debug)]
struct Args {
    port: String,
    baud_rate: u32
}

struct RecvMsg {
    buf: [u8; 256],
    len: usize
}

struct InnerSerialBus {
    serial: Box<dyn SerialPort>,
    messages: LinkedList<RecvMsg>
}

#[derive(Clone)]
struct SerialBus {
    inner: Arc<Mutex<InnerSerialBus>>
}

impl BusRead for SerialBus {
    fn poll_next(&self, buf: &mut [u8]) -> Result<u16, dxkb_common::bus::BusPollError> {
        let mut guard = self.inner.lock().unwrap();
        let msg = guard.messages.pop_front();
        drop(guard);

        match msg {
            Some(msg) => {
                if msg.len > buf.len() {
                    Err(BusPollError::BufferOverflow)
                } else {
                    buf[0..msg.len].copy_from_slice(&msg.buf[0..msg.len]);
                    Ok(msg.len as u16)
                }
            },
            None => Err(BusPollError::WouldBlock),
        }
    }
}

impl BusWrite for SerialBus {
    fn transfer(&mut self, buf: &[u8]) -> Result<(), dxkb_common::bus::BusTransferError> {
        let mut guard = self.inner.lock().unwrap();
        guard.serial.write(buf).unwrap();
        Ok(())
    }

    fn is_tx_busy(&self) -> bool {
        false
    }
}



fn main() {
    env_logger::builder()
        .filter_level(LevelFilter::Info)
        .parse_default_env()
        .init();

    let args = Args::parse();
    let mut port = serialport::new(args.port, args.baud_rate)
        .parity(Parity::None)
        .data_bits(DataBits::Eight)
        .stop_bits(StopBits::One)
        .timeout(Duration::from_millis(10))
        .open().unwrap();

    let serial_bus = SerialBus {
        inner: Arc::new(Mutex::new(InnerSerialBus {
            serial: port,
            messages: LinkedList::new(),
        })),
    };

    let sb = serial_bus.clone();
    std::thread::spawn(move || {
        loop {
            let mut buf = [0u8; 256];
            let mut len = 0;
            loop {
                let mut guard = sb.inner.lock().unwrap();
                let read = match guard.serial.read(&mut buf[len..]) {
                    Ok(r) => r,
                    Err(e) => if e.kind() == ErrorKind::TimedOut {
                        if len > 0 {
                            break;
                        } else {
                            0
                        }
                    } else {
                        panic!("Error: {:?}", e);
                    }
                };
                len += read;
                drop(guard);
            }
            let mut guard = sb.inner.lock().unwrap();
            guard.messages.push_back(RecvMsg { buf, len });
            drop(guard);
            dev_info!("Read message: {:?}", &buf[0..len]);
        }
    });

    let mut split_bus: SplitBus<u8, _, _, 256> = SplitBus::new(serial_bus.clone(), LinuxMonoticClock {});
    dev_info!("Start polling serial");
    loop {
        split_bus.poll(|m| {

        });
        thread::sleep(Duration::from_millis(1));
    }
}
