//! Local redaction: cover OCR lines that look like secrets with solid boxes.
//! No network. Line boxes are used; engines do not expose word boxes here.

use crate::annot::{Mark, NormRect};
use crate::ocr::OcrResult;
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Email,
    Url,
    Phone,
    Card,
    Address,
    Secret,
}

pub fn marks(result: &OcrResult, width: u32, height: u32) -> Vec<Mark> {
    spans(result, width, height)
        .into_iter()
        .map(|(rect, _)| Mark::Fill { rect, colour: crate::annot::Colour::rgb(0.0, 0.0, 0.0) })
        .collect()
}

pub fn spans(result: &OcrResult, width: u32, height: u32) -> Vec<(NormRect, Kind)> {
    let w = width.max(1) as f64;
    let h = height.max(1) as f64;
    let mut out = Vec::new();
    for line in &result.lines {
        let Some(kind) = classify(&line.text) else { continue };
        let pad = 2.0;
        let x = ((line.x as f64 - pad) / w).clamp(0.0, 1.0);
        let y = ((line.y as f64 - pad) / h).clamp(0.0, 1.0);
        let rw = ((line.w as f64 + pad * 2.0) / w).clamp(0.0, 1.0 - x);
        let rh = ((line.h as f64 + pad * 2.0) / h).clamp(0.0, 1.0 - y);
        if rw < 0.004 || rh < 0.004 {
            continue;
        }
        out.push((NormRect::new(x, y, rw, rh), kind));
    }
    out
}

pub fn classify(text: &str) -> Option<Kind> {
    let rules = rules();
    if rules.email.is_match(text) {
        return Some(Kind::Email);
    }
    if rules.url.is_match(text) {
        return Some(Kind::Url);
    }
    if rules.secret.is_match(text) {
        return Some(Kind::Secret);
    }
    if rules.addr.is_match(text) {
        return Some(Kind::Address);
    }
    if let Some(m) = rules.card.find(text)
        && luhn(m.as_str())
    {
        return Some(Kind::Card);
    }
    if rules.phone.is_match(text) {
        return Some(Kind::Phone);
    }
    None
}

struct Rules {
    email: Regex,
    url: Regex,
    phone: Regex,
    card: Regex,
    addr: Regex,
    secret: Regex,
}

fn rules() -> &'static Rules {
    static RULES: OnceLock<Rules> = OnceLock::new();
    RULES.get_or_init(|| Rules {
        email: Regex::new(r"(?i)[A-Z0-9._%+\-]+@[A-Z0-9.\-]+\.[A-Z]{2,}").expect("email"),
        url: Regex::new(r"(?i)https?://[^\s]+").expect("url"),
        phone: Regex::new(r"(?:\+\d{1,3}[\s.\-]?)?(?:\(?\d{3}\)?[\s.\-]?)\d{3}[\s.\-]?\d{4}").expect("phone"),
        card: Regex::new(r"\b(?:\d{4}[\s\-]?){3}\d{4}\b").expect("card"),
        addr: Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").expect("addr"),
        secret: Regex::new(r"\b(?:sk|pk|ghp|github_pat)_[A-Za-z0-9_\-]{16,}\b").expect("secret"),
    })
}

fn luhn(raw: &str) -> bool {
    let digits: Vec<u32> = raw.chars().filter_map(|c| c.to_digit(10)).collect();
    if digits.len() < 13 || digits.len() > 19 {
        return false;
    }
    let mut sum = 0;
    let mut alt = false;
    for d in digits.iter().rev() {
        let mut n = *d;
        if alt {
            n *= 2;
            if n > 9 {
                n -= 9;
            }
        }
        sum += n;
        alt = !alt;
    }
    sum.is_multiple_of(10)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocr::OcrLine;

    fn result(text: &str) -> OcrResult {
        OcrResult {
            text: text.into(),
            engine: "test".into(),
            lines: vec![OcrLine { text: text.into(), x: 10, y: 20, w: 200, h: 24, conf: 90.0 }],
            fallback_used: false,
        }
    }

    #[test]
    fn email_line_is_redacted() {
        let r = result("write ken@example.com later");
        let marks = marks(&r, 400, 200);
        assert_eq!(marks.len(), 1);
        assert!(matches!(marks[0], Mark::Fill { .. }));
        assert_eq!(classify("ken@example.com"), Some(Kind::Email));
    }

    #[test]
    fn plain_sentence_is_left() {
        assert!(classify("kernel panic on boot").is_none());
        assert!(marks(&result("kernel panic on boot"), 400, 200).is_empty());
    }

    #[test]
    fn luhn_card_matches() {
        assert_eq!(classify("card 4111 1111 1111 1111"), Some(Kind::Card));
        assert!(classify("card 4111 1111 1111 1112").is_none());
    }

    #[test]
    fn url_and_key() {
        assert_eq!(classify("see https://example.com/a"), Some(Kind::Url));
        assert_eq!(classify("token sk_test_abcdefghijklmnopqrst"), Some(Kind::Secret));
    }
}
