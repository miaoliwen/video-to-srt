mod asr;
mod convert;
mod ffmpeg;
mod srt;

#[cfg(test)]
mod encoding_tests;
#[cfg(test)]
mod integration_tests;

use std::path::{Path, PathBuf};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Serialize, Clone)]
struct StepEvent {
    /// Which pipeline run this event belongs to (None = not run-scoped, e.g.
    /// save_srt). Lets the frontend ignore stale events from earlier runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    job_id: Option<String>,
    stage: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    progress: Option<f32>,
}

fn emit(
    app: &AppHandle,
    job_id: Option<&str>,
    stage: &str,
    message: impl Into<String>,
    progress: Option<f32>,
) {
    if let Err(e) = app.emit(
        "pipeline-progress",
        StepEvent {
            job_id: job_id.map(str::to_string),
            stage: stage.into(),
            message: message.into(),
            progress,
        },
    ) {
        eprintln!("pipeline-progress 事件发送失败: {e}");
    }
}

#[derive(Debug, Serialize)]
pub struct ExtractResult {
    audio_path: String,
    duration_secs: f64,
}

#[tauri::command]
async fn extract_audio(
    app: AppHandle,
    video_path: String,
    job_id: Option<String>,
) -> Result<ExtractResult, String> {
    emit(&app, job_id.as_deref(), "extracting", "正在提取音频…", Some(0.05));
    let video_path = PathBuf::from(video_path);
    if !video_path.exists() {
        return Err(format!("视频文件不存在: {}", video_path.display()));
    }
    if !video_path.is_file() {
        return Err(format!("所选路径不是文件: {}", video_path.display()));
    }
    let ffmpeg = ffmpeg::locate_ffmpeg().map_err(|e| e.to_string())?;
    let stem = video_path.file_stem().and_then(|s| s.to_str()).unwrap_or("audio");
    // Always write the intermediate WAV into the system temp dir: the video's
    // folder may be read-only, and `transcribe` deletes this file when done,
    // so leaving it next to the source would accumulate ~115 MB per hour of
    // video. The PID+nanos suffix keeps concurrent runs from colliding.
    let pid = std::process::id();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let out_path = std::env::temp_dir()
        .join("video-to-srt")
        .join(format!("{}_16k_mono_{}_{}.wav", stem, pid, ts));
    ffmpeg::extract_audio(&ffmpeg, &video_path, &out_path).await.map_err(|e| e.to_string())?;
    // The 30s timeout lives inside probe_duration (kill_on_drop), so a hung
    // ffprobe is killed instead of being left running in the background.
    let duration = ffmpeg::probe_duration(&out_path).await.unwrap_or(0.0);
    emit(&app, job_id.as_deref(), "extracting", "音频提取完成", Some(0.25));
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

/// Windows Credential Manager service/account names for the API key. The key
/// never touches the frontend or a plaintext file in the normal path.
const KEYRING_SERVICE: &str = "com.vibecoding.video-to-srt";
const KEYRING_USER: &str = "qwen-api-key";

fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|e| format!("无法定位应用数据目录: {}", e))
}

/// Abstraction over the OS credential store, so the migration chain can be
/// unit-tested without touching the real Windows Credential Manager.
trait CredentialStore {
    /// Raw stored value, or None if no entry exists.
    fn read(&self) -> Option<String>;
    fn write(&self, key: &str) -> Result<(), String>;
    fn delete(&self) -> Result<(), String>;
}

/// The real Windows Credential Manager backend (keyring + wincred).
struct CredentialManagerStore;

impl CredentialStore for CredentialManagerStore {
    fn read(&self) -> Option<String> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).ok()?;
        entry.get_password().ok()
    }

    fn write(&self, key: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .map_err(|e| format!("无法访问系统凭据管理器: {}", e))?;
        entry
            .set_password(key)
            .map_err(|e| format!("保存到系统凭据管理器失败: {}", e))
    }

    fn delete(&self) -> Result<(), String> {
        match keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
            Ok(entry) => match entry.delete_credential() {
                Ok(()) => Ok(()),
                Err(keyring::Error::NoEntry) => Ok(()),
                Err(e) => Err(format!("删除系统凭据失败: {}", e)),
            },
            Err(_) => Ok(()),
        }
    }
}

