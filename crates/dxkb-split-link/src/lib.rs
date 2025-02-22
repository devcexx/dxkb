/*! This module defines a Split Bus Link, which is a protocol that
 manages a peer to peer communication link. This protocol combines
 link and transport layer capabilities, so it is able to establish a
 communication link and guarantee a reliable communication, ensuring
 packets are transmitted in order, are retransmitted if they are lost,
 and they are received in the same order as it was send. The transport
 messages exchanged are generic and can be easily extended.

 ## Frame format

Each frame has the following format:


```
    8 bits       8 bits
+------------+------------+
|  Preamble  |     Crc    |
+------------+------------+
|    Seq     | Frame Type |
+------------+------------+
|   Frame payload (0-var) |
+------------+------------+
````

Where:
  - `Preamble`: 8 bits that marks the beginning of the frame. It is set
    to the constant `0x99`.

  - `Crc`: a CRC-8 computation of the Seq + Frame Type and Frame
    Payload, using the same parameters as the SMBus CRC 8 calculation
    ([ref](https://reveng.sourceforge.io/crc-catalogue/all.htm#crc.cat.crc-8-smbus)).
  - `Frame Type`: Specifies the current frame type. It can be set to:
     - `LinkProbe`: Frame that is sent at a fixed rate and allows the
       peer to determine if the link is down. If the peer stops
       receiving this frame, it will consider the link to be
       down. When a peer that is down receives this packet, it will
       send a Sync frame for starting setting up the link. For this
       frame, Seq is always set to zero and the Frame payload is not present.

     - `Ack`: Indicates to the peer that the frame with the
*/

#![no_std]
#![allow(incomplete_features)]
#![feature(generic_const_exprs)]

use core::convert::Infallible;
use core::fmt::Debug;
use core::marker::PhantomData;
use core::time::Duration;
use crc::Table;
use dxkb_common::time::{Clock, Instant};
use ringbuffer::{ConstGenericRingBuffer, RingBuffer};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use dxkb_common::bus::{BusPollError, BusRead, BusTransferError, BusWrite};
use dxkb_common::{dev_error, dev_info, dev_warn};

pub trait SplitLinkTimings {
    /// The max time that can happen between successfully received frames
    /// in the link. After that, the link is considered down.
    const MAX_LINK_IDLE_TIME: Duration;

    /// Time between the current split bus instance will send a link probe
    /// frame through the wire, if no other transfer has happened in that
    /// period of time.
    const LINK_IDLE_PROBE_INTERVAL_TIME: Duration;

    /// Max time the link will be kept in Sync state waiting for a Sync
    /// ACK frame to come, before giving up.
    const MAX_SYNC_ACK_WAIT_TIME: Duration;
}

pub struct DefaultSplitLinkTimings {}
impl SplitLinkTimings for DefaultSplitLinkTimings {
    const MAX_LINK_IDLE_TIME: Duration = Duration::from_millis(999999);
    const LINK_IDLE_PROBE_INTERVAL_TIME: Duration = Duration::from_millis(100);
    const MAX_SYNC_ACK_WAIT_TIME: Duration = Duration::from_millis(1000);
}

const SPLIT_BUS_CRC: crc::Crc<u8, Table<1>> = crc::Crc::<u8, Table<1>>::new(&crc::CRC_8_SMBUS);
const FRAME_PRELUDE_BYTE: u8 = 0x99;

fn device_id() -> &'static [u8; 12] {
    let ptr = unsafe {
        // SAFETY: Kinda. According to the datasheet, the UID of the
        // device is 12 bytes long and it is stored starting on the
        // base address below.
        &*(0x1FFF_7A10 as *const [u8; 12])
    };
    return ptr;
}

fn device_id_as_u128(id: &[u8; 12]) -> u128 {
    let mut buf = [0u8; 16];
    buf[0..12].copy_from_slice(id);

    u128::from_le_bytes(buf)
}

#[derive(Serialize, Debug)]
pub enum NoMsg {}

