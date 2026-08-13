use base64::Engine;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

const DASHSCOPE_BASE: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1";
const DEFAULT_MODEL: &str = "qwen3-asr-flash";

#[derive(Debug, Error)]
pub enum AsrError {
    #[error("missing api key")]
    MissingApiKey,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("network error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("api error ({code}): {message}")]
    Api { code: String, message: String },
    #[error("invalid response: {0}")]
    Invalid(String),
    #[error("local asr error: {0}")]
    Local(String),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SubtitleSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum InputContent {
    InputAudio {
        input_audio: InputAudioData,
    },
}

#[derive(Debug, Serialize)]
struct InputAudioData {
    data: String,
}

#[derive(Debug, Serialize)]
struct UserMessage {
    role: String,
    content: Vec<InputContent>,
}

#[derive(Debug, Serialize, Default)]
struct AsrOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<String>,
    enable_itn: bool,
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<UserMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    asr_options: Option<AsrOptions>,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChoiceMessage {
    #[serde(default)]
    content: serde_json::Value,
}

fn detect_mime(path: &Path) -> &'static str {
    match path.extension().and_then(|s| s.to_str()).map(str::to_lowercase).as_deref() {
        Some("wav") => "audio/wav",
        Some("mp3") => "audio/mpeg",
        Some("m4a") | Some("mp4") => "audio/mp4",
        Some("ogg") => "audio/ogg",
        Some("flac") => "audio/flac",
        Some("opus") => "audio/ogg",
        Some("pcm") => "audio/L16",
        Some("aac") => "audio/aac",
        _ => "audio/wav",
    }
}

/// Call the official Qwen-ASR (qwen3-asr-flash) OpenAI-compatible endpoint.
///
/// Per the Alibaba Cloud Model Studio reference:
///   POST {base}/chat/completions
///   body: { "model": "qwen3-asr-flash",
///           "messages": [{ "role": "user",
///                          "content": [{ "type": "input_audio",
///                                        "input_audio": { "data": "<url | data: URI>" } }] }],
///           "stream": false,
///           "asr_options": { "language": "<iso>", "enable_itn": false } }
///
/// The model returns plain transcribed text (no timestamps) under
/// `choices[0].message.content`. We split that text into subtitle cues by
/// punctuation; `srt::build_srt` will evenly assign timestamps across the
/// known total audio duration.
pub async fn transcribe_file(
    audio_path: &Path,
    api_key: &str,
    model: Option<&str>,
    language: Option<&str>,
    enable_itn: bool,
) -> Result<Vec<SubtitleSegment>, AsrError> {
    if api_key.trim().is_empty() {
        return Err(AsrError::MissingApiKey);
    }
    let model = model.unwrap_or(DEFAULT_MODEL).to_string();

    // Guard against OOM: reject files larger than 50 MB
    const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;
    let metadata = tokio::fs::metadata(audio_path).await?;
    if metadata.len() > MAX_FILE_SIZE {
        return Err(AsrError::Invalid(format!(
            "音频文件过大 ({} MB)，最大支持 50 MB。请先压缩或分段处理。",
            metadata.len() / (1024 * 1024)
        )));
    }

    let bytes = tokio::fs::read(audio_path).await?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let mime = detect_mime(audio_path);
    let data_url = format!("data:{};base64,{}", mime, b64);

    let asr_options = AsrOptions {
        language: language.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        enable_itn,
    };
    let asr_options = if asr_options.language.is_some() || asr_options.enable_itn {
        Some(asr_options)
    } else {
        None
    };

    let payload = ChatRequest {
        model: model.clone(),
        messages: vec![UserMessage {
            role: "user".into(),
            content: vec![InputContent::InputAudio {
                input_audio: InputAudioData { data: data_url },
            }],
        }],
        stream: false,
        asr_options,
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()?;

    let resp = client
        .post(format!("{}/chat/completions", DASHSCOPE_BASE))
        .bearer_auth(api_key)
        .json(&payload)
        .send()
        .await?;

    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        // Don't echo the raw body back to the frontend — a 200 body is the
        // full transcript and even error bodies can echo request content.
        // Extract only a short, structured summary when the body is JSON
        // (DashScope/OpenAI error shape: {"error": {"message": ...}}).
        let summary = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| {
                v.pointer("/error/message")
                    .and_then(|m| m.as_str())
                    .map(|s| s.chars().take(300).collect::<String>())
            })
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "请求失败，请检查 API Key 与网络后重试".to_string());
        return Err(AsrError::Api { code: status.to_string(), message: summary });
    }

    // Parse failure on a 2xx response: don't include the raw body (it is the
    // transcription of the user's audio — potentially sensitive).
    let parsed: ChatResponse =
        serde_json::from_str(&text).map_err(|_| AsrError::Invalid("无法解析识别响应（响应格式异常）".into()))?;

    let content = parsed
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .ok_or_else(|| AsrError::Invalid("empty choices".into()))?;

    let raw = match content {
        serde_json::Value::String(s) => s,
        // Some responses wrap the text in an array of `{text: ...}` parts.
        serde_json::Value::Array(items) => items
            .into_iter()
            .filter_map(|v| v.get("text").and_then(|t| t.as_str()).map(str::to_string))
            .collect::<Vec<_>>()
            .join(""),
        other => serde_json::to_string(&other).unwrap_or_default(),
    };

    Ok(split_into_cues(&raw))
}

