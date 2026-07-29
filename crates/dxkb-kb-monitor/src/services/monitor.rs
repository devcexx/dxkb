#![feature(int_from_ascii)]
#![feature(impl_trait_in_bindings)]

use anyhow::{Context, Result};
use futures::StreamExt;
use std::{
    fs::{File, OpenOptions},
    io::{self, Read},
    os::unix::{ffi::OsStrExt, fs::OpenOptionsExt},
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    task::{self, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader, ReadBuf, unix::AsyncFd},
    select,
    sync::{
        Mutex,
        mpsc::Sender,
    },
};
use tokio_udev::AsyncMonitorSocket;
use udev::Device;

use dxkb_kb_monitor_lib::ipc;

// TODO hardcoded for now
const ALLOWED_DEVICE_IDS: &[(u16, u16)] = &[(0x16c0, 0x27db)];

const ALLOWED_INTF_NUM: u8 = 0;
const PANIC_HEADER: &str = "!!!!PANICPANICPANICPANICPANICPANIC!!!!";

pub struct NonBlockingFile(AsyncFd<File>);

impl NonBlockingFile {
    pub fn open(path: &Path) -> io::Result<Self> {
        let f = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)?;

        Ok(Self(AsyncFd::new(f)?))
    }
}

impl AsyncRead for NonBlockingFile {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            let mut guard = match self.0.poll_read_ready(cx) {
                Poll::Ready(Ok(guard)) => guard,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            };

            let unfilled = buf.initialize_unfilled();
            match guard.try_io(|inner| {
                let bytes_read = inner.get_ref().read(unfilled)?;
                Ok(bytes_read)
            }) {
                Ok(Ok(bytes_read)) => {
                    buf.advance(bytes_read);
                    return Poll::Ready(Ok(()));
                }
                Ok(Err(e)) => return Poll::Ready(Err(e)),
                Err(_would_block) => continue,
            }
        }
    }
}

fn new_udev_monitor() -> Result<AsyncMonitorSocket> {
    Ok(AsyncMonitorSocket::new(
        udev::MonitorBuilder::new()?
            .match_subsystem("hidraw")?
            .listen()?,
    )?)
}

fn new_enumerator() -> Result<udev::Enumerator> {
    let mut enumerator = udev::Enumerator::new()?;
    enumerator.match_subsystem("hidraw")?;
    Ok(enumerator)
}

#[derive(Clone)]
struct DeviceMeta {
    devnum: u64,
    name: String,
    node_path: PathBuf,
}

impl From<DeviceMeta> for ipc::IpcDevice {
    fn from(meta: DeviceMeta) -> Self {
        Self {
            devnum: meta.devnum,
            name: meta.name,
            node_path: meta.node_path.to_string_lossy().to_string(),
        }
    }
}

struct DeviceInfo {
    device: Device,
    meta: DeviceMeta,
}

enum DeviceMonitorEvent {
    DeviceCrashed(String),
    DeviceLogLine(String),
}

async fn monitor_device(meta: DeviceMeta, event_bus: Sender<(DeviceMeta, DeviceMonitorEvent)>) {
    if let Err(e) = monitor_device0(meta, event_bus).await {
        println!("Device monitoring exited with error code: {:?}", e);
    }
}

async fn monitor_device0(
    meta: DeviceMeta,
    event_bus: Sender<(DeviceMeta, DeviceMonitorEvent)>,
) -> anyhow::Result<()> {
    let f = BufReader::with_capacity(
        1024,
        NonBlockingFile::open(&meta.node_path)
            .context(format!("Failed to open device: {:?}", meta.node_path))?,
    );
    let mut lines = f.lines();

    while let Some(line) = lines.next_line().await? {
        if line.contains(PANIC_HEADER) {
            let mut msgbuf = String::new();
            loop {
                select! {
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {
                        break;
                    }
                    line = lines.next_line() => {
                        match line {
                            Err(_) | Ok(None)  => {
                                // Device crashed, ignore errors or EOF while reading the msg.
                                break;
                            }
                            Ok(Some(line)) => {
                                if line.contains(PANIC_HEADER) {
                                    // This is the device repeating in a loop the panic message. Stop buffering the panic message here.
                                    break;
                                }
                                msgbuf.push_str(&line);
                                msgbuf.push('\n');
                            }
                        }
                    }
                }
            }
            let _ = event_bus
                .send((meta.clone(), DeviceMonitorEvent::DeviceCrashed(msgbuf)))
                .await;
        } else {
            println!("[{}] {}", meta.name, line);
            let _ = event_bus
                .send((meta.clone(), DeviceMonitorEvent::DeviceLogLine(line)))
                .await;
        }
    }

    Ok(())
}

