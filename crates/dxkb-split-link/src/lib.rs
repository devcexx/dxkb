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

use core::fmt::Debug;
use core::marker::PhantomData;
use core::time::Duration;
use crc::Table;
use dxkb_common::bus::{BusPollError, BusRead, BusTransferError, BusWrite};
use dxkb_common::time::Clock;
use dxkb_common::{dev_debug, dev_info, dev_trace, dev_warn};
use heapless::Vec;
use ringbuffer::{ConstGenericRingBuffer, RingBuffer};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

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

    /// The time that the current instance will wait for an ACK of a
    /// just sent user message, before re-sending it in case that the
    /// peer hasn't properly received it.
    const MSG_REPLAY_DELAY_TIME: Duration;
}

pub struct DefaultSplitLinkTimings {}

// TODO The default timings are so big for now, they need to be reduced.
impl SplitLinkTimings for DefaultSplitLinkTimings {
    const MAX_LINK_IDLE_TIME: Duration = Duration::from_millis(999999);
    const LINK_IDLE_PROBE_INTERVAL_TIME: Duration = Duration::from_millis(100);
    const MAX_SYNC_ACK_WAIT_TIME: Duration = Duration::from_millis(1000);
    const MSG_REPLAY_DELAY_TIME: Duration = Duration::from_millis(500);
}

pub struct TestingTimings {}
impl SplitLinkTimings for TestingTimings {
    const MAX_LINK_IDLE_TIME: Duration = Duration::from_secs(10);
    const LINK_IDLE_PROBE_INTERVAL_TIME: Duration = Duration::from_secs(3);
    const MAX_SYNC_ACK_WAIT_TIME: Duration = Duration::from_secs(5);
    const MSG_REPLAY_DELAY_TIME: Duration = Duration::from_millis(200);
}

const SPLIT_BUS_CRC: crc::Crc<u8, Table<1>> = crc::Crc::<u8, Table<1>>::new(&crc::CRC_8_SMBUS);
const FRAME_PRELUDE_BYTE: u8 = 0x99;

fn seq_diff(new: u8, cur: u8) -> i8 {
    // Since the sequence number space is limited to 8 bytes, we
    // divide the space in half and we consider that new is:
    // - Greater than cur if cur <= new <= cur + 127
    // - Lower than cur if cur - 127 <= new <= cur
    new.wrapping_sub(cur) as i8
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
    content: FrameContent<M>,
}

#[derive(Serialize, Deserialize, Debug)]
#[repr(C)]
pub enum FrameContent<M> {
    LinkProbe,
    Ack,
    SyncAck,
    Sync,
    TransportMessage(M),
}

#[derive(Debug)]
pub enum FrameDecodeError {
    PreludeError,
    CrcError,
    SerdeError(ssmarshal::Error),
}

#[derive(Debug)]
pub enum TransferError {
    BufferOverflow,
    LinkDown
}

impl<M> FrameContentEnvelope<M> {
    #[inline(always)]
    pub const fn new(seq: u8, content: FrameContent<M>) -> Self {
        Self { seq, content }
    }

    #[inline(always)]
    pub fn crc8(&self) -> u8 {
        let self_bytes: &[u8] = unsafe {
            // SAFETY: slice length matches size of the input
            // envelop. Returned reference has the same lifetime
            // as the envelope lifetime.
            core::slice::from_raw_parts(
                self as *const FrameContentEnvelope<M> as *const u8,
                size_of::<FrameContentEnvelope<M>>(),
            )
        };
        dev_info!("CRC calculation of {:?}", self_bytes);
        SPLIT_BUS_CRC.checksum(self_bytes)
    }

