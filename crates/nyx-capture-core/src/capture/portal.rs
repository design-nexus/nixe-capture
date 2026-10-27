//! XDG Screenshot portal via ashpd. Used when grim is missing, or when prefs ask for it.

use super::{CaptureBackend, CaptureMode, CaptureRequest, Shot};
use crate::geom::Rect;
use anyhow::{Context, Result, bail};
use ashpd::desktop::screenshot::Screenshot;
use image::DynamicImage;

pub struct Portal;

impl CaptureBackend for Portal {
    fn id(&self) -> &'static str {
        "portal"
    }

    fn capture(&self, req: &CaptureRequest) -> Result<Shot> {
        let interactive = matches!(
            req.mode,
            CaptureMode::SelectRegion | CaptureMode::SelectWindow | CaptureMode::Region(_) | CaptureMode::Window(_)
        );
        let Some(shot) = take(interactive)? else {
            bail!("screenshot cancelled");
        };
        Ok(shot)
    }
}

/// Portal dialog. `None` is cancel.
pub fn take(interactive: bool) -> Result<Option<Shot>> {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().context("portal runtime")?;
    match rt.block_on(request(interactive)) {
        Ok(shot) => Ok(Some(shot)),
        Err(err) if cancelled(&err) => Ok(None),
        Err(err) => Err(err),
    }
}

async fn request(interactive: bool) -> Result<Shot> {
    let response = Screenshot::request()
        .interactive(interactive)
        .modal(true)
        .send()
        .await
        .context("screenshot portal")?
        .response()
        .context("screenshot portal")?;
    let path = file_uri_path(response.uri().as_str())?;
    let bytes = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).context("portal image")?;
    let area = fallback_area(&image);
    Shot::from_png(bytes, area, None, "portal")
}

fn file_uri_path(uri: &str) -> Result<std::path::PathBuf> {
    let rest = uri.strip_prefix("file://").ok_or_else(|| anyhow::anyhow!("portal did not return a file"))?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    Ok(std::path::PathBuf::from(rest))
}

fn cancelled(err: &anyhow::Error) -> bool {
    let s = format!("{err:#}").to_ascii_lowercase();
    s.contains("cancel") || s.contains("dismiss")
}

fn fallback_area(image: &DynamicImage) -> Rect {
    Rect::new(0, 0, image.width() as i32, image.height() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_id() {
        assert_eq!(Portal.id(), "portal");
    }

    #[test]
    fn cancel_text_is_detected() {
        assert!(cancelled(&anyhow::anyhow!("User cancelled screenshot")));
        assert!(!cancelled(&anyhow::anyhow!("portal not available")));
    }

    #[test]
    fn file_uri_strips_scheme() {
        assert_eq!(file_uri_path("file:///tmp/shot.png").unwrap(), std::path::PathBuf::from("/tmp/shot.png"));
    }
}
