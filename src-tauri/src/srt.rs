use crate::asr::SubtitleSegment;

/// Format a duration in seconds as an SRT timestamp: `HH:MM:SS,mmm`
/// Clamps to [0, 100 hours) to prevent overflow; values >= 100h show as 99:59:59,999.
const MAX_SRT_SECS: f64 = 100.0 * 3600.0 - 0.001;
pub fn format_ts(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "00:00:00,000".into();
    }
    let seconds = seconds.min(MAX_SRT_SECS);
    let total_ms = (seconds * 1000.0).round() as u64;
    let h = total_ms / 3_600_000;
    let m = (total_ms % 3_600_000) / 60_000;
    let s = (total_ms % 60_000) / 1000;
    let ms = total_ms % 1000;
    format!("{:02}:{:02}:{:02},{:03}", h, m, s, ms)
}

/// Generate SRT content from segments. If some segments have zero/equal timestamps
/// (single-line fallback), we evenly distribute them across the audio duration.
///
/// # Timestamp validation
/// - Segments with NaN/Inf start or end: replaced with 0
/// - Segments with end <= start (invalid order): individually redistributed
/// - Negative start times: clamped to 0
/// - Upper bound: 99:59:59,999 (prevents saturation overflow)
pub fn build_srt(segments: &[SubtitleSegment], total_duration_secs: f64) -> String {
    let mut fixed: Vec<(f64, f64, String)> = segments
        .iter()
        .map(|s| {
            let mut start = s.start;
            let mut end = s.end;
            // Guard against NaN/Inf
            if !start.is_finite() || start < 0.0 { start = 0.0; }
            if !end.is_finite() || end < 0.0 { end = 0.0; }
            // Clamp negative to 0
            if start < 0.0 { start = 0.0; }
            if end < 0.0 { end = 0.0; }
            // Guard: if NaN comparison gave true (NaN <= x is false), we already handled above
            (start, end, s.text.clone())
        })
        .collect();

    // Per-segment redistribution: only touches items with end <= start
    let total = fixed.len();
    let n = fixed.len().max(1) as f64;
    let span = if total_duration_secs > 0.0 && total_duration_secs.is_finite() {
        total_duration_secs
    } else {
        n * 4.0
    };
    let step = span / n;
    for (i, item) in fixed.iter_mut().enumerate() {
        if item.1 <= item.0 {
            let a = i as f64 * step;
            let b = if i + 1 == total { span } else { (i + 1) as f64 * step };
            item.0 = a;
            item.1 = b;
        }
    }

    // Enforce monotonicity: ensure end >= start + 0.1s minimum gap
    let min_gap = 0.1;
    for i in 1..fixed.len() {
        if fixed[i].0 < fixed[i - 1].1 + min_gap {
            fixed[i].0 = fixed[i - 1].1 + min_gap;
        }
        if fixed[i].1 <= fixed[i].0 {
            fixed[i].1 = fixed[i].0 + min_gap;
        }
    }

    let mut out = String::with_capacity(segments.len() * 80);
    for (i, (start, end, text)) in fixed.iter().enumerate() {
        if i > 0 { out.push_str("\r\n"); }
        out.push_str(&format!("{}\r\n", i + 1));
        out.push_str(&format!("{} --> {}\r\n", format_ts(*start), format_ts(*end)));
        out.push_str(text.trim());
        out.push('\r');
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

    #[test]
    fn empty_segments_produce_empty_srt() {
        assert_eq!(build_srt(&[], 0.0), "");
        assert_eq!(build_srt(&[], 120.0), "");
    }

    #[test]
    fn nan_or_infinite_timestamps_clamp_to_zero() {
        // NaN start + Inf end both become 0.0 → invalid pair → redistributed
        // across the full span.
        let segs = vec![SubtitleSegment {
            start: f64::NAN,
            end: f64::INFINITY,
            text: "x".into(),
        }];
        let srt = build_srt(&segs, 8.0);
        assert!(srt.contains("00:00:00,000 --> 00:00:08,000"));

        // Positive Inf alone is clamped; the pair then stays valid.
        let segs = vec![SubtitleSegment {
            start: f64::INFINITY,
            end: 5.0,
            text: "y".into(),
        }];
        let srt = build_srt(&segs, 0.0);
        assert!(srt.contains("00:00:00,000 --> 00:00:05,000"));
    }

    #[test]
    fn negative_timestamps_clamp_to_zero() {
        let segs = vec![SubtitleSegment {
            start: -3.0,
            end: -1.0,
            text: "neg".into(),
        }];
        // -3/-1 → 0/0 → invalid pair → redistributed over the span.
        let srt = build_srt(&segs, 10.0);
        assert!(srt.contains("00:00:00,000 --> 00:00:10,000"));
    }

    #[test]
    fn only_invalid_segments_are_redistributed() {
        let segs = vec![
            SubtitleSegment { start: 0.0, end: 2.0, text: "valid".into() },
            SubtitleSegment { start: 100.0, end: 50.0, text: "broken".into() },
        ];
        let srt = build_srt(&segs, 10.0);
        // The valid cue keeps its own timestamps; only the broken one is
        // redistributed into the [5,10) slice of the span.
        assert!(srt.contains("00:00:00,000 --> 00:00:02,000"));
        assert!(srt.contains("00:00:05,000 --> 00:00:10,000"));
        assert!(!srt.contains("00:01:40,000"));
    }

    #[test]
    fn monotonicity_keeps_positive_gap_between_cues() {
        let segs = vec![
            SubtitleSegment { start: 0.0, end: 5.0, text: "a".into() },
            SubtitleSegment { start: 0.0, end: 0.0, text: "b".into() },
        ];
        let srt = build_srt(&segs, 10.0);
        // i=1 redistributed to start 5.0, but that collides with cue 1's end
        // (5.0) → monotonicity pushes it to 5.1.
        assert!(srt.contains("00:00:05,100 --> 00:00:10,000"));
    }

    #[test]
    fn format_ts_clamps_at_100_hours() {
        assert_eq!(format_ts(0.0), "00:00:00,000");
        assert_eq!(format_ts(2.5), "00:00:02,500");
        assert_eq!(format_ts(3600.0), "01:00:00,000");
        // 100h and beyond saturate at the 99:59:59,999 cap.
        assert_eq!(format_ts(100.0 * 3600.0), "99:59:59,999");
        assert_eq!(format_ts(1e12), "99:59:59,999");
        // Non-finite / negative inputs never reach formatting.
        assert_eq!(format_ts(f64::NAN), "00:00:00,000");
        assert_eq!(format_ts(f64::NEG_INFINITY), "00:00:00,000");
        assert_eq!(format_ts(-1.0), "00:00:00,000");
    }

    #[test]
    fn srt_uses_crlf_line_endings() {
        let segs = vec![
            SubtitleSegment { start: 0.0, end: 1.0, text: "a".into() },
            SubtitleSegment { start: 1.1, end: 2.0, text: "b".into() },
        ];
        let srt = build_srt(&segs, 2.0);
        // Every \n must be preceded by \r (CRLF, Windows-friendly).
        for (i, ch) in srt.char_indices() {
            if ch == '\n' {
                assert!(i > 0 && srt.as_bytes()[i - 1] == b'\r', "lone \\n at byte {i}");
            }
        }
        // Cue separator: text ends with CRLF, then a blank CRLF line before
        // the next index.
        assert!(srt.contains("a\r\n\r\n2\r\n"));
    }
}
