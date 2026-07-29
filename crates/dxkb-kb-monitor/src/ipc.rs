use std::{
    future::poll_fn,
    io,
    marker::PhantomData,
    pin::Pin,
    task::{self, Context, Poll, ready},
    time::Duration,
};

use bitcode::{DecodeOwned, Encode};
use futures::{
    FutureExt, Stream, StreamExt,
    future::join_all,
    stream::SelectAll,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf},
    net::{
        UnixListener, UnixStream,
    },
    select,
};

const MONITOR_SOCKET_PATH: &str = "/tmp/dxkb-monitor.sock";

#[derive(bitcode::Encode, bitcode::Decode, Debug)]
pub struct IpcDevice {
    pub devnum: u64,
    pub name: String,
    pub node_path: String,
}

#[derive(bitcode::Encode, bitcode::Decode, Debug)]
pub struct IpcListDevicesResponse {
    pub request_id: u32,
    pub devices: Vec<IpcDevice>,
}

#[derive(bitcode::Encode, bitcode::Decode, Debug)]
pub enum IpcMessageDown {
    Ping(u32),
    ListDevicesRequest { request_id: u32 },
}

#[derive(bitcode::Encode, bitcode::Decode, Debug)]
pub enum IpcMessageUp {
    Pong(u32),
    ListDevicesResponse(IpcListDevicesResponse),
    DeviceConnected(IpcDevice),
    DeviceDisconnected(IpcDevice),
    DeviceCrashed { device: IpcDevice, msg: String },
    DeviceLogLine { device: IpcDevice, line: String },
}

#[derive(Debug)]
pub enum IpcServerEvent<'a> {
    ClientConnected(&'a mut IpcServerClient),
    Message(&'a mut IpcServerClient, IpcMessageDown),
    ClientDisconnected(&'a mut IpcServerClient, Option<io::Error>),
}

#[derive(Debug)]
pub enum IpcServerReadEvent {
    Message(IpcMessageDown),
    Eof(Option<io::Error>),
}

pub struct IpcServer {
    sock: UnixListener,
    streams: SelectAll<IpcServerClient>,
    next_id: u32,
}

impl IpcServer {
    pub fn listen() -> Self {
        let _ = std::fs::remove_file(MONITOR_SOCKET_PATH);
        let sock = UnixListener::bind(MONITOR_SOCKET_PATH).unwrap();
        Self {
            sock,
            streams: SelectAll::new(),
            next_id: 1,
        }
    }

    pub async fn handle_next<'a>(&'a mut self) -> io::Result<IpcServerEvent<'a>> {
        loop {
            // Futures may be selected multiple times if the polled future haven't generated an actual event
            select! {
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    eprintln!("Heartbeat: {}", self.streams.len());
                },
                accept_res = self.sock.accept() => {
                    match accept_res {
                        Ok((socket, _)) => {
                            let client_id = self.next_id;
                            self.streams.push(IpcServerClient::new(client_id, socket));
                            self.next_id += 1;

                            return Ok(IpcServerEvent::ClientConnected(self.require_server_client_mut(client_id)))
                        },
                        Err(e) => {
                            eprintln!("Error accepting connection: {}", e);
                            return Err(e)
                        }
                    }
                }
                r = poll_fn(|cx| {
                    if self.streams.is_empty() {
                        Poll::Pending // Otherwise, an empty SelectAll will always return Poll::Ready(None)
                    } else {
                        self.streams.poll_next_unpin(cx)
                    }
                }) => {
                    match r {
                        Some((client_id, ev)) => {
                            return Ok(match ev {
                                IpcServerReadEvent::Message(msg) => IpcServerEvent::Message(self.require_server_client_mut(client_id), msg),

                                // EOF or errors while reading clients are not
                                // considered server errors, and are propagated
                                // as a polled event, with further details of
                                // the error inside.
                                IpcServerReadEvent::Eof(e) => IpcServerEvent::ClientDisconnected(self.require_server_client_mut(client_id), e),
                            })
                        },
                        None => {},
                    }
                }
            }
        }
    }

    pub async fn broadcast(&mut self, m: &IpcMessageUp) {
        let mut futs = Vec::with_capacity(self.streams.len());
        for stream in self.streams.iter_mut() {
            futs.push(stream.transfer(&m));
        }

        join_all(futs).await;
    }

    pub fn get_server_client_mut(&mut self, id: u32) -> Option<&mut IpcServerClient> {
        self.streams.iter_mut().find(|c| c.id == id)
    }

    pub fn require_server_client_mut(&mut self, id: u32) -> &mut IpcServerClient {
        self.get_server_client_mut(id).expect(&format!(
            "Client {} was required but it was not available!",
            id
        ))
    }
}