#[repr(C)]
pub struct Frame<M> {
    crc: u8,
    envelope: FrameContentEnvelope<M>,
}

// Store together the fields that are used to calculate the CRC to
// make it easier.
#[derive(Serialize, Deserialize, Debug)]
#[repr(C)]
pub struct FrameContentEnvelope<M> {
    seq: u8,
    content: FrameContent<M>
}

#[derive(Serialize, Deserialize, Debug)]
#[repr(C)]
pub enum FrameContent<M> {
    LinkProbe,
    Ack,
    Nack,
    SyncAck,
    Sync,
    TransportMessage(M)
}

#[derive(Debug)]
pub enum FrameDecodeError {
    PreludeError,
    CrcError,
    SerdeError(ssmarshal::Error)
}

impl<M> FrameContentEnvelope<M> {
    #[inline(always)]
    pub const fn new(seq: u8, content: FrameContent<M>) -> Self {
        Self {
            seq,
            content
        }
    }

    #[inline(always)]
    pub fn crc8(&self) -> u8 {
        let self_bytes: &[u8] = unsafe {
            // SAFETY: slice length matches size of the input
            // envelop. Returned reference has the same lifetime
            // as the envelope lifetime.
            core::slice::from_raw_parts(self as *const FrameContentEnvelope<M> as *const u8, size_of::<FrameContentEnvelope<M>>())
        };
        dev_info!("CRC calculation of {:?}", self_bytes);
        SPLIT_BUS_CRC.checksum(self_bytes)
    }

    #[inline(always)]
    pub fn into_frame(self) -> Frame<M> {
        Frame { crc: self.crc8(), envelope: self }
    }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkStatus {
    /// No activity or probes received from Rx for a while. Considering it down.
    Down,

    /// A Sync packet has been sent and waiting for the first ACK.
    Sync, // TODO Add a timestamp here to indicate when the last
          // status changed, so that we can timeout the link status
          // change.

    /// The link has been sync'ed and is ready to transmit or receive frames.
    Up
}

pub struct SplitBus<Msg, Ts: SplitLinkTimings, B: BusWrite + BusRead, CS: Clock, const TX_QUEUE_LEN: usize> {
    bus: B,
    clock: CS,
    link_status: LinkStatus,
    last_link_status_change_time: Instant,
    last_recv_frame_time: Instant,
    last_sent_frame_time: Instant,

    /// The sequence number that will use the outgoing frames to the
    /// peer.
    tx_seq: u8,

    /// The sequence number that will use the current instance to
    /// receive incoming frames from the peer. The next upcoming
    /// packet must match the value here. Otherwise, it will be
    /// dropped.
    rx_seq: u8,

    /// The queue that contains the frames that are queued to be sent
    /// that are required to control the link. These differs from the
    /// `user_tx_queue` queue in which the latter won't be read until
    /// this one is empty, since control frames always takes
    /// precedence over user transmission requests (e.g an ACK for a
    /// previous received message must be sent before any other
    /// transport frame is sent)
    control_tx_queue: ConstGenericRingBuffer<FrameContentEnvelope<NoMsg>, TX_QUEUE_LEN>,
    user_tx_queue: ConstGenericRingBuffer<Msg, TX_QUEUE_LEN>,
    _msg: PhantomData<Msg>,
    _timings: PhantomData<Ts>
}

pub struct MaxFrameLength<Msg> {
    _msg: PhantomData<Msg>
}

impl<Msg> MaxFrameLength<Msg> where Msg: Sized {
    const MAX_FRAME_LENGTH: usize = size_of::<Frame<Msg>>() + 1; // Max frame length plus the preamble byte.
}

impl<Msg: Debug + DeserializeOwned + Serialize, Ts: SplitLinkTimings, B: BusWrite + BusRead, CS: Clock, const TX_QUEUE_LEN: usize> SplitBus<Msg, Ts, B, CS, TX_QUEUE_LEN> where [(); MaxFrameLength::<Msg>::MAX_FRAME_LENGTH]:, [(); MaxFrameLength::<NoMsg>::MAX_FRAME_LENGTH]: {

