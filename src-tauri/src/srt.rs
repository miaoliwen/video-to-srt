use crate::asr::SubtitleSegment;

/// Format a duration in seconds as an SRT timestamp: `HH:MM:SS,mmm`
pub fn format_ts(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "00:00:00,000".into();
    }
    let total_ms = (seconds * 1000.0).round() as u64;
    let h = total_ms / 3_600_000;
    let m = (total_ms % 3_600_000) / 60_000;
    let s = (total_ms % 60_000) / 1000;
    let ms = total_ms % 1000;
    format!("{:02}:{:02}:{:02},{:03}", h, m, s, ms)
}

/// Generate SRT content from segments. If some segments have zero/equal timestamps
/// (single-line fallback), we evenly distribute them across the audio duration.
pub fn build_srt(segments: &[SubtitleSegment], total_duration_secs: f64) -> String {
    let mut fixed: Vec<(f64, f64, String)> = segments
        .iter()
        .map(|s| (s.start, s.end, s.text.clone()))
        .collect();

    let needs_distribute = fixed.iter().any(|(s, e, _)| *e <= *s);
    if needs_distribute {
        let n = fixed.len().max(1) as f64;
        let span = if total_duration_secs > 0.0 { total_duration_secs } else { n * 4.0 };
        let step = span / n;
        let total = fixed.len();
        for (i, item) in fixed.iter_mut().enumerate() {
            let a = i as f64 * step;
            let b = if i + 1 == total { span } else { (i + 1) as f64 * step };
            item.0 = a;
            item.1 = b;
        }
    }

    let mut out = String::new();
    for (i, (start, end, text)) in fixed.iter().enumerate() {
        if i > 0 { out.push('\n'); }
        out.push_str(&format!("{}\n", i + 1));
        out.push_str(&format!("{} --> {}\n", format_ts(*start), format_ts(*end)));
        out.push_str(text.trim());
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srt_preserves_chinese_bytes() {
        let segs = vec![SubtitleSegment {
            start: 0.0,
            end: 2.5,
            text: "你好世界！这是中文测试。".into(),
        }];
        let srt = build_srt(&segs, 0.0);
        // Must remain valid UTF-8 (no replacement chars).
        let bytes = srt.as_bytes();
        let back = std::str::from_utf8(bytes).expect("srt must be valid UTF-8");
        assert!(back.contains("你好世界"));
        assert!(back.contains("00:00:00,000 --> 00:00:02,500"));
        // E2 9C for 你's third byte ensures multi-byte sequences are intact.
        assert!(bytes.windows(3).any(|w| w == [0xE4, 0xBD, 0xA0]));
    }

    #[test]
    fn save_bytes_with_bom_round_trip() {
        let segs = vec![SubtitleSegment {
            start: 1.0,
            end: 3.0,
            text: "中文标点：，。！？".into(),
        }];
        let srt = build_srt(&segs, 0.0);
        let mut bytes: Vec<u8> = Vec::with_capacity(srt.len() + 3);
        bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
        bytes.extend_from_slice(srt.as_bytes());

        // Begins with UTF-8 BOM.
        assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF]);
        // Decoding as UTF-8 with BOM must yield original text (minus BOM).
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.contains("中文标点：，。！？"));
    }
}