/// Split a transcript into subtitle cues on sentence-ending punctuation.
/// Caller is responsible for assigning timestamps (srt::build_srt does this
/// by distributing cues evenly across `total_duration_secs`).
fn split_into_cues(text: &str) -> Vec<SubtitleSegment> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut cues = Vec::new();
    let mut buf = String::new();
    for c in trimmed.chars() {
        buf.push(c);
        if matches!(c, '。' | '！' | '？' | '!' | '?' | ';' | '；') {
            let cue = buf.trim().to_string();
            if !cue.is_empty() {
                cues.push(cue);
            }
            buf.clear();
        } else if c == '\n' {
            let cue = buf.trim_end_matches('\n').trim().to_string();
            if !cue.is_empty() {
                cues.push(cue);
            }
            buf.clear();
        }
    }
    let tail = buf.trim();
    if !tail.is_empty() {
        cues.push(tail.to_string());
    }

    if cues.is_empty() {
        cues.push(trimmed.to_string());
    }

    cues.into_iter()
        .map(|t| SubtitleSegment { start: 0.0, end: 0.0, text: t })
        .filter(|s| !s.text.is_empty())
        .collect()
}

/// Configuration for the local whisper.cpp backend.
#[derive(Debug, Clone)]
pub struct LocalConfig {
    /// Absolute path to `whisper-cli.exe`.
    pub exe: PathBuf,
    /// Absolute path to a ggml model file (e.g. `ggml-base.bin`).
    pub model: PathBuf,
    /// Optional language hint (`zh`, `en`, ...). `None` -> auto-detect.
    pub language: Option<String>,
}

impl LocalConfig {
    pub fn validate(&self) -> Result<(), AsrError> {
        if !self.exe.exists() {
            return Err(AsrError::Local(format!(
                "whisper-cli 未找到: {}",
                self.exe.display()
            )));
        }
        if !self.model.exists() {
            return Err(AsrError::Local(format!(
                "whisper 模型未找到: {}",
                self.model.display()
            )));
        }
        Ok(())
    }
}