    pub const fn new(bus: B, clock: CS) -> Self {
        Self {
            bus,
            clock,
            link_status: LinkStatus::Down,
            last_link_status_change_time: Instant::new(0),
            last_recv_frame_time: Instant::new(0),
            last_sent_frame_time: Instant::new(0),
            tx_seq: 0,
            rx_seq: 0,
            control_tx_queue: ConstGenericRingBuffer::new(),
            user_tx_queue: ConstGenericRingBuffer::new(),
            _msg: PhantomData,
            _timings: PhantomData
        }
    }

    fn crc8(buf: &[u8]) -> u8 {
        dev_info!("Computing CRC for {:x?}", &buf);
        SPLIT_BUS_CRC.checksum(&buf)
    }

    fn decode_frame(buf: &[u8]) -> Result<Frame<Msg>, FrameDecodeError> {
        if buf.len() < 4 { // Min bytes are Preamble, CRC, Seq and Frame Type
            // Reusing the EOF error already defined in ssmarshal.
            return Err(FrameDecodeError::SerdeError(ssmarshal::Error::EndOfStream));
        }

        if buf[0] != FRAME_PRELUDE_BYTE {
            return Err(FrameDecodeError::PreludeError);
        }

        let crc = buf[1];
        let envelope_bytes = &buf[2..];
        let (envelope, read_bytes) = ssmarshal::deserialize::<FrameContentEnvelope<Msg>>(envelope_bytes).map_err(|e| FrameDecodeError::SerdeError(e))?;
        let expected_crc = Self::crc8(&envelope_bytes[0..read_bytes]);

        if crc != expected_crc {
            dev_warn!("Frame CRC mismatch. Dropping frame. Expected CRC: {:x} but got {:x}", expected_crc, crc);
            return Err(FrameDecodeError::CrcError);
        }

        dev_info!("Read bytes: {}", read_bytes);
        dev_info!("Frame bytes: {}", envelope_bytes.len());

        let leftover = envelope_bytes.len() - read_bytes;
        if leftover > 0 {
            dev_warn!("Frame decode left {} bytes unused. Ignoring", leftover);
        }

        Ok(Frame { crc, envelope })
    }

    #[inline(always)]
    fn reset_sequence_numbers(&mut self) {
        self.tx_seq = 0;
        self.rx_seq = 0;
    }

    pub fn bus(&self) -> &B {
        &self.bus
    }

    fn change_link_state(&mut self, new_state: LinkStatus) {
        if self.link_status != new_state {
            dev_info!("Link state changed {:?} => {:?}", self.link_status, new_state);
            self.last_link_status_change_time = self.clock.current_instant();
            self.link_status = new_state;
        }
    }

    fn push_control_frame(&mut self, frame: FrameContentEnvelope<NoMsg>) {
        if self.control_tx_queue.is_full() {
            panic!("No more space in the TX control queue. This MUST NOT happen!");
        }

        self.control_tx_queue.push(frame);
    }