pub async fn transfer(
    message: &IpcMessageDown,
    out: &mut (impl AsyncWrite + Unpin),
) -> io::Result<()> {
    let enc = bitcode::encode(message);
    if enc.len() > u16::MAX as usize {
        panic!("Message too large to send over IPC: {} bytes", enc.len());
    }

    out.write_u16(enc.len() as u16).await?;
    out.write_all(&enc).await?;
    Ok(())
}

pub async fn recv(input: &mut (impl AsyncRead + Unpin)) -> anyhow::Result<IpcMessageDown> {
    let len = input.read_u16().await? as usize;
    let mut inbuf = vec![0; len]; // TODO avoid too many allocations here
    input.read_buf(&mut inbuf).await?;

    Ok(bitcode::decode(&inbuf)?)
}

#[derive(Debug)]
pub struct RxBuf<const N: usize> {
    buf: RxBufInner<N>,

    /// The position of the last unfilled byte in the buffer.
    position: usize,

    /// The number of maximum bytes that the buffer can hold, regardless of its capacity.
    limit: usize,
}

#[derive(Debug)]
pub enum RxBufInner<const N: usize> {
    Static([u8; N]),
    Dynamic(Vec<u8>),
}

impl<const N: usize> RxBuf<N> {
    pub fn new() -> Self {
        RxBuf {
            buf: RxBufInner::Static([0; N]),
            position: 0,
            limit: usize::MAX,
        }
    }

    pub fn require_capacity(&mut self, new_cap: usize) {
        match &mut self.buf {
            RxBufInner::Static(arr) => {
                if new_cap > N {
                    let mut new_buf = vec![0; new_cap];
                    new_buf[..self.position].copy_from_slice(&arr[..self.position]);
                    self.buf = RxBufInner::Dynamic(new_buf);
                }
            }
            RxBufInner::Dynamic(vec) => {
                vec.resize(new_cap, 0);
            }
        }
    }

    pub fn advance(&mut self, n: usize) {
        self.position += n;
        self.position = self.position.min(self.limit);
    }

    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
    }

    pub fn clear_limit(&mut self) {
        self.limit = usize::MAX;
    }

    pub fn clear_position(&mut self) {
        self.position = 0;
    }

    pub fn remaining(&self) -> usize {
        self.limit - self.position
    }

    pub fn reset(&mut self) {
        self.position = 0;
        self.limit = usize::MAX;
        self.buf = RxBufInner::Static([0; N])
    }

    pub fn as_read_buf<'a>(&'a mut self) -> ReadBuf<'a> {
        match &mut self.buf {
            RxBufInner::Static(arr) => {
                let max_len = arr.len();
                ReadBuf::new(&mut arr[self.position..usize::min(self.limit, max_len)])
            }
            RxBufInner::Dynamic(vec) => {
                let max_len = vec.len();
                ReadBuf::new(&mut vec[self.position..usize::min(self.limit, max_len)])
            }
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        match &self.buf {
            RxBufInner::Static(arr) => &arr[..self.position],
            RxBufInner::Dynamic(vec) => &vec[..self.position],
        }
    }
}

#[derive(Debug)]
pub struct IpcSocket<I, O> {
    _msg_type: PhantomData<(I, O)>,
    sock: UnixStream,
    rxbuf: RxBuf<1024>,
    hdr_received: bool,
}

pub type IpcClient = IpcSocket<IpcMessageUp, IpcMessageDown>;

impl<I: DecodeOwned, O: Encode> IpcSocket<I, O> {
    pub fn new(sock: UnixStream) -> Self {
        Self {
            _msg_type: PhantomData,
            sock,
            rxbuf: RxBuf::new(),
            hdr_received: false,
        }
    }

