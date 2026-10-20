//! Clipboard via wl-copy.

use anyhow::{Context, Result, bail};
use std::io::Write;
use std::process::{Command, Stdio};

pub fn copy_png(bytes: &[u8]) -> Result<()> {
    pipe(&["wl-copy", "--type", "image/png"], bytes)
}

pub fn copy_text(text: &str) -> Result<()> {
    pipe(&["wl-copy", "--type", "text/plain;charset=utf-8"], text.as_bytes())
}

fn pipe(args: &[&str], bytes: &[u8]) -> Result<()> {
    let (program, rest) = args.split_first().context("empty command")?;
    let mut child = Command::new(program).args(rest).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().with_context(|| format!("{program} isn't available"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(bytes).context("clipboard stdin")?;
    }
    let out = child.wait_with_output().context("clipboard")?;
    if !out.status.success() {
        bail!("{} failed: {}", program, String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}