fn match_device(device: Device) -> Option<DeviceInfo> {
    let id_vendor = device
        .property_value("ID_VENDOR_ID")
        .and_then(|s| u16::from_ascii_radix(s.as_bytes(), 16).ok());
    let id_model = device
        .property_value("ID_MODEL_ID")
        .and_then(|s| u16::from_ascii_radix(&s.as_bytes(), 16).ok());
    let intf_num = device
        .property_value("ID_USB_INTERFACE_NUM")
        .and_then(|s| u8::from_ascii_radix(&s.as_bytes(), 10).ok());

    let Some(id_vendor) = id_vendor else {
        return None;
    };

    let Some(id_model) = id_model else {
        return None;
    };

    let Some(intf_num) = intf_num else {
        return None;
    };

    if !ALLOWED_DEVICE_IDS.contains(&(id_vendor, id_model)) {
        return None;
    }

    if intf_num != ALLOWED_INTF_NUM {
        return None;
    }

    let devnum = device.devnum();
    let monitor_path = device.devnode().map(|x| x.to_path_buf());
    let device_name = device.property_value("ID_MODEL_ENC").map(|s| {
        let enc_name = String::from_utf8_lossy(s.as_bytes());
        if let Ok(device_name) = unicode_escape::decode(&enc_name) {
            device_name
        } else {
            enc_name.into_owned()
        }
    });

    if let Some(monitor_path) = monitor_path
        && let Some(device_name) = device_name
        && let Some(devnum) = devnum
    {
        Some(DeviceInfo {
            device: device,
            meta: DeviceMeta {
                devnum,
                name: device_name,
                node_path: monitor_path,
            },
        })
    } else {
        eprintln!(
            "Device matched criteria, but won't be monitored: {:?}",
            device.devpath()
        );
        None
    }
}

struct AppState {
    monitored_devices: Mutex<Vec<DeviceMeta>>,
    dev_event_tx: Sender<(DeviceMeta, DeviceMonitorEvent)>,
}

impl AppState {
    fn new(dev_event_tx: Sender<(DeviceMeta, DeviceMonitorEvent)>) -> Self {
        Self {
            monitored_devices: Mutex::new(Vec::new()),
            dev_event_tx,
        }
    }

    pub async fn add_monitored_device(&self, device_info: DeviceMeta) {
        let mut devices = self.monitored_devices.lock().await;

        if devices.iter().any(|d| d.devnum == device_info.devnum) {
            panic!("Device is already being monitored: {}", device_info.name);
        } else {
            eprintln!("Started monitoring device: {}", device_info.name);
            devices.push(device_info);
        }
    }

    pub async fn remove_monitored_device(&self, devnum: u64) -> Option<DeviceMeta> {
        let mut devices = self.monitored_devices.lock().await;
        let Some(idx) = devices.iter().position(|d| d.devnum == devnum) else {
            return None;
        };

        let dev = devices.remove(idx);
        eprintln!("Stopped monitoring device: {}", dev.name);
        Some(dev)
    }
}

async fn on_device_plugged(device: Device, state: &Arc<AppState>, server: &mut ipc::IpcServer) {
    if let Some(dev) = match_device(device) {
        let meta = dev.meta.clone();
        state.add_monitored_device(dev.meta).await;
        server
            .broadcast(&ipc::IpcMessageUp::DeviceConnected(ipc::IpcDevice {
                devnum: meta.devnum,
                name: meta.name.clone(),
                node_path: meta.node_path.to_string_lossy().to_string(),
            }))
            .await;
        tokio::spawn(monitor_device(meta, state.dev_event_tx.clone()));
    }
}