/// Run the local whisper.cpp backend. We invoke the CLI with `-osrt` so the
/// model writes SRT next to its output prefix; we then parse that file into
/// `SubtitleSegment`s with real timestamps.
pub async fn transcribe_local(
    audio_path: &Path,
    cfg: &LocalConfig,
) -> Result<Vec<SubtitleSegment>, AsrError> {
    cfg.validate()?;

    let audio_path = audio_path.to_path_buf();
    let stem = audio_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("audio")
        .to_string();
    let out_dir = std::env::temp_dir();
    // Use PID + nanoseconds to avoid collisions between concurrent runs
    let pid = std::process::id();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let out_base = out_dir.join(format!("whisper_out_{}_{}_{}", stem, pid, ts));

    // Build args. `--no-prints` keeps stderr/stdout minimal; `-osrt` writes SRT.
    let mut args: Vec<String> = vec![
        "--no-prints".into(),
        "-m".into(),
        cfg.model.to_string_lossy().into_owned(),
        "-f".into(),
        audio_path.to_string_lossy().into_owned(),
        "-of".into(),
        out_base.to_string_lossy().into_owned(),
        "-osrt".into(),
    ];
    if let Some(lang) = cfg.language.as_ref().filter(|s| !s.is_empty()) {
        args.push("-l".into());
        args.push(lang.clone());
    }

    let exe_path = cfg.exe.clone();
    let srt_path: PathBuf = out_base.with_extension("srt");
    // kill_on_drop(true): if the timeout fires, the future owning the child is
    // dropped and tokio kills the process — a plain timeout on `output()`
    // would leave whisper-cli running in the background.
    let child = tokio::process::Command::new(&exe_path)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| AsrError::Local(format!("启动 whisper-cli 失败: {}", e)))?;

    let output = match tokio::time::timeout(Duration::from_secs(300), async {
        child.wait_with_output().await
    })
    .await
    {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => return Err(AsrError::Local(format!("whisper-cli 运行失败: {}", e))),
        Err(_) => {
            let _ = std::fs::remove_file(&srt_path);
            return Err(AsrError::Local(
                "whisper-cli 执行超时（5分钟），可能被卡死，请检查模型和音频文件".into(),
            ));
        }
    };

    if !output.status.success() {
        let mut msg = String::from_utf8_lossy(&output.stderr).into_owned();
        if msg.is_empty() {
            msg = String::from_utf8_lossy(&output.stdout).into_owned();
        }
        let _ = std::fs::remove_file(&srt_path);
        return Err(AsrError::Local(format!(
            "whisper-cli 退出码 {:?}: {}",
            output.status.code(),
            msg
        )));
    }

    // whisper-cli writes "<out_base>.srt" — try that path first.
    if !srt_path.exists() {
        return Err(AsrError::Local(format!(
            "未找到 whisper 生成的 SRT: {}",
            srt_path.display()
        )));
    }
    let srt_text = tokio::fs::read_to_string(&srt_path).await?;
    let _ = tokio::fs::remove_file(&srt_path).await;

    Ok(parse_srt(&srt_text))
}

/// Minimal SRT parser: tolerates blank lines, BOM, and CRLF; ignores the
/// numeric cue index. Returns one `SubtitleSegment` per cue.
/// `pub(crate)` so the integration regression can exercise the real parse
/// path; not part of the external API.
pub(crate) fn parse_srt(text: &str) -> Vec<SubtitleSegment> {
    let text = text.trim_start_matches('\u{feff}');
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let ts_re = srt_ts_re();

    let mut out = Vec::new();
    for block in normalized.split("\n\n") {
        let block = block.trim_matches('\n');
        if block.is_empty() {
            continue;
        }
        let Some(caps) = ts_re.captures(block) else { continue };
        let start = hmsf_to_secs(&caps[1], &caps[2], &caps[3], &caps[4]);
        let end = hmsf_to_secs(&caps[5], &caps[6], &caps[7], &caps[8]);
        // Drop the cue index line ("1", "2", ...) and the timestamp line; keep the rest as text.
        let lines: Vec<&str> = block.lines().collect();
        let mut text_lines: Vec<&str> = Vec::new();
        let mut ts_seen = false;
        for line in lines {
            if !ts_seen && ts_re.is_match(line) {
                ts_seen = true;
                continue;
            }
            if !ts_seen {
                // cue index line(s) — skip anything that's purely digits/whitespace.
                if line.trim().chars().all(|c| c.is_ascii_digit() || c.is_whitespace()) {
                    continue;
                }
            }
            text_lines.push(line);
        }
        let body = text_lines.join("\n").trim().to_string();
        if body.is_empty() {
            continue;
        }
        out.push(SubtitleSegment { start, end, text: body });
    }
    out
}

static SRT_TS_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
fn srt_ts_re() -> &'static Regex {
    SRT_TS_RE.get_or_init(|| {
        Regex::new(r"(?m)^(\d+):(\d{2}):(\d{2})[,.](\d{1,3})\s*-->\s*(\d+):(\d{2}):(\d{2})[,.](\d{1,3})")
            .expect("hardcoded SRT timestamp regex must be valid")
    })
}