    /// Mutates the current state of the link based on a received frame.
    fn handle_rx_frame<F: FnMut(&Msg) -> ()>(&mut self, frame: &Frame<Msg>, recvf: &mut F) {
        match frame.envelope.content {
            FrameContent::LinkProbe => {
                // There's nothing to do with this frame, unless the
                // link is down. If that case, receiving this frame
                // triggers a link sync.
                if self.link_status == LinkStatus::Down {
                    dev_info!("Received bus probe. Starting link synchronization");
                    self.change_link_state(LinkStatus::Sync);
                    self.push_control_frame(FrameContentEnvelope::new(0, FrameContent::Sync));
                }
            },
            FrameContent::Ack => {
                let seq_diff = (frame.envelope.seq as i32).wrapping_sub(self.tx_seq as i32);
                if seq_diff < 0 {
                    dev_warn!("Received duplicated ACK for seq number {}", frame.envelope.seq);
                } else {
                    if seq_diff != 0 {
                        dev_warn!("TX seq number increased unexpectedly by remoted peer by {}.", seq_diff);
                    }

                    self.tx_seq = frame.envelope.seq.wrapping_add(1);
                }
            },
            FrameContent::Nack => todo!(),
            FrameContent::SyncAck => {
                // This only should be received when our link is in
                // sync state, and confirms that the peer has resetted
                // the seq numbers and it has set its link to Up,
                // becoming ready to receive traffic.
                if self.link_status == LinkStatus::Sync {
                    dev_info!("Received SyncACK");
                    self.change_link_state(LinkStatus::Up);
                    self.reset_sequence_numbers();
                } else {
                    dev_warn!("Received unsolicitated SyncACK");
                }
            },
            FrameContent::Sync => {
                // A sync can happen on any of the different link states:
                //
                // - Down: We were anyway wainting for a sync, and the
                // peer has started it.
                // - Sync: We've recently sent a sync frame. The peer
                // did this too, so we are both trying to sync.
                // - Up: We were supposed both peers to be Up at this
                // point, but it seems that the other maybe have
                // de-sync the status, and it is trying to sync again.

                // Regardless of when we do receive this kind of
                // frame, the outcome must be a transition to the Up
                // state, a reset in the sequence numbers, and a
                // transmission of a SyncAck frame.

                // TODO Should clean the control tx queue when
                // receiving this? I mean, seq numbers are resetted,
                // any Ack or Nack that was pending before to be sent
                // is anyway useless.
                self.change_link_state(LinkStatus::Up);
                self.reset_sequence_numbers();
                self.push_control_frame(FrameContentEnvelope::new(0, FrameContent::SyncAck));
            },
            FrameContent::TransportMessage(ref msg) => {
                if self.link_status == LinkStatus::Up {
                    let seq_diff = (frame.envelope.seq as i32).wrapping_sub(self.rx_seq as i32);
                    if seq_diff < 0 {
                        dev_warn!("Dropping possibly duplicated frame. Expecting seq {} but {} found", self.rx_seq, frame.envelope.seq);
                    } else {
                        if seq_diff != 0 {
                            dev_warn!("RX seq number increased unexpectedly by remoted peer by {}.", seq_diff);
                        }

                        self.rx_seq = frame.envelope.seq.wrapping_add(1);
                        recvf(msg)
                    }
                } else {
                    dev_warn!("Received transport frame when link status was not Up. Silently discarding frame");
                }
            },
        }

    }

    fn do_rx<F: FnMut(&Msg) -> ()>(&mut self, mut recvf: F) {
        // Process received frames.
        let mut rxbuf = [0u8; {MaxFrameLength::<Msg>::MAX_FRAME_LENGTH}];
        while {
            let should_continue = match self.bus.poll_next(&mut rxbuf) {
                Ok(frame_len) => {
                    dev_info!("<-- RX: {:x?}", &rxbuf[0..frame_len as usize]);
                    match Self::decode_frame(&rxbuf[0..frame_len as usize]) {
                        Ok(frame) => {
                            self.last_recv_frame_time = self.clock.current_instant();
                            self.handle_rx_frame(&frame, &mut recvf);
                        },
                        Err(FrameDecodeError::PreludeError) => {
                            dev_warn!("Invalid prelude in frame. Dropping frame");
                        },
                        Err(FrameDecodeError::CrcError) => {
                            dev_warn!("Invalid frame CRC. Dropping frame");
                        },
                        Err(e @ FrameDecodeError::SerdeError(_)) => {
                            dev_warn!("Failed to parse frame: {:?}", e);
                        }
                    }
                    true
                },
                Err(BusPollError::BufferOverflow) => true,
                Err(BusPollError::WouldBlock) => false,
            };

            should_continue
        } {}
    }

