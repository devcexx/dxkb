#![allow(incomplete_features)]
#![feature(generic_const_exprs)]

use std::{cell::RefCell, collections::LinkedList, io::ErrorKind, os::fd::AsFd, sync::{Arc, Mutex}, thread::{self, panicking}, time::{Duration, Instant}};

use clap::Parser;
use dxkb_common::{__log::{info, LevelFilter}, bus::{BusPollError, BusRead, BusWrite}, time::Clock, dev_debug, dev_info};
use dxkb_split_link::{SplitBus, SplitLinkTimings};
use nix::{poll::{PollFd, PollFlags, PollTimeout}, sys::time::TimeSpec, time::{clock_gettime, clock_nanosleep, ClockId, ClockNanosleepFlags}};
use serial2::{CharSize, FlowControl, Parity, SerialPort, Settings, StopBits};

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

struct TestingTimings {}
impl SplitLinkTimings for TestingTimings {
    const MAX_LINK_IDLE_TIME: Duration = Duration::from_secs(10);
    const LINK_IDLE_PROBE_INTERVAL_TIME: Duration = Duration::from_secs(3);
    const MAX_SYNC_ACK_WAIT_TIME: Duration = Duration::from_secs(5);
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
    messages: LinkedList<RecvMsg>
}

#[derive(Clone)]
struct SerialBus {
    inner: Arc<Mutex<InnerSerialBus>>,
    serial: Arc<SerialPort>
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
        self.serial.write(buf).unwrap();
        Ok(())
    }

    fn is_tx_busy(&self) -> bool {
        false
    }
}

struct SerialTimings {
    pub nanos_per_char: u64
}

impl SerialTimings {
    pub fn from_config(settings: &Settings) -> SerialTimings {
        let char_size = settings.get_char_size().unwrap();
        let parity = settings.get_parity().unwrap();
        let stop_bits = settings.get_stop_bits().unwrap();
        let baud_rate = settings.get_baud_rate().unwrap();

        let baud_per_char =
            char_size.as_u8() + match parity {
                Parity::None => 0,
                Parity::Odd | Parity::Even => 1,
            } + match stop_bits {
                StopBits::One => 1,
                StopBits::Two => 2,
            };

        return SerialTimings {
            nanos_per_char: (baud_per_char as u64 * 1_000_000_000u64) / baud_rate as u64
        };
    }
}

fn park_nanos(nanos: u64) {
    let duration = TimeSpec::from_duration(Duration::from_nanos(nanos));
    let cur_time = clock_gettime(ClockId::CLOCK_MONOTONIC).unwrap();

    let tgt_time = cur_time + duration;
    while clock_nanosleep(ClockId::CLOCK_MONOTONIC, ClockNanosleepFlags::TIMER_ABSTIME, &tgt_time).is_err() {}
}

fn read_next_frame(port: &SerialPort, timings: &SerialTimings, buf: &mut [u8]) -> usize {
    // Patiently wait until the first byte is available.
    let time_between_read = 3 * timings.nanos_per_char / 2; // Char time * 1.5

    dev_debug!("Waiting for first byte");
    nix::poll::poll(&mut [PollFd::new(port.as_fd(), PollFlags::POLLIN)], PollTimeout::MAX).unwrap();
    dev_debug!("First byte got!");

    // When the first byte is got, wait the time that a char takes to
    // be transmitted + a little bit more. If after that time, there's
    // nothing to read, we consider that the line is IDLE, and we
    // close the current frame. (This is not 100% accurate but should
    // work for testing.)
    let mut read = 0;
    loop {
        let got = match port.read(&mut buf[read..]) {
            Ok(got) => got,
            Err(e) if e.kind() != ErrorKind::TimedOut => {
                panic!("Error reading: {e}");
            },
            _ => 0
        };
        read += got;
        if got == 0 {
            break;
        }
        park_nanos(time_between_read);
    }

    return read;
}

fn main2() {
    // let port_name = "/dev/ttyUSB0";
    // let mut serial_port = SerialPort::open(port_name, |mut settings: Settings| {
    //     settings.set_baud_rate(9600)?;
    //     settings.set_flow_control(FlowControl::None);
    //     Ok(settings)
    // }).unwrap();
    // serial_port.set_read_timeout(Duration::MAX).unwrap();

    // let mut buf = [0u8; 256];
    // loop {
    //     let len = serial_port.read(&mut buf).unwrap();
    //     println!("Data: {:?}", &buf[0..len]);
    // }

//     let mut serial_port = serialport::new("/dev/ttyUSB0", 9600).open().unwrap();
//     serial_port.set_timeout(Duration::MAX).unwrap();
// //    serial_port.set_read_timeout(Duration::MAX).unwrap();

//     let mut buf = [0u8; 256];
//     loop {
//         let len = serial_port.read(&mut buf).unwrap();
//         println!("Data: {:?}", &buf[0..len]);
//     }


}

fn discard_rx_bytes(port: &SerialPort) {
    let mut buf = [0u8; 1];
    loop {
        match port.read(&mut buf) {
            Ok(0) => return,
            Err(e) if e.kind() == ErrorKind::TimedOut => return,
            _ => {}
        }
    }
}

fn main() {
  //  main2();

    env_logger::builder()
        .filter_level(LevelFilter::Info)
        .parse_default_env()
        .init();

    let args = Args::parse();
    let mut port = SerialPort::open(args.port, |mut settings: Settings| {
        settings.set_raw();
        settings.set_baud_rate(args.baud_rate).unwrap();
        settings.set_char_size(CharSize::Bits8);
        settings.set_parity(Parity::None);
        settings.set_stop_bits(StopBits::One);
        settings.set_flow_control(FlowControl::None);
        Ok(settings)
    }).unwrap();
    port.set_read_timeout(Duration::ZERO).unwrap();

    let port = Arc::new(port);
    let timings = SerialTimings::from_config(&port.get_configuration().unwrap());
    // No timeout


    let serial_bus = SerialBus {
        serial: Arc::clone(&port),
        inner: Arc::new(Mutex::new(InnerSerialBus {
            messages: LinkedList::new(),
        })),
    };

    let sb = serial_bus.clone();
    let th_port = Arc::clone(&port);
    std::thread::spawn(move || {
        discard_rx_bytes(&th_port);
        loop {
            let mut buf: [u8; 256] = [0u8; 256];
            let read = read_next_frame(&th_port, &timings, &mut buf);

//             match th_port.read(&mut buf) {
//                 Err(e) if e.kind() == ErrorKind::TimedOut => {
// //                    dev_info!("Timeout");
//                 },
//                 Ok(r) => {
//                     dev_info!("Read {r}");
//                 },
//                 e => {
//                     e.unwrap();
//                     unreachable!();
//                 }
//             };


            if read != 0 {
                let mut guard = sb.inner.lock().unwrap();
                guard.messages.push_back(RecvMsg { buf, len: read });
                drop(guard);
            }
        }
    });

    let mut split_bus: SplitBus<u8, TestingTimings, _, _, 256> = SplitBus::new(serial_bus.clone(), LinuxMonoticClock {});
    dev_info!("Start polling serial");
    loop {
        split_bus.poll(|m| {

        });
        thread::sleep(Duration::from_millis(1));
    }
}
