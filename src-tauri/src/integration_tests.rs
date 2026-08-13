//! 完整文本管线集成回归测试。
//!
//! 模拟真实链路：本地 whisper-cli 输出的 SRT（BOM + CRLF、繁简混杂、
//! 多对一字、全角标点）→ `asr::parse_srt` 解析 → `convert::t2s` 繁→简
//! → `srt::build_srt` 重新生成。与 `lib.rs::transcribe` 中实际执行的
//! 代码路径一致（仅跳过 ffmpeg / ASR 网络与子进程调用）。
//!
//! fixture 位于 `src-tauri/tests/fixtures/`，均为真实格式的 SRT 文件。

use crate::{asr, convert, srt};

/// 运行与 transcribe 相同的文本处理链，返回最终 SRT 字符串。
fn run_pipeline(fixture: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(fixture);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("读取 fixture {fixture} 失败: {e}"));
    let text = String::from_utf8(bytes).expect("fixture 必须是合法 UTF-8");

    let mut segments = asr::parse_srt(&text);
    for seg in segments.iter_mut() {
        seg.text = convert::t2s(&seg.text);
    }
    // 与 transcribe 一致：以最后一段的结束时间作为总时长重建 SRT。
    let total = segments.last().map(|s| s.end).unwrap_or(0.0);
    srt::build_srt(&segments, total)
}

#[test]
fn traditional_whisper_fixture_is_fully_simplified() {
    let out = run_pipeline("whisper_traditional.srt");

    // 六条 cue 全部转换为简体。
    for phrase in [
        "各位观众，欢迎收看今晚的新闻报导",
        "今天全国各地都在庆祝国庆节，学校和公园里人山人海",
        "老师教我们学习数学和科学，还要练习写字",
        "医生说要多运动、多锻炼，身体才会健康",
        "公司推出了新产品，价格实惠，质量很好",
        "历史上的乾隆皇帝很有名，大家都听过他的名字",
    ] {
        assert!(out.contains(phrase), "缺少转换后的字幕: {phrase}\n---\n{out}");
    }

    // 多对一：乾(qián) 保留，不得出现干隆误转。
    assert!(out.contains("乾隆皇帝"));
    assert!(!out.contains("干隆"));

    // 全角中文标点保留。
    assert!(out.contains("，"));
    assert!(!out.contains(",欢迎"));

    // 时间轴与序号完整保留（6 条 cue）。
    assert!(out.starts_with("1\r\n00:00:00,000 --> 00:00:03,200"));
    assert!(out.contains("6\r\n00:00:16,500 --> 00:00:20,000"));
    assert_eq!(out.matches("-->").count(), 6);
}

#[test]
fn mixed_english_numbers_fixture_normalizes_but_keeps_cjk_punct() {
    let out = run_pipeline("whisper_mixed.srt");

    // 全角数字与圈数字归一化，中文繁体转换。
    assert!(out.contains("大家好，今天是2026年8月13日，天气晴朗，大家都很开心。"));
    // 英文原文保留（含 C++ 等特殊字符）。
    assert!(out.contains(
        "English subtitle with 123 numbers and C++ code references."
    ));
    // 中文全角标点不被归一化。
    assert!(out.contains("，"));
    assert!(out.contains("。"));
    assert_eq!(out.matches("-->").count(), 2);
}

#[test]
fn simplified_fixture_stays_unchanged() {
    let out = run_pipeline("srt_simplified.srt");

    // 已简化的文本不被改动。
    assert!(out.contains("这是一个已经简化的字幕文件，用来验证简化字不会被改动。"));
    // 简体文本中的 乾隆（繁简同形）不被误转。
    assert!(out.contains("乾隆皇帝在位期间天下太平，全角标点：，。！？都应该保留。"));
    assert!(!out.contains("干隆"));
}