    pub async fn connect() -> io::Result<Self> {
        let sock = UnixStream::connect(MONITOR_SOCKET_PATH).await?;
        Ok(Self::new(sock))
    }

    pub fn socket_mut(&mut self) -> &mut UnixStream {
        &mut self.sock
    }

    pub async fn transfer(&mut self, message: &O) -> io::Result<()> {
        let enc = bitcode::encode(message);
        if enc.len() > u16::MAX as usize {
            panic!("Message too large to send over IPC: {} bytes", enc.len());
        }

        self.sock.write_u16(enc.len() as u16).await?;
        self.sock.write_all(&enc).await?;
        Ok(())
    }
}

impl<I: DecodeOwned + Unpin, O: Unpin> Stream for IpcSocket<I, O> {
    type Item = io::Result<I>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mself = self.get_mut();

        fn do_recv<I: DecodeOwned + Unpin, O: Unpin, R>(
            mself: &mut IpcSocket<I, O>,
            cx: &mut Context<'_>,
            on_complete: impl FnOnce(&mut IpcSocket<I, O>) -> Poll<Option<io::Result<R>>>,
        ) -> Poll<Option<io::Result<R>>> {
            let mut readbuf = mself.rxbuf.as_read_buf();
            let res = ready!(Pin::new(&mut mself.sock).poll_read(cx, &mut readbuf));
            match res {
                Err(e) => return Poll::Ready(Some(Err(e))),
                Ok(_) => {
                    let read_len = readbuf.filled().len();
                    if read_len == 0 {
                        // EOF
                        return Poll::Ready(None);
                    }

                    mself.rxbuf.advance(read_len);
                    if mself.rxbuf.remaining() == 0 {
                        on_complete(mself)
                    } else {
                        Poll::Pending
                    }
                }
            }
        }

        if !mself.hdr_received {
            // Pending to read the packet header
            mself.rxbuf.set_limit(2);
            let res = do_recv(mself, cx, |mself| {
                let size =
                    u16::from_be_bytes([mself.rxbuf.as_slice()[0], mself.rxbuf.as_slice()[1]])
                        as usize;
                mself.rxbuf.clear_position();
                mself.rxbuf.require_capacity(size);
                mself.rxbuf.set_limit(size);
                mself.hdr_received = true;

                Poll::Pending
            });

            if !mself.hdr_received {
                return res;
            }
        }

        if mself.hdr_received {
            return do_recv(mself, cx, |mself| {
                let msg = bitcode::decode(mself.rxbuf.as_slice());
                mself.rxbuf.reset();
                mself.hdr_received = false;
                Poll::Ready(Some(
                    msg.map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
                ))
            });
        }

        return Poll::Pending;
    }
}

#[derive(Debug)]
pub struct IpcServerClient {
    id: u32,
    client: IpcSocket<IpcMessageDown, IpcMessageUp>,
    closed: bool,
}

impl IpcServerClient {
    pub fn new(id: u32, sock: UnixStream) -> Self {
        Self {
            id,
            client: IpcSocket::new(sock),
            closed: false,
        }
    }

    pub fn socket_mut(&mut self) -> &mut UnixStream {
        &mut self.client.sock
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn transfer<'a>(
        &'a mut self,
        message: &'a IpcMessageUp,
    ) -> impl Future<Output = io::Result<()>> {
        self.client.transfer(message)
    }
}

impl Stream for IpcServerClient {
    type Item = (u32, IpcServerReadEvent);

    fn poll_next(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Option<Self::Item>> {
        let mself = self.get_mut();
        if mself.closed {
            return Poll::Ready(None);
        }

        let res = ready!(mself.client.poll_next_unpin(cx));
        Poll::Ready(match res {
            Some(Err(e)) => {
                // Errors while reading the remote stream should be treated as
                // an EOF / Client disconnected. The error details are attached
                // as part of the event.
                mself.closed = true;
                Some((mself.id, IpcServerReadEvent::Eof(Some(e))))
            }
            Some(Ok(msg)) => Some((mself.id, IpcServerReadEvent::Message(msg))),
            None => {
                mself.closed = true;
                Some((mself.id, IpcServerReadEvent::Eof(None)))
            }
        })
    }
}