    #[inline(always)]
    pub fn into_frame(self) -> Frame<M> {
        Frame {
            crc: self.crc8(),
            envelope: self,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkStatus {
    /// No activity or probes received from Rx for a while. Considering it down.
    Down,

    /// A Sync packet has been sent and waiting for the first ACK.
    Sync, // TODO Add a timestamp here to indicate when the last
    // status changed, so that we can timeout the link status
    // change.
    /// The link has been sync'ed and is ready to transmit or receive frames.
    Up,
}

pub trait SplitBusLike<Msg: Clone + Debug> {
    fn poll<F: FnMut(&Msg) -> bool>(&mut self, recvf: F);

    #[inline]
    fn poll_max<F: FnMut(&Msg, usize) -> ()>(
        &mut self,
        max_msg_count: usize,
        mut recvf: F,
    ) -> usize {
        let mut received = max_msg_count;
        self.poll(|msg| {
            recvf(msg, received);
            received += 1;
            received < max_msg_count
        });

        received
    }

    #[inline]
    fn poll_into_vec<const MAX: usize>(&mut self, buf: &mut Vec<Msg, MAX>) -> usize {
        self.poll_max(MAX, |msg, _| {
            buf.push(msg.clone()).unwrap();
        })
    }
    fn transfer(&mut self, message: Msg) -> Result<(), TransferError>;
}

pub struct SplitBus<
    Msg,
    Ts: SplitLinkTimings,
    B: BusWrite + BusRead,
    CS: Clock,
    const TX_QUEUE_LEN: usize,
> {
    bus: B,
    clock: CS,
    link_status: LinkStatus,
    last_link_status_change_time: CS::TInstant,
    last_recv_frame_time: CS::TInstant,
    last_sent_frame_time: CS::TInstant,

    /// If not empty, indicates that we haven't received yet an ACK
    /// from the peer indicating that it has received the user message
    /// stored in the head of the `user_tx_queue`. The time value
    /// inside the optional contains the last time the message was
    /// re-sent.
    user_msg_pending_ack_sent_time: Option<CS::TInstant>,

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
    _timings: PhantomData<Ts>,
}

pub struct MaxFrameLength<Msg> {
    _msg: PhantomData<Msg>,
}

impl<Msg> MaxFrameLength<Msg>
where
    Msg: Sized,
{
    const MAX_FRAME_LENGTH: usize = size_of::<Frame<Msg>>() + 1; // Max frame length plus the preamble byte.
}

// TODO Refactor code so that the code is based on the status of the link (like a state machine with actions on each transition and all that). (E.g move the code to an impl LinkStatus).
impl<
    Msg: Clone + Debug + DeserializeOwned + Serialize,
    Ts: SplitLinkTimings,
    B: BusWrite + BusRead,
    CS: Clock,
    const TX_QUEUE_LEN: usize,
> SplitBus<Msg, Ts, B, CS, TX_QUEUE_LEN>
where
    [(); MaxFrameLength::<Msg>::MAX_FRAME_LENGTH]:,
    [(); MaxFrameLength::<NoMsg>::MAX_FRAME_LENGTH]:,
{
    pub fn new(bus: B, clock: CS) -> Self {
        let cur = clock.current_instant();

        Self {
            bus,
            clock,
            link_status: LinkStatus::Down,
            last_link_status_change_time: cur,
            last_recv_frame_time: cur,
            last_sent_frame_time: cur,
            user_msg_pending_ack_sent_time: None,
            tx_seq: 0,
            rx_seq: 0,
            control_tx_queue: ConstGenericRingBuffer::new(),
            user_tx_queue: ConstGenericRingBuffer::new(),
            _msg: PhantomData,
            _timings: PhantomData,
        }
    }

    fn crc8(buf: &[u8]) -> u8 {
        let crc = SPLIT_BUS_CRC.checksum(&buf);
        dev_trace!("CRC for {:x?} = {:x}", &buf, crc);
        return crc;
    }

    fn decode_frame(buf: &[u8]) -> Result<Frame<Msg>, FrameDecodeError> {
        if buf.len() < 4 {
            // Min bytes are Preamble, CRC, Seq and Frame Type
            // Reusing the EOF error already defined in ssmarshal.
            return Err(FrameDecodeError::SerdeError(ssmarshal::Error::EndOfStream));
        }

        if buf[0] != FRAME_PRELUDE_BYTE {
            return Err(FrameDecodeError::PreludeError);
        }

        let crc = buf[1];
        let envelope_bytes = &buf[2..];
        let (envelope, read_bytes) =
            ssmarshal::deserialize::<FrameContentEnvelope<Msg>>(envelope_bytes)
                .map_err(|e| FrameDecodeError::SerdeError(e))?;
        let expected_crc = Self::crc8(&envelope_bytes[0..read_bytes]);

        if crc != expected_crc {
            dev_warn!("Frame CRC mismatch. Dropping frame");
            return Err(FrameDecodeError::CrcError);
        }

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

    pub fn link_status(&self) -> LinkStatus {
        self.link_status
    }

    fn change_link_state(&mut self, new_state: LinkStatus) {
        // For now I'm not validation the state transitions, but the possible status changes should be:
        // - Down -> Sync: When received a link probe and initiated a link synchronization process.
        // - Down -> Up: When we've received a sync message from the peer.
        // - Sync -> Up: When we receive a sync ack from the peer.
        // - Sync -> Down: When the sync process times out.
        // - Up -> Down: When something wrong happens in the link and it goes down.

        if self.link_status != new_state {
            dev_info!(
                "Link state changed {:?} => {:?}",
                self.link_status,
                new_state
            );
            self.last_link_status_change_time = self.clock.current_instant();
            self.link_status = new_state;

            if new_state == LinkStatus::Down {
                // Reset the link status, clearing all the outgoing control and user messages.
                self.last_recv_frame_time = self.clock.current_instant();
                self.last_sent_frame_time = self.clock.current_instant();
                self.user_msg_pending_ack_sent_time = None;
                self.control_tx_queue.clear();
                self.user_tx_queue.clear();
                dev_info!("Link was reset");
            }
        }
    }

    fn push_control_frame(&mut self, frame: FrameContentEnvelope<NoMsg>) {
        if self.control_tx_queue.is_full() {
            panic!("No more space in the TX control queue. This MUST NOT happen!");
        }

        self.control_tx_queue.push(frame);
    }

    /// Mutates the current state of the link based on a received
    /// frame. Returns a value that indicates whether it should
    /// continue polling, depending on the function provided by the
    /// user and the last received frame. The continuing decision will
    /// be always true unless the next frame type is a message, in
    /// which case, the function provided is executed, and its result
    /// is used as result for this function.
    fn handle_rx_frame<F: FnMut(&Msg) -> bool>(
        &mut self,
        frame: &Frame<Msg>,
        recvf: &mut F,
    ) -> bool {
        match frame.envelope.content {
            FrameContent::LinkProbe => {
                // There's nothing to do with this frame, unless the
                // link is down. If that case, receiving this frame
                // triggers a link sync.
                if self.link_status == LinkStatus::Down {
                    dev_debug!("Received bus probe. Starting link synchronization");
                    self.change_link_state(LinkStatus::Sync);
                    self.push_control_frame(FrameContentEnvelope::new(0, FrameContent::Sync));
                }
            }

            FrameContent::Ack => {
                // TODO Maybe I should put here a counter of
                // unexpected ACKs received, and when the number is
                // quite big (20?), give up and set the link down, to force
                // a new resync.
                let diff = seq_diff(frame.envelope.seq, self.tx_seq);
                if diff < 0 {
                    dev_warn!(
                        "Received duplicated ACK for seq number {}",
                        frame.envelope.seq
                    );
                } else {
                    if diff != 0 {
                        dev_warn!(
                            "TX seq number increased unexpectedly by remoted peer by {}.",
                            diff
                        );
                    }

                    if self.user_msg_pending_ack_sent_time.is_some() {
                        dev_debug!(
                            "Successfully ACK'ed message with seq: {}",
                            frame.envelope.seq
                        );
                        self.user_msg_pending_ack_sent_time = None;
                        let _ = self.user_tx_queue.dequeue();
                    } else {
                        dev_debug!("Received an ACK message when there was no in-flight message?");
                    }

                    self.tx_seq = frame.envelope.seq.wrapping_add(1);
                }
            }
            FrameContent::SyncAck => {
                // This only should be received when our link is in
                // sync state, and confirms that the peer has resetted
                // the seq numbers and it has set its link to Up,
                // becoming ready to receive traffic.
                if self.link_status == LinkStatus::Sync {
                    dev_debug!("Received SyncACK");
                    self.change_link_state(LinkStatus::Up);
                    self.reset_sequence_numbers();
                } else {
                    dev_debug!("Received unsolicitated SyncACK. Ignoring.");
                }
            }
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
                // any Ack or that was pending before to be sent
                // is anyway useless.
                self.change_link_state(LinkStatus::Up);
                self.reset_sequence_numbers();
                self.push_control_frame(FrameContentEnvelope::new(0, FrameContent::SyncAck));
            }
            FrameContent::TransportMessage(ref msg) => {
                if self.link_status == LinkStatus::Up {
                    let diff = seq_diff(frame.envelope.seq, self.rx_seq);

                    // For every message received, we need to answer with an ACK:
                    // - If the received seq number is the expected
                    // one, or greater, then we need to send the ACK
                    // to notify that we've received the message.

                    // - If we've received a seq number lower than
                    // expected, is because the peer has re-send a
                    // message that it considered dropped. This could
                    // be because the previous ACK frame that we've
                    // sent hasn't been received properly by the peer,
                    // so it is important to send it again.
                    self.push_control_frame(FrameContentEnvelope {
                        seq: frame.envelope.seq,
                        content: FrameContent::Ack,
                    });

                    if diff < 0 {
                        dev_debug!(
                            "Dropping possibly duplicated frame. Expecting seq {} but {} found",
                            self.rx_seq,
                            frame.envelope.seq
                        );
                    } else {
                        if diff != 0 {
                            dev_debug!(
                                "RX seq number increased unexpectedly by remoted peer by {}.",
                                diff
                            );
                        }

                        self.rx_seq = frame.envelope.seq.wrapping_add(1);

                        return recvf(msg);
                    }
                } else {
                    dev_debug!(
                        "Received transport frame when link status was not Up. Silently discarding frame"
                    );
                }
            }
        }

        true
    }

    fn do_rx<F: FnMut(&Msg) -> bool>(&mut self, mut recvf: F) {
        let mut rxbuf = [0u8; { MaxFrameLength::<Msg>::MAX_FRAME_LENGTH }];
        while {
            let should_continue = match self.bus.poll_next(&mut rxbuf) {
                Ok(frame_len) => {
                    dev_trace!("<-- RX: {:x?}", &rxbuf[0..frame_len as usize]);
                    match Self::decode_frame(&rxbuf[0..frame_len as usize]) {
                        Ok(frame) => {
                            self.last_recv_frame_time = self.clock.current_instant();
                            self.handle_rx_frame(&frame, &mut recvf)
                        }
                        Err(FrameDecodeError::PreludeError) => {
                            dev_debug!("Invalid prelude in frame. Dropping frame");
                            true
                        }
                        Err(FrameDecodeError::CrcError) => {
                            dev_debug!("Invalid frame CRC. Dropping frame");
                            true
                        }
                        Err(e @ FrameDecodeError::SerdeError(_)) => {
                            dev_debug!("Failed to parse frame: {:?}", e);
                            true
                        }
                    }
                }
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

    fn transfer_frame<M: Serialize + Debug>(
        bus: &mut B,
        clock: &CS,
        last_sent_frame_time: &mut CS::TInstant,
        frame: &FrameContentEnvelope<M>,
    ) -> Result<(), BusTransferError>
    where
        [(); MaxFrameLength::<M>::MAX_FRAME_LENGTH]:,
    {
        let mut txbuf = [0u8; { MaxFrameLength::<M>::MAX_FRAME_LENGTH }];
        let len = Self::encode_frame(&mut txbuf, frame);
        let res = bus.transfer(&mut txbuf[0..len]);
        if matches!(res, Ok(_)) {
            dev_trace!("--> TX: {:x?}; {:?}", &txbuf[0..len], &frame);
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

    fn transfer_next_user_msg(&mut self) {
        if let Some(next_frame) = self.user_tx_queue.peek() {
            if let Ok(_) = Self::transfer_frame(
                &mut self.bus,
                &self.clock,
                &mut self.last_sent_frame_time,
                &FrameContentEnvelope {
                    seq: self.tx_seq,
                    content: FrameContent::TransportMessage(next_frame.clone()),
                },
            ) {
                self.user_msg_pending_ack_sent_time = Some(self.clock.current_instant());
            }
        }
    }

    fn do_tx(&mut self) {
        if !self.bus.is_tx_busy() {
            if let Some(control_frame) = self.control_tx_queue.peek() {
                if let Ok(_) = Self::transfer_frame::<NoMsg>(
                    &mut self.bus,
                    &self.clock,
                    &mut self.last_sent_frame_time,
                    control_frame,
                ) {
                    self.control_tx_queue.dequeue();
                }
            }
        }

        // A new user message can only be transferred from here when:
        //  - The link is up.
        //  - The bus is not busy
        //  - No other priority control message is scheduled for transfer.
        //  - There's no message pending to be ACK'ed. Replying that message is part of the job of do_timed_actions.
        if self.link_status == LinkStatus::Up
            && !self.bus.is_tx_busy()
            && self.control_tx_queue.is_empty()
            && self.user_msg_pending_ack_sent_time.is_none()
        {
            self.transfer_next_user_msg();
        }
    }

    fn do_timed_actions(&mut self) {
        if self.clock.elapsed_since(self.last_sent_frame_time) >= Ts::LINK_IDLE_PROBE_INTERVAL_TIME
        {
            // TODO We need to do something about probes:
            // - If we stop sending probes when we receive normal frames, we need to trigger link sync everytime we receive a valid frame.
            // - Either that, or we keep sending link probes indefinitely. I prefer the first option just to save some bandwidth
            self.push_control_frame(FrameContentEnvelope::new(0, FrameContent::LinkProbe));
        }

        if self.link_status == LinkStatus::Sync
            && self.clock.elapsed_since(self.last_link_status_change_time)
                >= Ts::MAX_SYNC_ACK_WAIT_TIME
        {
            dev_warn!("Couldn't receive a SyncACK frame in time. Giving up link synchronization");
            self.change_link_state(LinkStatus::Down);
        }

        if self.link_status == LinkStatus::Up {
            if self.clock.elapsed_since(self.last_recv_frame_time) >= Ts::MAX_LINK_IDLE_TIME {
                dev_warn!("Link has been idle for so long. Considering it down");
                self.change_link_state(LinkStatus::Down);
            } else if let Some(last_replay_time) = self.user_msg_pending_ack_sent_time {
                if !self.bus.is_tx_busy()
                    && self.clock.elapsed_since(last_replay_time) > Ts::MSG_REPLAY_DELAY_TIME
                {
                    dev_debug!("Re-sent user message for which no ACK has been received");
                    self.transfer_next_user_msg();
                }
            }
        }
    }

    pub fn user_tx_queue_len(&self) -> usize {
        self.user_tx_queue.len()
    }
}

impl<
    Msg: Clone + Debug + DeserializeOwned + Serialize,
    Ts: SplitLinkTimings,
    B: BusWrite + BusRead,
    CS: Clock,
    const TX_QUEUE_LEN: usize,
> SplitBusLike<Msg> for SplitBus<Msg, Ts, B, CS, TX_QUEUE_LEN>
where
    [(); MaxFrameLength::<Msg>::MAX_FRAME_LENGTH]:,
    [(); MaxFrameLength::<NoMsg>::MAX_FRAME_LENGTH]:,
{
    fn poll<F: FnMut(&Msg) -> bool>(&mut self, recvf: F) {
        self.do_rx(recvf);
        self.do_timed_actions();
        self.do_tx();
    }

    fn transfer(&mut self, message: Msg) -> Result<(), TransferError> {
        if self.link_status != LinkStatus::Up {
            return Err(TransferError::LinkDown);
        }

        if self.user_tx_queue.is_full() {
            return Err(TransferError::BufferOverflow);
        } else {
            self.user_tx_queue.push(message);
            return Ok(());
        }
    }
}
