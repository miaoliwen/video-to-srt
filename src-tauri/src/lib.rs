mod asr;
mod convert;
mod ffmpeg;
mod srt;

#[cfg(test)]
mod encoding_tests;

use std::path::PathBuf;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Serialize, Clone)]
struct StepEvent {
    stage: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    progress: Option<f32>,
}

fn emit(app: &AppHandle, stage: &str, message: impl Into<String>, progress: Option<f32>) {
    let _ = app.emit(
        "pipeline-progress",
        StepEvent { stage: stage.into(), message: message.into(), progress },
    );
}

#[derive(Debug, Serialize)]
pub struct ExtractResult {
    audio_path: String,
    duration_secs: f64,
}

#[tauri::command]
async fn extract_audio(app: AppHandle, video_path: String) -> Result<ExtractResult, String> {
    emit(&app, "extracting", "正在提取音频…", Some(0.05));
    let video_path = PathBuf::from(video_path);
    if !video_path.exists() {
        return Err(format!("视频文件不存在: {}", video_path.display()));
    }
    let ffmpeg = ffmpeg::locate_ffmpeg().map_err(|e| e.to_string())?;
    let stem = video_path.file_stem().and_then(|s| s.to_str()).unwrap_or("audio");
    let parent = video_path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| std::env::temp_dir());
    let out_path = {
        let candidate = parent.join(format!("{}_16k_mono.wav", stem));
        if candidate.exists() {
            // Avoid overwriting existing file — append a unique suffix
            let pid = std::process::id();
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            parent.join(format!("{}_16k_mono_{}_{}.wav", stem, pid, ts))
        } else {
            candidate
        }
    };
    ffmpeg::extract_audio(&ffmpeg, &video_path, &out_path).map_err(|e| e.to_string())?;
    let duration = ffmpeg::probe_duration(&out_path).unwrap_or(0.0);
    emit(&app, "extracting", "音频提取完成", Some(0.25));
    Ok(ExtractResult {
        audio_path: out_path.to_string_lossy().into_owned(),
        duration_secs: duration,
    })
}

#[derive(Debug, Serialize)]
pub struct TranscribeResult {
    segments: Vec<asr::SubtitleSegment>,
    srt: String,
}

#[tauri::command]
async fn transcribe(
    app: AppHandle,
    audio_path: String,
    backend: String,
    api_key: Option<String>,
    _model: Option<String>,
    language: Option<String>,
    enable_itn: Option<bool>,
    whisper_exe: Option<String>,
    whisper_model: Option<String>,
    whisper_language: Option<String>,
    total_duration_secs: f64,
) -> Result<TranscribeResult, String> {
    emit(&app, "transcribing", "正在调用语音识别…", Some(0.4));
    let audio_path = PathBuf::from(audio_path);

    let segments = match backend.as_str() {
        "local" => {
            emit(&app, "transcribing", "正在调用本地 Whisper.cpp 识别…", Some(0.45));
            let exe = whisper_exe
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| "未配置 whisper-cli 路径".to_string())?;
            let model_path = whisper_model
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| "未配置 whisper 模型路径".to_string())?;
            let cfg = asr::LocalConfig {
                exe: PathBuf::from(exe),
                model: PathBuf::from(model_path),
                language: whisper_language
                    .as_ref()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
            };
            cfg.validate().map_err(|e| e.to_string())?;
            asr::transcribe_local(&audio_path, &cfg)
                .await
                .map_err(|e| e.to_string())?
        }
        "cloud" | "qwen" => {
            emit(&app, "transcribing", "正在调用 Qwen ASR 识别…", Some(0.45));
            if api_key.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()).is_none() {
                return Err("未配置 API Key，请先在设置中填写".to_string());
            }
            asr::transcribe_file(
                &audio_path,
                api_key.as_deref().unwrap_or(""),
                None,
                language.as_deref(),
                enable_itn.unwrap_or(false),
            )
            .await
            .map_err(|e| e.to_string())?
        }
        _ => {
            return Err(format!(
                "无效的 backend 值: '{}'。支持的值为 'local' 和 'cloud'。",
                backend
            ));
        }
    };

    emit(&app, "transcribing", &format!("识别完成，共 {} 段", segments.len()), Some(0.85));

        // 强制把任何繁体中文字符转换为简体 —— whisper 多语言模型偶尔会输出繁体。
        let segments: Vec<asr::SubtitleSegment> = segments
            .into_iter()
            .map(|mut s| {
                s.text = convert::t2s(&s.text);
                s
            })
            .collect();

        let srt_content = srt::build_srt(&segments, total_duration_secs);
    Ok(TranscribeResult { segments, srt: srt_content })
}

/// Quick health check for the local Whisper backend.
#[tauri::command]
async fn check_local_whisper(
    whisper_exe: Option<String>,
    whisper_model: Option<String>,
) -> Result<bool, String> {
    let exe = whisper_exe.unwrap_or_default();
    let model = whisper_model.unwrap_or_default();
    if exe.trim().is_empty() || model.trim().is_empty() {
        return Ok(false);
    }
    let cfg = asr::LocalConfig {
        exe: PathBuf::from(exe),
        model: PathBuf::from(model),
        language: None,
    };
    Ok(cfg.validate().is_ok())
}

#[tauri::command]
async fn save_srt(app: AppHandle, output_path: String, content: String) -> Result<String, String> {
    let p = PathBuf::from(&output_path);

    // Security: validate path is within an allowed write scope
    let canonical = p.canonicalize().map_err(|e| format!("路径无效: {}", e))?;
    // Only allow .srt extension to prevent writing executables/configs
    if canonical.extension().and_then(|e| e.to_str()) != Some("srt") {
        return Err("只允许保存 .srt 文件".to_string());
    }

    if let Some(parent) = p.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
    }
    // Prepend UTF-8 BOM so Chinese characters render correctly in Notepad and
    // various players that fall back to ANSI/GBK when no BOM is present. The
    // in-memory `content` returned to the frontend stays BOM-less.
    let mut bytes: Vec<u8> = Vec::with_capacity(content.len() + 3);
    bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    bytes.extend_from_slice(content.as_bytes());
    tokio::fs::write(&p, bytes).await.map_err(|e| e.to_string())?;
    emit(&app, "done", &format!("SRT 已保存: {}", p.display()), Some(1.0));
    Ok(p.to_string_lossy().into_owned())
}

#[tauri::command]
fn locate_ffmpeg_binary() -> Result<String, String> {
    let p = ffmpeg::locate_ffmpeg().map_err(|e| e.to_string())?;
    Ok(p.to_string_lossy().into_owned())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            extract_audio,
            transcribe,
            save_srt,
            locate_ffmpeg_binary,
            check_local_whisper,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