// --- Legacy plaintext locations (from previous releases) ---

fn read_api_key_file_at(dir: &Path) -> Option<String> {
    let s = std::fs::read_to_string(dir.join("api_key.txt")).ok()?;
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

fn remove_api_key_file_at(dir: &Path) {
    let _ = std::fs::remove_file(dir.join("api_key.txt"));
}

/// Legacy pre-fix location: `settings.json`, which the frontend could read.
fn read_legacy_settings_key_at(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("settings.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let key = value.get("apiKey")?.as_str()?.trim();
    if key.is_empty() {
        None
    } else {
        Some(key.to_string())
    }
}

/// Best-effort removal of the legacy `apiKey` field from settings.json.
fn scrub_settings_key_in(dir: &Path) {
    let legacy = dir.join("settings.json");
    let Ok(text) = std::fs::read_to_string(&legacy) else {
        return;
    };
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    if value
        .as_object_mut()
        .and_then(|o| o.remove("apiKey"))
        .is_some()
    {
        let _ = std::fs::write(
            &legacy,
            serde_json::to_string_pretty(&value).unwrap_or_default(),
        );
    }
}

/// Read the API key, preferring the primary store and migrating legacy
/// plaintext locations (`api_key.txt`, then `settings.json`) into it.
/// Migration is best-effort: if the store is unavailable, the legacy key is
/// still returned (and the plaintext file kept) so the app keeps working.
fn read_api_key_with<S: CredentialStore>(store: &S, dir: &Path) -> Result<String, String> {
    // 1) Primary store — Windows Credential Manager in production.
    if let Some(key) = store.read().map(|k| k.trim().to_string()).filter(|k| !k.is_empty()) {
        return Ok(key);
    }
    // 2) Migrate from the legacy api_key.txt (previous release's storage).
    if let Some(key) = read_api_key_file_at(dir) {
        if store.write(&key).is_ok() {
            remove_api_key_file_at(dir);
        }
        return Ok(key);
    }
    // 3) Migrate from the pre-fix settings.json (frontend-visible plaintext).
    if let Some(key) = read_legacy_settings_key_at(dir) {
        if store.write(&key).is_ok() {
            scrub_settings_key_in(dir);
        }
        return Ok(key);
    }
    Err("未配置 API Key，请先在设置中填写".to_string())
}

fn set_api_key_with<S: CredentialStore>(store: &S, dir: &Path, key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("API Key 不能为空".to_string());
    }
    store.write(key)?;
    // The key now lives in the OS credential store — remove any plaintext
    // leftovers so it isn't duplicated on disk.
    remove_api_key_file_at(dir);
    scrub_settings_key_in(dir);
    Ok(())
}

fn clear_api_key_with<S: CredentialStore>(store: &S, dir: &Path) -> Result<(), String> {
    store.delete()?;
    remove_api_key_file_at(dir);
    scrub_settings_key_in(dir);
    Ok(())
}

/// Read the API key for the running app.
fn read_api_key(app: &AppHandle) -> Result<String, String> {
    let dir = app_data_dir(app)?;
    read_api_key_with(&CredentialManagerStore, &dir)
}

/// Whether an API key is configured. Never exposes the key itself.
#[tauri::command]
fn get_has_api_key(app: AppHandle) -> bool {
    read_api_key(&app).is_ok()
}

#[tauri::command]
fn set_api_key(app: AppHandle, key: String) -> Result<(), String> {
    let dir = app_data_dir(&app)?;
    set_api_key_with(&CredentialManagerStore, &dir, &key)
}

#[tauri::command]
fn clear_api_key(app: AppHandle) -> Result<(), String> {
    let dir = app_data_dir(&app)?;
    clear_api_key_with(&CredentialManagerStore, &dir)
}

/// Whether `path`'s canonicalized parent directory is the app's own temp
/// subdirectory (`<temp>/video-to-srt`, where `extract_audio` writes the
/// intermediate WAV). Used to constrain `transcribe`'s cleanup so it can
/// never delete a caller-chosen path outside that directory.
fn is_in_temp_subdir(path: &Path, expected_dir: &Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    match (parent.canonicalize(), expected_dir.canonicalize()) {
        (Ok(actual), Ok(expected)) => actual == expected,
        // Fail-safe: if either side can't be resolved (e.g. the temp dir
        // doesn't exist), refuse to delete rather than risk a wrong path.
        _ => false,
    }
}

/// Best-effort cleanup of intermediate WAV files left behind by a crash
/// (process killed between `extract_audio` and `transcribe`). Only removes
/// files older than `max_age` so a concurrent run's fresh files are never
/// touched; silently ignores any I/O error.
fn cleanup_stale_temp_wavs_in(dir: &Path, max_age: std::time::Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let Ok(modified) = meta.modified() else {
            continue;
        };
        let stale = now
            .duration_since(modified)
            .map(|age| age > max_age)
            // Clock skew / future mtime: treat as not stale, don't delete.
            .unwrap_or(false);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[tauri::command]
async fn transcribe(
    app: AppHandle,
    audio_path: String,
    backend: String,
    model: Option<String>,
    language: Option<String>,
    enable_itn: Option<bool>,
    whisper_exe: Option<String>,
    whisper_model: Option<String>,
    whisper_language: Option<String>,
    total_duration_secs: f64,
    job_id: Option<String>,
) -> Result<TranscribeResult, String> {
    emit(&app, job_id.as_deref(), "transcribing", "正在调用语音识别…", Some(0.4));
    let audio_path = PathBuf::from(audio_path);
    // Validate the input up front so both backends fail with the same,
    // readable error instead of cryptic subprocess/HTTP errors.
    if !audio_path.exists() {
        return Err(format!("音频文件不存在: {}", audio_path.display()));
    }
    if !audio_path.is_file() {
        return Err(format!("音频路径不是文件: {}", audio_path.display()));
    }

    // Route every error through the result so the temp WAV is cleaned up on
    // all paths (success or failure) — the file is owned by this run.
    let segments_result: Result<Vec<asr::SubtitleSegment>, String> = match backend.as_str() {
        "local" => {
            emit(&app, job_id.as_deref(), "transcribing", "正在调用本地 Whisper.cpp 识别…", Some(0.45));
            let cfg_result: Result<asr::LocalConfig, String> = (|| {
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
                Ok(cfg)
            })();
            match cfg_result {
                Ok(cfg) => {
                    asr::transcribe_local(&audio_path, &cfg).await.map_err(|e| e.to_string())
                }
                Err(e) => Err(e),
            }
        }
        "cloud" | "qwen" => {
            emit(&app, job_id.as_deref(), "transcribing", "正在调用 Qwen ASR 识别…", Some(0.45));
            // The key is never passed from the frontend — read it from the
            // backend-owned store so it can't be exfiltrated by injected JS.
            match read_api_key(&app) {
                Ok(api_key) => asr::transcribe_file(
                    &audio_path,
                    &api_key,
                    model.as_deref(),
                    language.as_deref(),
                    enable_itn.unwrap_or(false),
                )
                .await
                .map_err(|e| e.to_string()),
                Err(e) => Err(e),
            }
        }
        _ => Err(format!(
            "无效的 backend 值: '{}'。支持的值为 'local' 和 'cloud'。",
            backend
        )),
    };

    // The intermediate WAV is owned by this pipeline run and lives in the
    // app's temp subdirectory — delete it on every path so temp files don't
    // accumulate. Only delete files that actually live there: `audio_path`
    // comes from the IPC caller, so an unverified delete would let any
    // future caller destroy arbitrary files by pointing the path elsewhere.
    // Fail-safe: if the path can't be verified, leave the file behind.
    if is_in_temp_subdir(&audio_path, &std::env::temp_dir().join("video-to-srt")) {
        let _ = tokio::fs::remove_file(&audio_path).await;
    }
    let segments = segments_result?;

    emit(&app, job_id.as_deref(), "transcribing", format!("识别完成，共 {} 段", segments.len()), Some(0.85));

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

#[tauri::command]
async fn save_srt(app: AppHandle, output_path: String, content: String) -> Result<String, String> {
    let p = PathBuf::from(&output_path);

    // Only allow .srt extension (case-insensitive) to prevent writing
    // executables/configs.
    let ext_ok = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("srt"))
        .unwrap_or(false);
    if !ext_ok {
        return Err("只允许保存 .srt 文件".to_string());
    }

    // The chosen file usually doesn't exist yet, so canonicalize the *parent*
    // directory (resolving symlinks) instead of the file itself, then re-join
    // the file name. This prevents the target from being redirected outside
    // the chosen folder while still allowing new files.
    let parent = p
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
    let canonical_parent = parent.canonicalize().map_err(|e| format!("路径无效: {}", e))?;
    let file_name = p.file_name().ok_or_else(|| "路径无效".to_string())?;
    let final_path = canonical_parent.join(file_name);

    // Prepend UTF-8 BOM so Chinese characters render correctly in Notepad and
    // various players that fall back to ANSI/GBK when no BOM is present. The
    // in-memory `content` returned to the frontend stays BOM-less.
    let mut bytes: Vec<u8> = Vec::with_capacity(content.len() + 3);
    bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    bytes.extend_from_slice(content.as_bytes());
    tokio::fs::write(&final_path, bytes).await.map_err(|e| e.to_string())?;
    emit(
        &app,
        None,
        "done",
        format!("SRT 已保存: {}", final_path.display()),
        Some(1.0),
    );
    Ok(final_path.to_string_lossy().into_owned())
}

#[tauri::command]
fn locate_ffmpeg_binary() -> Result<String, String> {
    let p = ffmpeg::locate_ffmpeg().map_err(|e| e.to_string())?;
    Ok(p.to_string_lossy().into_owned())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Best-effort cleanup of intermediate WAVs left behind by a crash
    // (process killed between extract_audio and transcribe).
    cleanup_stale_temp_wavs_in(
        &std::env::temp_dir().join("video-to-srt"),
        std::time::Duration::from_secs(24 * 3600),
    );
    // Build failures (e.g. bad config) must not panic — report and exit
    // cleanly instead of aborting via `panic = "abort"` with no message.
    match tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            extract_audio,
            transcribe,
            save_srt,
            locate_ffmpeg_binary,
            get_has_api_key,
            set_api_key,
            clear_api_key,
        ])
        .build(tauri::generate_context!())
    {
        Ok(app) => {
            app.run(|_app_handle, _event| {});
        }
        Err(e) => {
            eprintln!("应用启动失败: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// In-memory stand-in for the OS credential store; None = no entry.
    struct MockStore(Mutex<Option<String>>);

    impl MockStore {
        fn new(key: Option<String>) -> Self {
            MockStore(Mutex::new(key))
        }
    }

    impl CredentialStore for MockStore {
        fn read(&self) -> Option<String> {
            self.0.lock().unwrap().clone()
        }

        fn write(&self, key: &str) -> Result<(), String> {
            *self.0.lock().unwrap() = Some(key.to_string());
            Ok(())
        }

        fn delete(&self) -> Result<(), String> {
            *self.0.lock().unwrap() = None;
            Ok(())
        }
    }

    /// A store whose writes always fail — simulates an unavailable
    /// Credential Manager during migration.
    struct FailingStore;

    impl CredentialStore for FailingStore {
        fn read(&self) -> Option<String> {
            None
        }

        fn write(&self, _key: &str) -> Result<(), String> {
            Err("storage unavailable".into())
        }

        fn delete(&self) -> Result<(), String> {
            Err("storage unavailable".into())
        }
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "video-to-srt-keytest_{}_{}_{}",
            tag,
            std::process::id(),
            ts
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(dir: &Path) {
        let _ = std::fs::remove_dir_all(dir);
    }

    fn seed_txt(dir: &Path, content: &str) {
        std::fs::write(dir.join("api_key.txt"), content).unwrap();
    }

    fn seed_settings(dir: &Path, content: &str) {
        std::fs::write(dir.join("settings.json"), content).unwrap();
    }

    #[test]
    fn primary_store_wins_over_legacy_files() {
        let dir = temp_dir("priority");
        seed_txt(&dir, "file-key");
        seed_settings(&dir, r#"{"apiKey":"settings-key"}"#);
        let store = MockStore::new(Some("cm-key".into()));

        let key = read_api_key_with(&store, &dir).unwrap();
        assert_eq!(key, "cm-key");
        // Legacy files must be left untouched when the primary store wins.
        assert_eq!(std::fs::read_to_string(dir.join("api_key.txt")).unwrap(), "file-key");
        cleanup(&dir);
    }

    #[test]
    fn migrates_from_api_key_txt_and_deletes_it() {
        let dir = temp_dir("from_txt");
        seed_txt(&dir, "  old-txt-key  ");
        let store = MockStore::new(None);

        let key = read_api_key_with(&store, &dir).unwrap();
        assert_eq!(key, "old-txt-key"); // trimmed
        assert_eq!(store.read().as_deref(), Some("old-txt-key")); // migrated
        assert!(!dir.join("api_key.txt").exists()); // plaintext removed
        cleanup(&dir);
    }

    #[test]
    fn migrates_from_settings_json_and_scrubs_it() {
        let dir = temp_dir("from_settings");
        seed_settings(
            &dir,
            r#"{"asrSettings":{"backend":"qwen"},"apiKey":"old-settings-key"}"#,
        );
        let store = MockStore::new(None);

        let key = read_api_key_with(&store, &dir).unwrap();
        assert_eq!(key, "old-settings-key");
        assert_eq!(store.read().as_deref(), Some("old-settings-key"));

        // apiKey field scrubbed, unrelated settings preserved.
        let remaining: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("settings.json")).unwrap())
                .unwrap();
        assert!(remaining.get("apiKey").is_none());
        assert_eq!(remaining["asrSettings"]["backend"], "qwen");
        cleanup(&dir);
    }

    #[test]
    fn api_key_txt_takes_precedence_over_settings_json() {
        let dir = temp_dir("txt_vs_settings");
        seed_txt(&dir, "txt-key");
        seed_settings(&dir, r#"{"apiKey":"settings-key"}"#);
        let store = MockStore::new(None);

        let key = read_api_key_with(&store, &dir).unwrap();
        assert_eq!(key, "txt-key");
        // The txt migration doesn't touch settings.json — a later set/clear
        // scrubs it.
        assert!(std::fs::read_to_string(dir.join("settings.json"))
            .unwrap()
            .contains("settings-key"));
        cleanup(&dir);
    }

    #[test]
    fn migration_failure_keeps_legacy_file() {
        let dir = temp_dir("migration_fail");
        seed_txt(&dir, "txt-key");
        let store = FailingStore;

        // Best-effort migration: the key is still returned (session keeps
        // working) and the plaintext file is NOT deleted.
        let key = read_api_key_with(&store, &dir).unwrap();
        assert_eq!(key, "txt-key");
        assert!(dir.join("api_key.txt").exists());
        cleanup(&dir);
    }

    #[test]
    fn no_key_anywhere_returns_error() {
        let dir = temp_dir("no_key");
        let store = MockStore::new(None);
        let err = read_api_key_with(&store, &dir).unwrap_err();
        assert!(err.contains("未配置 API Key"));
        cleanup(&dir);
    }

    #[test]
    fn set_key_writes_primary_store_and_cleans_legacy() {
        let dir = temp_dir("set");
        seed_txt(&dir, "old-file-key");
        seed_settings(&dir, r#"{"apiKey":"old-settings-key"}"#);
        let store = MockStore::new(Some("old-cm-key".into()));

        set_api_key_with(&store, &dir, "  new-key  ").unwrap();
        assert_eq!(store.read().as_deref(), Some("new-key"));
        assert!(!dir.join("api_key.txt").exists());
        assert!(!std::fs::read_to_string(dir.join("settings.json"))
            .unwrap()
            .contains("apiKey"));
        cleanup(&dir);
    }

    #[test]
    fn set_key_rejects_blank() {
        let dir = temp_dir("set_blank");
        let store = MockStore::new(None);
        assert!(set_api_key_with(&store, &dir, "   ").is_err());
        assert_eq!(store.read(), None);
        cleanup(&dir);
    }

    #[test]
    fn clear_key_removes_everything() {
        let dir = temp_dir("clear");
        seed_txt(&dir, "old-file-key");
        seed_settings(&dir, r#"{"apiKey":"old-settings-key"}"#);
        let store = MockStore::new(Some("cm-key".into()));

        clear_api_key_with(&store, &dir).unwrap();
        assert_eq!(store.read(), None);
        assert!(!dir.join("api_key.txt").exists());
        assert!(!std::fs::read_to_string(dir.join("settings.json"))
            .unwrap()
            .contains("apiKey"));
        cleanup(&dir);
    }

    #[test]
    fn delete_guard_accepts_only_app_temp_subdir() {
        let base = temp_dir("delete_guard");
        let app_tmp = base.join("video-to-srt");
        std::fs::create_dir_all(&app_tmp).unwrap();

        let inside = app_tmp.join("movie_16k_mono_1_2.wav");
        std::fs::write(&inside, b"x").unwrap();
        let outside = base.join("user-video.mp4");
        std::fs::write(&outside, b"x").unwrap();

        // A file really inside the app temp subdir is deletable.
        assert!(is_in_temp_subdir(&inside, &app_tmp));
        // Anything outside — even a sibling file — is not.
        assert!(!is_in_temp_subdir(&outside, &app_tmp));
        // Textual containment is not enough: `..` segments resolve outside.
        let sneaky = app_tmp.join("..").join("..").join("user-video.mp4");
        assert!(!is_in_temp_subdir(&sneaky, &app_tmp));
        // The temp dir not existing (no extract_audio ran) → fail-safe false.
        assert!(!is_in_temp_subdir(&base.join("missing").join("x.wav"), &app_tmp));
        // No parent (bare file name) → not deletable.
        assert!(!is_in_temp_subdir(Path::new("x.wav"), &app_tmp));

        // The guard must actually protect the file: delete it only when the
        // path is verified, and leave the outside file untouched.
        if is_in_temp_subdir(&inside, &app_tmp) {
            let _ = std::fs::remove_file(&inside);
        }
        if is_in_temp_subdir(&outside, &app_tmp) {
            let _ = std::fs::remove_file(&outside);
        }
        assert!(!inside.exists());
        assert!(outside.exists());

        cleanup(&base);
    }

    #[test]
    fn stale_temp_cleanup_removes_only_old_files() {
        let base = temp_dir("stale_cleanup");
        let app_tmp = base.join("video-to-srt");
        std::fs::create_dir_all(&app_tmp).unwrap();

        // A WAV left behind by a crash two days ago → should be removed.
        let old = app_tmp.join("old_16k_mono_1_2.wav");
        std::fs::write(&old, b"x").unwrap();
        let backdated =
            std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 24 * 3600);
        // Need a write handle to set the mtime (read-only handles lack
        // FILE_WRITE_ATTRIBUTES on Windows).
        std::fs::OpenOptions::new()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(backdated)
            .unwrap();

        // A fresh file from a run that is still in progress → must survive.
        let fresh = app_tmp.join("fresh_16k_mono_3_4.wav");
        std::fs::write(&fresh, b"x").unwrap();

        cleanup_stale_temp_wavs_in(&app_tmp, std::time::Duration::from_secs(24 * 3600));

        assert!(!old.exists());
        assert!(fresh.exists());
        cleanup(&base);
    }
}
