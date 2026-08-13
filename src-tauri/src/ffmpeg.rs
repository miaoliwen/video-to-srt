use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FfmpegError {
    #[error("ffmpeg binary not found. Please ensure ffmpeg.exe is bundled in src-tauri/binaries/.")]
    NotFound,
    #[error("ffmpeg failed (exit code {code:?}): {stderr}")]
    Failed { code: Option<i32>, stderr: String },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Locate the bundled ffmpeg.exe:
/// 1. resource_path/binaries/<exe> (production bundle, beside app exe)
/// 2. <exe>_x64.exe (Tauri sidecar convention)
/// 3. Dev-only PATH lookup (NOT used in production builds)
pub fn locate_ffmpeg() -> Result<PathBuf, FfmpegError> {
    let exe_name = if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };

    if let Ok(resource_dir) = std::env::var("TAURI_BUNDLE_RESOURCE_DIR") {
        let p = Path::new(&resource_dir).join("binaries").join(exe_name);
        if p.exists() {
            return Ok(p);
        }
        let p2 = Path::new(&resource_dir)
            .join("binaries")
            .join(format!("{}_x64.exe", "ffmpeg"));
        if p2.exists() {
            return Ok(p2);
        }
    }

    // Dev fallback: walk up from CARGO_MANIFEST_DIR to find src-tauri/binaries
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let p = Path::new(&manifest).join("binaries").join(exe_name);
        if p.exists() {
            return Ok(p);
        }
    }

    // Current working dir fallback (for `tauri dev`)
    if let Ok(cwd) = std::env::current_dir() {
        let p = cwd.join("src-tauri").join("binaries").join(exe_name);
        if p.exists() {
            return Ok(p);
        }
        let p2 = cwd.join("binaries").join(exe_name);
        if p2.exists() {
            return Ok(p2);
        }
    }

    // PATH fallback: ONLY allowed in dev builds (when CARGO_MANIFEST_DIR is set).
    // In production, this is disabled to prevent binary hijacking.
    if std::env::var("CARGO_MANIFEST_DIR").is_ok() {
        if let Ok(p) = which::which(exe_name) {
            return Ok(p);
        }
    }

    Err(FfmpegError::NotFound)
}

/// Extract mono 16kHz WAV audio from a media file to the given output path.
/// Uses PCM s16le for compact payload that ASR services love.
/// Timeout: 5 minutes to prevent permanent blocking.
pub async fn extract_audio(ffmpeg: &Path, input: &Path, output: &Path) -> Result<(), FfmpegError> {
    if let Some(parent) = output.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let ffmpeg = ffmpeg.to_path_buf();
    let input = input.to_path_buf();
    let output = output.to_path_buf();

    // kill_on_drop(true): if the timeout fires, the future owning the child is
    // dropped and tokio kills ffmpeg — a plain timeout on `output()` would
    // leave it running and keep writing the output file.
    let child = tokio::process::Command::new(&ffmpeg)
        .arg("-y")
        .arg("-i").arg(&input)
        .arg("-vn")
        .arg("-ac").arg("1")
        .arg("-ar").arg("16000")
        .arg("-acodec").arg("pcm_s16le")
        .arg("-f").arg("wav")
        .arg(&output)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| FfmpegError::Failed { code: None, stderr: e.to_string() })?;

    let status = match tokio::time::timeout(Duration::from_secs(300), async {
        child.wait_with_output().await
    })
    .await
    {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            return Err(FfmpegError::Failed { code: None, stderr: e.to_string() });
        }
        Err(_) => {
            // Drop the partial output file ffmpeg may have written.
            let _ = std::fs::remove_file(&output);
            return Err(FfmpegError::Failed {
                code: None,
                stderr: "ffmpeg 执行超时（5分钟），文件可能过大或已损坏".into(),
            });
        }
    };

    if status.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&status.stderr).to_string();
        // Remove the partial output file so it can't be mistaken for a valid
        // extraction result later.
        let _ = std::fs::remove_file(&output);
        Err(FfmpegError::Failed { code: status.status.code(), stderr })
    }
}

/// Probe a media file and return its duration in seconds (best-effort).
pub fn probe_duration(path: &Path) -> Option<f64> {
    if let Ok(ffprobe) = locate_ffprobe() {
        let out = Command::new(ffprobe)
            .arg("-v").arg("error")
            .arg("-show_entries").arg("format=duration")
            .arg("-of").arg("default=noprint_wrappers=1:nokey=1")
            .arg(path)
            .output()
            .ok()?;
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            return s.parse().ok();
        }
    }
    None
}

fn locate_ffprobe() -> Result<PathBuf, FfmpegError> {
    let name = if cfg!(target_os = "windows") { "ffprobe.exe" } else { "ffprobe" };
    if let Ok(resource_dir) = std::env::var("TAURI_BUNDLE_RESOURCE_DIR") {
        let p = Path::new(&resource_dir).join("binaries").join(name);
        if p.exists() { return Ok(p); }
        // Also try _x64 variant
        let p2 = Path::new(&resource_dir)
            .join("binaries")
            .join(format!("{}_x64.exe", "ffprobe"));
        if p2.exists() { return Ok(p2); }
    }
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let p = Path::new(&manifest).join("binaries").join(name);
        if p.exists() { return Ok(p); }
    }
    if let Ok(cwd) = std::env::current_dir() {
        let p = cwd.join("src-tauri").join("binaries").join(name);
        if p.exists() { return Ok(p); }
    }
    // PATH fallback: ONLY in dev builds
    if std::env::var("CARGO_MANIFEST_DIR").is_ok() {
        if let Ok(p) = which::which(name) {
            return Ok(p);
        }
    }
    Err(FfmpegError::NotFound)
}
