//! Scrolling capture by wrapping omasnap `--scroll`. No stitcher in this crate.

use crate::detect;
use anyhow::{Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

pub fn available() -> bool {
    detect::present("omasnap")
}

/// Run omasnap's scrolling capture and return the PNG it wrote under `dir`.
pub fn capture(dir: &Path) -> Result<PathBuf> {
    if !available() {
        bail!("scrolling capture needs omasnap");
    }
    fs::create_dir_all(dir)?;
    let before = newest_png(dir);
    let status = Command::new("omasnap").args(["--scroll", "--save"]).env("OMASNAP_SCREENSHOT_DIR", dir).status()?;
    if !status.success() {
        bail!("scroll capture cancelled");
    }
    let after = newest_png(dir).ok_or_else(|| anyhow::anyhow!("omasnap did not write a PNG"))?;
    if let Some(prev) = before
        && after == prev
    {
        bail!("scroll capture cancelled");
    }
    Ok(after)
}

fn newest_png(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(SystemTime, PathBuf)> = None;
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("png") {
            continue;
        }
        let modified = entry.metadata().ok()?.modified().ok()?;
        if best.as_ref().is_none_or(|(t, _)| modified > *t) {
            best = Some((modified, path));
        }
    }
    best.map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_png_picks_latest() {
        let dir = std::env::temp_dir().join(format!("nixe-scroll-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.png");
        let b = dir.join("b.png");
        fs::write(&a, b"a").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(&b, b"b").unwrap();
        assert_eq!(newest_png(&dir), Some(b));
        let _ = fs::remove_dir_all(&dir);
    }
}