async fn on_device_removed(device: &Device, state: &Arc<AppState>, server: &mut ipc::IpcServer) {
    if let Some(devnum) = device.devnum() {
        if let Some(meta) = state.remove_monitored_device(devnum).await {
            server
                .broadcast(&ipc::IpcMessageUp::DeviceDisconnected(ipc::IpcDevice {
                    devnum: meta.devnum,
                    name: meta.name.clone(),
                    node_path: meta.node_path.to_string_lossy().to_string(),
                }))
                .await;
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    main0().await
}

async fn main0() -> anyhow::Result<()> {
    let (dev_event_tx, mut dev_event_rx) =
        tokio::sync::mpsc::channel::<(DeviceMeta, DeviceMonitorEvent)>(1);
    let state = Arc::new(AppState::new(dev_event_tx));
    let mut server = ipc::IpcServer::listen();
    eprintln!("Started listening in daemon socket");

    let mut monitor = new_udev_monitor().context("Failed to create udev monitor")?;

    for dev in new_enumerator()
        .context("Failed to create device enumerator")?
        .scan_devices()?
    {
        on_device_plugged(dev, &state, &mut server).await;
    }

    loop {
        select! {
            ev = monitor.next() => {
                let Some(ev) = ev else {
                    break; // udev channel closed?
                };

                let ev = ev?;
                if let Some(action) = ev.action() {
                    if action == "add" {
                        on_device_plugged(ev.device(), &state, &mut server).await;
                    } else if action == "remove" {
                        on_device_removed(&ev.device(), &state, &mut server).await;
                    }
                }
            }

            server_ev = server.handle_next() => {
                let ev = server_ev?;
                match ev {
                    ipc::IpcServerEvent::ClientConnected(ipc_server_client) => {
                        eprintln!("Client connected: {}", ipc_server_client.id());
                    },
                    ipc::IpcServerEvent::Message(ipc_server_client, s) => {
                        eprintln!("Received message from client {}: {:?}", ipc_server_client.id(), s);
                        match s {
                            ipc::IpcMessageDown::Ping(ping_id) => {
                                eprintln!("Received ping from client {}: {}", ipc_server_client.id(), ping_id);
                                if let Err(e) = ipc_server_client.transfer(&ipc::IpcMessageUp::Pong(ping_id)).await {
                                    eprintln!("Failed to send pong to client {}: {}", ipc_server_client.id(), e);
                                }
                            },
                            ipc::IpcMessageDown::ListDevicesRequest { request_id } => {
                                let devices = state.monitored_devices.lock().await;
                                let resp = ipc::IpcListDevicesResponse {
                                    request_id: request_id,
                                    devices: devices.iter().map(|d| ipc::IpcDevice::from(d.clone())).collect()
                                };
                                drop(devices);
                                eprintln!("Sending list of devices to client {}: {:?}", ipc_server_client.id(), resp);
                                if let Err(e) = ipc_server_client.transfer(&ipc::IpcMessageUp::ListDevicesResponse(resp)).await {
                                    eprintln!("Failed to send list of devices {}: {}", ipc_server_client.id(), e);
                                }
                            }
                        }

                    },
                    ipc::IpcServerEvent::ClientDisconnected(ipc_server_client, err_reason) => {
                        eprintln!("Client disconnected: {}: {}", ipc_server_client.id(), err_reason.map(|e| format!("{}", e)).unwrap_or_else(|| "Disconnected".to_string()));
                    },
                }
            }

            dev_event = dev_event_rx.recv() => {
                let Some(dev_event) = dev_event else {
                    break;
                };

                let (meta, dev_event) = dev_event;

                match dev_event {
                    DeviceMonitorEvent::DeviceCrashed(msg) => {
                        server.broadcast(&ipc::IpcMessageUp::DeviceCrashed { device: meta.into(), msg }).await;
                    },
                    DeviceMonitorEvent::DeviceLogLine(line) => {
                        server.broadcast(&ipc::IpcMessageUp::DeviceLogLine { device: meta.into(), line }).await;
                    }
                }
            }
        }
    }
    Ok(())
}

// ipc::handle_server(async |ev| {
//     println!("{:?}", ev);
//     ()
// }).await;

// Ok(())
