#![allow(incomplete_features)]
#![feature(generic_const_exprs)]

mod logger;

use std::{collections::LinkedList, io::{Cursor, ErrorKind, Read, Write}, os::fd::AsFd, path::Path, sync::{Arc, Mutex}, thread::{self}, time::{Duration}};

use clap::Parser;
use dxkb_common::{__log::LevelFilter, bus::{BusPollError, BusRead, BusWrite}, dev_error, dev_info, dev_trace, time::{Clock, TimeDiff}};
use dxkb_split_link::{LinkStatus, SplitBus, SplitLinkTimings};
use flexi_logger::writers::LogWriter;
use nix::{poll::{PollFd, PollFlags, PollTimeout}, sys::time::TimeSpec, time::{clock_gettime, clock_nanosleep, ClockId, ClockNanosleepFlags}};
use rustyline::ExternalPrinter;
use serde::{Deserialize, Serialize};
use serial2::{CharSize, FlowControl, Parity, SerialPort, Settings, StopBits};

#[derive(Clone, Copy)]
struct LinuxMonotonicClockInstant {
    nanos: u64
}

#[derive(Clone)]
struct LinuxMonotonicClock {}
impl Clock for LinuxMonotonicClock {
    type TInstant = LinuxMonotonicClockInstant;

    fn current_instant(&self) -> Self::TInstant {
        let time = nix::time::clock_gettime(ClockId::CLOCK_MONOTONIC).unwrap();

        LinuxMonotonicClockInstant {
            nanos: (time.tv_sec() * 1_000_000_000 + time.tv_nsec()) as u64
        }
    }

    fn diff(&self, newer: Self::TInstant, older: Self::TInstant) -> dxkb_common::time::TimeDiff {
        if newer.nanos >= older.nanos {
            TimeDiff::Forward(Duration::from_nanos(newer.nanos - older.nanos))
        } else {
            TimeDiff::Backward(Duration::from_nanos(older.nanos - newer.nanos))
        }
    }

    fn nanos(&self, instant: Self::TInstant) -> u64 {
        instant.nanos
    }
}

struct TestingTimings {}
impl SplitLinkTimings for TestingTimings {
    const MAX_LINK_IDLE_TIME: Duration = Duration::from_secs(10);
    const LINK_IDLE_PROBE_INTERVAL_TIME: Duration = Duration::from_secs(3);
    const MAX_SYNC_ACK_WAIT_TIME: Duration = Duration::from_secs(5);
    const MSG_REPLAY_DELAY_TIME: Duration = Duration::from_millis(200);
}

#[derive(clap::ValueEnum, Debug, Clone)]
enum TransferMode {
    SendSampleMessage,
    JustReceive,
    SendFile,
    ReceiveFile
}

#[derive(Parser, Debug)]
struct Args {
    port: String,
    baud_rate: u32,

    #[clap(long)]
    transfer_mode: TransferMode,

