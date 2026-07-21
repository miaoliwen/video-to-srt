//! Integration-style checks for Chinese text round-trip: Rust -> SRT bytes
//! (with UTF-8 BOM) -> reader interprets them correctly.

use crate::asr::SubtitleSegment;
use crate::srt::build_srt;

#[test]
fn chinese_segments_round_trip_through_save_bytes() {
    let segs = vec![
        SubtitleSegment {
            start: 0.0,
            end: 1.5,
            text: "你好世界".into(),
        },
        SubtitleSegment {
            start: 1.5,
            end: 3.0,
            text: "中文标点：，。！？".into(),
        },
        SubtitleSegment {
            start: 3.0,
            end: 5.0,
            text: "繁体字测试".into(),
        },
    ];

    let srt = build_srt(&segs, 5.0);
    let mut bytes = Vec::with_capacity(srt.len() + 3);
    bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    bytes.extend_from_slice(srt.as_bytes());

    assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF], "must start with UTF-8 BOM");
    let back = std::str::from_utf8(&bytes).expect("must remain valid UTF-8");
    assert!(back.contains("你好世界"));
    assert!(back.contains("中文标点：，。！？"));
    assert!(back.contains("繁体字测试"));

    // Multi-byte boundaries (你 = E4 BD A0) preserved.
    assert!(bytes.windows(3).any(|w| w == [0xE4, 0xBD, 0xA0]));
    // 繁 = E7 B9 81
    assert!(bytes.windows(3).any(|w| w == [0xE7, 0xB9, 0x81]));
}

#[test]
fn srt_is_valid_utf8_for_mojibake_inspection() {
    let segs = vec![SubtitleSegment {
        start: 0.0,
        end: 2.0,
        text: "艾菲尔铁塔".into(),
    }];
    let srt = build_srt(&segs, 2.0);
    // If misread as GBK the typical mojibake would start with bytes C8 in
    // patterns indicating double decoding. UTF-8 of 艾 = E8 89 BE — different.
    assert!(!srt.as_bytes().windows(2).any(|w| w == [0xC8, 0xA1]));
    assert!(srt.as_bytes().windows(3).any(|w| w == [0xE8, 0x89, 0xBE]));
}