fn hmsf_to_secs(h: &str, m: &str, s: &str, ms: &str) -> f64 {
    let h: f64 = h.parse().unwrap_or(0.0);
    let m: f64 = m.parse().unwrap_or(0.0);
    let s: f64 = s.parse().unwrap_or(0.0);
    let frac = if ms.len() == 3 {
        ms.parse::<f64>().unwrap_or(0.0) / 1000.0
    } else if ms.len() == 1 {
        ms.parse::<f64>().unwrap_or(0.0) / 10.0
    } else if ms.len() == 2 {
        ms.parse::<f64>().unwrap_or(0.0) / 100.0
    } else {
        0.0
    };
    h * 3600.0 + m * 60.0 + s + frac
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_chinese_and_english() {
        let cues = split_into_cues("你好，世界！Hello world? 是的。");
        assert_eq!(cues.len(), 3);
        assert_eq!(cues[0].text, "你好，世界！");
        assert_eq!(cues[1].text, "Hello world?");
        assert_eq!(cues[2].text, "是的。");
    }

    #[test]
    fn parses_srt_basic() {
        let srt = "1\n00:00:00,000 --> 00:00:02,500\nHi there.\n\n2\n00:00:02,600 --> 00:00:05,000\nNext line.\n";
        let cues = parse_srt(srt);
        assert_eq!(cues.len(), 2);
        assert!((cues[0].start - 0.0).abs() < 1e-6);
        assert!((cues[0].end - 2.5).abs() < 1e-6);
        assert_eq!(cues[1].text, "Next line.");
    }

    #[test]
    fn parses_srt_with_bom_and_crlf() {
        // whisper-cli writes UTF-8 BOM + CRLF on Windows; both must be handled.
        let srt = "\u{feff}1\r\n00:00:00,000 --> 00:00:02,500\r\nHi.\r\n\r\n2\r\n00:00:02,600 --> 00:00:05,000\r\nNext.\r\n";
        let cues = parse_srt(srt);
        assert_eq!(cues.len(), 2);
        assert!((cues[0].start - 0.0).abs() < 1e-6);
        assert!((cues[0].end - 2.5).abs() < 1e-6);
        assert_eq!(cues[0].text, "Hi.");
        assert_eq!(cues[1].text, "Next.");
    }

    #[test]
    fn empty_or_garbage_input_yields_no_cues() {
        assert!(parse_srt("").is_empty());
        assert!(parse_srt("  \n  ").is_empty());
        assert!(parse_srt("hello world").is_empty());
        // Timestamp but no body → skipped.
        assert!(parse_srt("1\n00:00:00,000 --> 00:00:02,500\n").is_empty());
    }

    #[test]
    fn malformed_blocks_are_skipped() {
        let srt = "1\njust some text without timestamps\n\n2\n00:00:00,000 --> 00:00:01,000\nGood\n\n3\n";
        let cues = parse_srt(srt);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].text, "Good");
        assert!((cues[0].end - 1.0).abs() < 1e-6);
    }

    #[test]
    fn timestamp_fraction_digits_and_separators() {
        // 3-digit ms: comma and dot separators both accepted.
        assert!((hmsf_to_secs("0", "0", "1", "234") - 1.234).abs() < 1e-6);
        assert!((hmsf_to_secs("0", "0", "1", "500") - 1.5).abs() < 1e-6);
        // 2-digit and 1-digit fractions scale accordingly.
        assert!((hmsf_to_secs("0", "0", "1", "23") - 1.23).abs() < 1e-6);
        assert!((hmsf_to_secs("0", "0", "1", "2") - 1.2).abs() < 1e-6);
        // Hours beyond 59 parse fine (SRT allows arbitrary hours).
        assert!((hmsf_to_secs("1", "2", "3", "000") - 3723.0).abs() < 1e-6);
        assert!((hmsf_to_secs("99", "59", "59", "999") - 359999.999).abs() < 1e-6);
        // Empty fraction must not panic (regex prevents it, defensive only).
        assert!((hmsf_to_secs("0", "0", "1", "") - 1.0).abs() < 1e-6);
    }

    #[test]
    fn multi_line_cue_body_is_preserved() {
        let srt = "1\n00:00:00,000 --> 00:00:02,000\nfirst line\nsecond line\n";
        let cues = parse_srt(srt);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].text, "first line\nsecond line");
    }
}
