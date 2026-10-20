//! hyprctl JSON: monitors and clients in layout coordinates.

use super::{Monitor, Window};
use crate::detect;
use crate::geom::Rect;
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::process::{Command, Stdio};

#[derive(Deserialize)]
struct HyprMonitor {
    name: String,
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    scale: f64,
    #[serde(default)]
    transform: i32,
    focused: bool,
}

#[derive(Deserialize)]
struct HyprClient {
    class: String,
    title: String,
    at: [i32; 2],
    size: [i32; 2],
    #[serde(default)]
    hidden: bool,
    #[serde(default)]
    mapped: bool,
    #[serde(rename = "focusHistoryID", default)]
    focus_history_id: i32,
    workspace: HyprWorkspace,
}

#[derive(Deserialize)]
struct HyprWorkspace {
    id: i32,
}

pub fn monitors() -> Result<Vec<Monitor>> {
    let raw = hyprctl(&["monitors", "-j"])?;
    let list: Vec<HyprMonitor> = serde_json::from_str(&raw).context("hyprctl monitors")?;
    Ok(list
        .into_iter()
        .map(|m| {
            let mut w = (m.width as f64 / m.scale).floor() as i32;
            let mut h = (m.height as f64 / m.scale).floor() as i32;
            if m.transform == 1 || m.transform == 3 {
                std::mem::swap(&mut w, &mut h);
            }
            Monitor { name: m.name, geometry: Rect::new(m.x, m.y, w.max(1), h.max(1)), scale: m.scale, focused: m.focused }
        })
        .collect())
}

pub fn windows() -> Result<Vec<Window>> {
    let raw = hyprctl(&["clients", "-j"])?;
    let list: Vec<HyprClient> = serde_json::from_str(&raw).context("hyprctl clients")?;
    Ok(list
        .into_iter()
        .filter(|c| c.mapped && !c.hidden)
        .map(|c| Window {
            class: c.class,
            title: c.title,
            geometry: Rect::new(c.at[0], c.at[1], c.size[0], c.size[1]),
            workspace: c.workspace.id,
            focused: c.focus_history_id == 0,
            hidden: c.hidden,
        })
        .collect())
}

fn hyprctl(args: &[&str]) -> Result<String> {
    if !detect::present("hyprctl") {
        bail!("hyprctl isn't available");
    }
    let out = Command::new("hyprctl").args(args).stdin(Stdio::null()).output().context("hyprctl")?;
    if !out.status.success() {
        bail!("hyprctl failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_json_accounts_for_scale() {
        let raw = r#"[{"name":"eDP-2","width":2560,"height":1600,"x":0,"y":0,"scale":1.6,"transform":0,"focused":true}]"#;
        let list: Vec<HyprMonitor> = serde_json::from_str(raw).unwrap();
        let m = &list[0];
        let w = (m.width as f64 / m.scale).floor() as i32;
        let h = (m.height as f64 / m.scale).floor() as i32;
        assert_eq!((w, h), (1600, 1000));
    }
}