    #[clap(long)]
    file: Option<String>
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

#[inline(always)]
fn park_nanos(nanos: u64) {
    let duration = TimeSpec::from_duration(Duration::from_nanos(nanos));
    let cur_time = clock_gettime(ClockId::CLOCK_MONOTONIC).unwrap();

    let tgt_time = cur_time + duration;
    while clock_nanosleep(ClockId::CLOCK_MONOTONIC, ClockNanosleepFlags::TIMER_ABSTIME, &tgt_time).is_err() {}
}

fn read_next_frame(port: &SerialPort, timings: &SerialTimings, buf: &mut [u8]) -> usize {
    // Patiently wait until the first byte is available.

    // TODO This just does not work. For now, use only
    // low speeds (9600). I guess that this issue is related to the
    // fact that I'm using a USB to serial converter, that is sending
    // data in packets rather than continuously. Not sure how to fix
    // this for testing though.
    let time_between_read = 3 * timings.nanos_per_char / 2; // Char time * 1.5

    dev_trace!("Waiting for first byte");
    nix::poll::poll(&mut [PollFd::new(port.as_fd(), PollFlags::POLLIN)], PollTimeout::MAX).unwrap();
    dev_trace!("First byte got!");

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

struct RustyLogWriter<P: ExternalPrinter> {
    printer: Arc<Mutex<P>>
}

impl<P: ExternalPrinter + Send> LogWriter for RustyLogWriter<P> {
    fn write(&self, now: &mut flexi_logger::DeferredNow, record: &log::Record) -> std::io::Result<()> {
        let log_line = record.args().to_string();
        self.printer.lock().unwrap().print(format!("{}\n", log_line)).unwrap();
        Ok(())
    }

    fn flush(&self) -> std::io::Result<()> {
        Ok(())
    }
}

// fn main() {

//     dev_info!("HOLA");

//     thread::spawn(move || {
//         let mut i = 0usize;
//         loop {
// //            dev_info!("HOLA");
//             thread::sleep(Duration::from_millis(1000));
//             i += 1;
//         }
//     });

//     loop {
//         let line = rl.readline("> ").unwrap();
//         println!("Line: {line}");
//     }
// }

#[derive(Serialize, Deserialize, Clone, Debug)]
struct TransferChunk {
    chunk: [u8; 32],
    chunk_len: u8,
}

fn transfer_file<B: BusRead + BusWrite, CS: Clock>(file_path: String, link: &mut SplitBus<TransferChunk, TestingTimings, B, CS, 256>) {
    let original_contents = std::fs::read(Path::new(&file_path)).unwrap();
    let total_length = original_contents.len();
    let mut contents = Cursor::new(original_contents);
    let mut next_chunk: Option<TransferChunk> = None;
    let xfer_completed = false;
    let mut last_link_state = LinkStatus::Down;
    let mut last_chunk_len: u8 = 1;
    let mut last_transfer_status_msg = (LinuxMonotonicClock{}).current_instant();
    let mut transferred_length = 0;



    loop {
        link.poll(|m| {});
        if last_chunk_len == 0 && next_chunk.is_none() && link.user_tx_queue_len() == 0 {
            dev_info!("Transfer completed");
            break;
        }

        if link.user_tx_queue_len() == 0 {
            let next_transfer = match &next_chunk {
                Some(chunk) => chunk,
                None => {
                    let mut buf = [0u8; 32];
                    let count = contents.read(&mut buf).unwrap();
                    next_chunk.insert(TransferChunk { chunk: buf, chunk_len: count as u8 })

                },
            };
            last_chunk_len = next_transfer.chunk_len;

            if last_link_state == LinkStatus::Up && link.link_status() != LinkStatus::Up {
                dev_error!("Link crashed while doing the transfer. Operation aborted!");
                break;
            }

            last_link_state = link.link_status();

            if link.link_status() == LinkStatus::Up {
                if let Ok(_) = link.transfer(next_transfer.clone()) {
                    transferred_length += next_transfer.chunk_len as usize;
                    next_chunk = None;
                    if (LinuxMonotonicClock{}).elapsed_since(last_transfer_status_msg) > Duration::from_millis(500) {
                        last_transfer_status_msg = LinuxMonotonicClock{}.current_instant();
                        dev_info!("Total transferred: {} / {}", transferred_length, total_length);
                    }
                }
            }


        }

    }
}

fn receive_file<B: BusRead + BusWrite, CS: Clock>(file_path: String, link: &mut SplitBus<TransferChunk, TestingTimings, B, CS, 256>) {
    let mut file = std::fs::File::create(file_path).unwrap();
    let completed_time: Option<CS::TInstant> = None;

    loop {
        link.poll(|m| {
            if completed_time.is_none() {
                if m.chunk_len == 0 {
                    todo!()
//                    completed_time = Some(LinuxMonotonicClock{}.current_instant());
                } else {
                    file.write(&m.chunk[0..m.chunk_len as usize]).unwrap();
                }
            }

        });

        if let Some(completed_time) = completed_time {
            // Wait a couple of seconds before finishing to let the lasts ACK to be sent.
            // if completed_time.elapsed(&LinuxMonotonicClock{}) > Duration::from_secs(2) {
            //     dev_info!("Transfer completed");
            //     break;
            // }

            todo!()

        }
    }

    drop(file);
}


fn main() {
    //  main2();



    env_logger::builder()
        .filter_level(LevelFilter::Trace)
        .parse_default_env()
        .init();

    let args = Args::parse();

    let is_sender = args.port.contains("ttyUSB0");
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
            if read != 0 {
                let mut guard = sb.inner.lock().unwrap();
                guard.messages.push_back(RecvMsg { buf, len: read });
                drop(guard);
            }
        }
    });

    let clock = LinuxMonotonicClock {};

    let mut last_sent_message = clock.current_instant();


    // match args.transfer_mode {
    //     TransferMode::SendSampleMessage => todo!(),
    //     TransferMode::JustReceive => todo!(),
    //     TransferMode::SendFile => {
    //         let mut split_bus: SplitBus<TransferChunk, TestingTimings, _, _, 256> = SplitBus::new(serial_bus.clone(), clock.clone());
    //         transfer_file(args.file.unwrap(), &mut split_bus);
    //     },
    //     TransferMode::ReceiveFile => {
    //         let mut split_bus: SplitBus<TransferChunk, TestingTimings, _, _, 256> = SplitBus::new(serial_bus.clone(), clock.clone());
    //         receive_file(args.file.unwrap(), &mut split_bus);
    //     },
    // }


    let mut split_bus: SplitBus<u8, TestingTimings, _, _, 256> = SplitBus::new(serial_bus.clone(), clock.clone());
    let mut next = 0;
    dev_info!("Start polling serial");
    loop {
        split_bus.poll(|m| {
            dev_info!("Received message: {}", *m)
        });

        if is_sender && split_bus.link_status() == LinkStatus::Up && clock.elapsed_since(last_sent_message) > Duration::from_millis(50) {
            dev_info!("Sample message was sent");
            split_bus.transfer(next).unwrap();
            next += 1;
            last_sent_message = clock.current_instant();
        }
        thread::sleep(Duration::from_millis(1));
    }
}
