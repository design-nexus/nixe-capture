//! Well-known locations. Every path honours the XDG overrides.

pub use nixe_core::paths::*;

use std::path::PathBuf;

pub fn library_db() -> PathBuf {
    data_dir().join("capture.sqlite")
}

pub fn captures_dir() -> PathBuf {
    data_dir().join("captures")
}

pub fn thumbs_dir() -> PathBuf {
    cache_dir().join("thumbs")
}

pub fn thumb_file(id: &str) -> PathBuf {
    thumbs_dir().join(format!("{id}.jpg"))
}
