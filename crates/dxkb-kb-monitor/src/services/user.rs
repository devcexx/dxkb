use std::time::{Duration, Instant};

use anyhow::Result;

use dxkb_kb_monitor_lib::{ipc, notify};
use futures::StreamExt;
use log::{info, warn};
use rand::random;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    main0().await
}

async fn main0() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .init();

    info!("Connecting to daemon...");
    let mut client = ipc::IpcClient::connect().await?;
    let mut devices = Vec::new();

    let ping_req_id: u32 = random();
    let mut pending_devices_resp: Option<(u32, Instant)> = Some((ping_req_id, Instant::now()));

    client
        .transfer(&ipc::IpcMessageDown::ListDevicesRequest {
            request_id: ping_req_id,
        })
        .await?;

    eprintln!("Connected to daemon! Waiting for events...");
    loop {
        let Some(msg) = client.next().await else {
            break;
        };

        let msg = msg?;

        match msg {
            ipc::IpcMessageUp::Pong(_) => {
                info!("Pong received!");
            }
            ipc::IpcMessageUp::ListDevicesResponse(resp) => {
                if let Some((req_id, req_time)) = &pending_devices_resp
                    && *req_id == resp.request_id
                    && req_time.elapsed() < Duration::from_secs(2)
                {
                    devices = resp.devices;
                    pending_devices_resp.take();
                    info!("Refreshed list of connected devices");
                } else {
                    warn!("Received unsolicitated ListDevicesResponse");
                }
            }
            ipc::IpcMessageUp::DeviceConnected(ipc_device) => {
                if devices.iter().any(|d| d.devnum == ipc_device.devnum) {
                    warn!(
                        "Received duplicate device connected event for device {} (devnum: {})",
                        ipc_device.name, ipc_device.devnum
                    );
                } else {
                    info!(
                        "Device connected: {} (devnum: {})",
                        ipc_device.name, ipc_device.devnum
                    );
                    let _ = notify::hypr::hypr_notify(
                        notify::hypr::NotificationIcon::Info,
                        format!("DXKB device ({}) connected!", ipc_device.name),
                        notify::hypr::Color::Default,
                        Duration::from_secs(5),
                    )
                    .await;
                    devices.push(ipc_device);
                }
            }
            ipc::IpcMessageUp::DeviceDisconnected(ipc_device) => {
                if let Some(idx) = devices.iter().position(|d| d.devnum == ipc_device.devnum) {
                    info!(
                        "Device disconnected: {} (devnum: {})",
                        ipc_device.name, ipc_device.devnum
                    );
                    devices.remove(idx);
                    let _ = notify::hypr::hypr_notify(
                        notify::hypr::NotificationIcon::Warning,
                        format!("DXKB device ({}) disconnected!", ipc_device.name),
                        notify::hypr::Color::Default,
                        Duration::from_secs(5),
                    )
                    .await;
                } else {
                    warn!(
                        "Received device disconnected event for unknown device {} (devnum: {})",
                        ipc_device.name, ipc_device.devnum
                    );
                }
            }
            ipc::IpcMessageUp::DeviceCrashed { device, msg } => {
                let _ = notify::hypr::hypr_notify(
                    notify::hypr::NotificationIcon::Warning,
                    format!("DXKB device ({}) has crashed!:\n{}", device.name, msg),
                    notify::hypr::Color::Default,
                    Duration::from_secs(5),
                )
                .await;
            }
            ipc::IpcMessageUp::DeviceLogLine { device: _, line: _ } => {}
        }
    }
    eprintln!("Bye!");
    Ok(())
}