    fn encode_frame<M: Serialize>(buf: &mut [u8], frame: &FrameContentEnvelope<M>) -> usize {
        buf[0] = FRAME_PRELUDE_BYTE;
        let encoded_len = ssmarshal::serialize(&mut buf[2..], frame).unwrap();
        buf[1] = Self::crc8(&buf[2..2 + encoded_len]);
        encoded_len + 2
    }

    fn transfer_frame<M: Serialize + Debug>(bus: &mut B, clock: &CS, last_sent_frame_time: &mut Instant, frame: &FrameContentEnvelope<M>) -> Result<(), BusTransferError> where [(); MaxFrameLength::<M>::MAX_FRAME_LENGTH]: {
        let mut txbuf = [0u8; {MaxFrameLength::<M>::MAX_FRAME_LENGTH}];
        let len = Self::encode_frame(&mut txbuf, frame);
        let res = bus.transfer(&mut txbuf[0..len]);
        if matches!(res, Ok(_)) {
            dev_info!("--> TX: {:x?}; {:?}", &txbuf[0..len], &frame);
            // TODO Should I add the estimated time that a frame will
            // take to be transferred through the bus to this, so that
            // I can store the time when I think message should have
            // been received by the peer? Note: this will make the
            // current ticks to go beyond the current time, so any
            // calculation like cycle_count - last_sent_frame_ticks
            // will give invalid results.
            *last_sent_frame_time = clock.current_instant();
        }

        res
    }


    // TODO Maybe the frames sent or queued while the link is down
    // shouldn't be even sent. Thinking about the use-case: UART is
    // disconnected in slave side, you press a lot of keys, then the
    // UART is re-connected, and all those old messages are replied?
    // Doesn't make much sense.
    fn do_tx(&mut self) {
        if !self.bus.is_tx_busy() {
            if let Some(control_frame) = self.control_tx_queue.peek() {
                if let Ok(_) = Self::transfer_frame::<NoMsg>(&mut self.bus, &self.clock, &mut self.last_sent_frame_time, control_frame) {
                    self.control_tx_queue.dequeue();
                }
            }
        }

        // Ensure that the link is up before sending anything (otherwise is a little bit nonsense)


        // if self.link_state && !self.bus.is_tx_busy() {
        //     if let Some(frame) = self.control_tx_queue.peek() {
        //         let _ = Self::transfer_frame(&mut self.bus, &self.clock, &mut self.last_sent_frame_ticks, frame);
        //     }
        // };

        // if self.last_sent_frame_ticks > LINK_IDLE_PROBE_INTERVAL_MS && !self.bus.is_tx_busy() {
        //     // TODO Cache this frame as it is always the same?
        //     let _ = Self::transfer_frame(&mut self.bus, &self.clock, &mut self.last_sent_frame_ticks, &Frame::Probe);
        // }
    }

    fn do_timed_actions(&mut self) {
        if self.last_sent_frame_time.elapsed(&self.clock) >= Ts::LINK_IDLE_PROBE_INTERVAL_TIME {
            self.push_control_frame(FrameContentEnvelope::new(0, FrameContent::LinkProbe));
        }

        if self.link_status == LinkStatus::Sync && self.last_link_status_change_time.elapsed(&self.clock) >= Ts::MAX_SYNC_ACK_WAIT_TIME {
            dev_warn!("Couldn't receive a SyncACK frame in time. Giving up link synchronization");
            self.change_link_state(LinkStatus::Down);
        }

        if self.link_status == LinkStatus::Up && self.last_recv_frame_time.elapsed(&self.clock) >= Ts::MAX_LINK_IDLE_TIME {
            dev_warn!("Link has been idle for so long. Considering it down");
            self.change_link_state(LinkStatus::Down);
        }
    }

    // // TODO Do not expose the frame. This is exclusive for the L2 protocol. Only the final messages should be exposed.
    pub fn poll<F: FnMut(&Msg) -> ()>(&mut self, recvf: F) {
        self.do_rx(recvf);
        self.do_timed_actions();
        self.do_tx();
    }

}
