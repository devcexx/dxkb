use std::time::Duration;

use anyhow::Context;

#[derive(Debug, Clone, Copy)]
#[repr(i32)]
pub enum NotificationIcon {
    None = -1,
    Warning = 0,
    Info = 1,
    Hint = 2,
    Error = 3,
    Confused = 4,
    Ok = 5,
}

#[derive(Debug, Clone)]
pub enum Color {
    Rgb(u32),
    Default,
}

pub async fn hypr_notify(
    icon: NotificationIcon,
    message: String,
    color: Color,
    duration: Duration,
) -> anyhow::Result<()> {
    tokio::process::Command::new("hyprctl")
        .arg("notify")
        .arg((icon as i32).to_string())
        .arg(duration.as_millis().to_string())
        .arg(match color {
            Color::Rgb(c) => format!("rgb({:06x})", c),
            Color::Default => "0".to_string(),
        })
        .arg(message)
        .spawn()
        .context("Failed to start hyprctl")?;

    Ok(())
}
