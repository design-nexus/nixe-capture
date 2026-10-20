//! Editable annotation objects. The original capture file is never overwritten.

mod raster;

pub use raster::{draw_marks, preview, render};

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct NormRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl NormRect {
    pub const FULL: NormRect = NormRect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };

    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    pub fn from_corners(a: (f64, f64), b: (f64, f64)) -> Self {
        Self::new(a.0.min(b.0), a.1.min(b.1), (a.0 - b.0).abs(), (a.1 - b.1).abs())
    }

    pub fn right(self) -> f64 {
        self.x + self.w
    }

    pub fn bottom(self) -> f64 {
        self.y + self.h
    }

    pub fn contains(self, px: f64, py: f64) -> bool {
        px >= self.x && py >= self.y && px <= self.right() && py <= self.bottom()
    }

    pub fn scaled(self, sx: f64, sy: f64) -> Self {
        Self::new(self.x * sx, self.y * sy, self.w * sx, self.h * sy)
    }

    pub fn is_full(self) -> bool {
        self.w >= 0.9999 && self.h >= 0.9999 && self.x <= 0.0001 && self.y <= 0.0001
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Colour {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

impl Colour {
    pub const fn rgb(r: f64, g: f64, b: f64) -> Colour {
        Colour { r, g, b }
    }

    pub fn contrast(self) -> Colour {
        if 0.299 * self.r + 0.587 * self.g + 0.114 * self.b > 0.55 {
            Colour::rgb(0.0, 0.0, 0.0)
        } else {
            Colour::rgb(1.0, 1.0, 1.0)
        }
    }

    pub fn css(self) -> String {
        let b = |v: f64| (v * 255.0).round().clamp(0.0, 255.0) as u8;
        format!("#{:02x}{:02x}{:02x}", b(self.r), b(self.g), b(self.b))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Mark {
    Stroke { points: Vec<(f64, f64)>, colour: Colour, size: f64, highlighter: bool },
    Line { from: (f64, f64), to: (f64, f64), colour: Colour, size: f64 },
    Arrow { from: (f64, f64), to: (f64, f64), colour: Colour, size: f64 },
    Rect { rect: NormRect, colour: Colour, size: f64 },
    Ellipse { rect: NormRect, colour: Colour, size: f64 },
    Text { at: (f64, f64), text: String, colour: Colour, size: f64 },
    Callout { at: (f64, f64), n: u32, colour: Colour },
    Pixelate { rect: NormRect, size: f64 },
    Blur { rect: NormRect, size: f64 },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EditState {
    pub crop: Option<NormRect>,
    pub marks: Vec<Mark>,
}

impl EditState {
    pub fn next_callout(&self) -> u32 {
        self.marks
            .iter()
            .filter_map(|m| match m {
                Mark::Callout { n, .. } => Some(*n),
                _ => None,
            })
            .max()
            .unwrap_or(0)
            + 1
    }

    pub fn set_crop(&mut self, crop: Option<NormRect>) {
        self.crop = crop.filter(|c| !c.is_full());
    }

    pub fn is_edited(&self) -> bool {
        self.crop.is_some() || !self.marks.is_empty()
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    pub fn load(path: &Path) -> anyhow::Result<Self> {
        Ok(Self::from_json(&std::fs::read_to_string(path)?)?)
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        crate::atomic_write(path, self.to_json()?.as_bytes())
    }
}

/// Undo and redo: a snapshot after each finished action.
pub struct History {
    states: Vec<EditState>,
    at: usize,
}

impl Default for History {
    fn default() -> Self {
        Self { states: vec![EditState::default()], at: 0 }
    }
}

impl History {
    pub fn from_state(state: EditState) -> Self {
        Self { states: vec![state], at: 0 }
    }

    pub fn current(&self) -> &EditState {
        &self.states[self.at]
    }

    pub fn push(&mut self, state: EditState) {
        if state == *self.current() {
            return;
        }
        self.states.truncate(self.at + 1);
        self.states.push(state);
        self.at += 1;
    }

    pub fn can_undo(&self) -> bool {
        self.at > 0
    }

    pub fn can_redo(&self) -> bool {
        self.at + 1 < self.states.len()
    }

    pub fn undo(&mut self) -> &EditState {
        self.at = self.at.saturating_sub(1);
        self.current()
    }

    pub fn redo(&mut self) -> &EditState {
        if self.can_redo() {
            self.at += 1;
        }
        self.current()
    }
}

pub fn sidecar_path(png: &Path) -> std::path::PathBuf {
    png.with_extension("ann.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_round_trip_and_callout_numbers() {
        let mut st = EditState::default();
        assert_eq!(st.next_callout(), 1);
        st.marks.push(Mark::Callout { at: (0.2, 0.3), n: 1, colour: Colour::rgb(1.0, 0.0, 0.0) });
        assert_eq!(st.next_callout(), 2);
        let json = st.to_json().unwrap();
        let back = EditState::from_json(&json).unwrap();
        assert_eq!(back, st);
    }

    #[test]
    fn history_undoes_and_drops_the_future() {
        let mut h = History::default();
        let mut a = EditState::default();
        a.marks.push(Mark::Line { from: (0.0, 0.0), to: (1.0, 1.0), colour: Colour::rgb(1.0, 0.0, 0.0), size: 0.01 });
        h.push(a.clone());
        let mut b = a.clone();
        b.crop = Some(NormRect::new(0.1, 0.1, 0.5, 0.5));
        h.push(b);
        assert!(h.current().crop.is_some());
        assert!(h.undo().crop.is_none());
        assert!(h.can_redo());
        h.redo();
        h.undo();
        let mut c = a.clone();
        c.marks.clear();
        h.push(c);
        assert!(!h.can_redo());
    }

    #[test]
    fn sidecar_sits_beside_the_png() {
        assert_eq!(sidecar_path(Path::new("/tmp/abc.png")), Path::new("/tmp/abc.ann.json"));
    }
}
