# FFmpeg 双层架构设计

## 目标

- 安装包体积 ≤ 100MB
- 不影响音频转码性能
- 主流格式（MP4/MOV/MKV/AVI/WEBM）开箱即用
- 罕见格式通过下载完整版支持

## 架构概览

```
┌─────────────────────────────────────────────────────┐
│                    App 安装包                         │
│  ┌───────────────────────────────────────────────┐  │
│  │  FFmpeg Lite (捆绑)                            │  │
│  │  ffmpeg-lite.exe + ffprobe-lite.exe           │  │
│  │  体积：~30-40MB                                │  │
│  │  覆盖：主流容器 + 常用音频 codec                │  │
│  └───────────────────────────────────────────────┘  │
│  ┌───────────────────────────────────────────────┐  │
│  │  Tauri Rust 产物          ~10MB               │  │
│  └───────────────────────────────────────────────┘  │
│  ┌───────────────────────────────────────────────┐  │
│  │  前端资源                  ~2MB                │  │
│  └───────────────────────────────────────────────┘  │
│                          总计：~45MB                 │
└─────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────┐
│              用户下载（按需）                          │
│  ┌───────────────────────────────────────────────┐  │
│  │  FFmpeg Full (GitHub Releases)                │  │
│  │  ffmpeg.exe + ffprobe.exe                     │  │
│  │  体积：~165MB                                 │  │
│  │  存储：APPDATA/ffmpeg-full/                   │  │
│  └───────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────┘
```

## FFmpeg Lite 规格

### 来源

- 社区预构建（gyan.dev 或 btbn/ffmpeg-builds）
- 锁定版本，不追新
- 随 App 大版本更新时评估是否升级

### 支持的格式

| 类型 | 格式 |
|------|------|
| 容器 | mov, matroska, avi, flv, ogg, wav, mp3, webm |
| 音频解码 | aac, mp3, opus, vorbis, flac, pcm_* |
| 输出编码 | pcm_s16le（仅用于 WAV 输出） |
| 重采样 | swresample |

### 裁剪掉的模块

- 所有视频编码器（H.264/H.265/VP9/AV1 encode）
- 硬件加速（CUDA/VAAPI/DXVA）
- 滤镜系统（仅保留 format/resample 必需项）
- 字幕编码器
- 网络协议（http/rtmp/rtp 等）
- 图片编解码器

### 版本对齐

- Lite 与 Full 必须基于同一 FFmpeg 大版本（如 6.1.x 或 7.0.x）
- 确保行为一致，避免 Lite 探测说支持但 Full 处理时行为不同

## 后端设计

### 新增命令：`probe_format`

```rust
#[tauri::command]
fn probe_format(video_path: String) -> Result<FormatProbe, String>
```

**返回值：**
```typescript
interface FormatProbe {
  container: string;    // "mp4", "mkv", "avi", etc.
  codec: string;        // "aac", "opus", "mp3", etc.
  supported: boolean;   // Lite 是否支持（后端白名单判定）
}
```

**实现逻辑：**
1. 调用 ffprobe 探测容器格式 + 音频流 codec
2. 后端 `LITE_SUPPORTED_CODECS` 白名单比对，填充 `supported` 字段
3. 返回探测结果（前端无需维护白名单副本）

### 新增命令：`download_ffmpeg_full`

```rust
#[tauri::command]
async fn download_ffmpeg_full(app: AppHandle) -> Result<String, String>
```

**实现逻辑：**
1. 从 GitHub Releases 下载 ffmpeg.exe + ffprobe.exe
2. 存储到 `%APPDATA%/字幕生成工作台/ffmpeg-full/`
3. 通过 `ffmpeg-download-progress` 事件推送进度
4. 完成后返回存储路径

### 新增命令：`check_ffmpeg_full_status`

```rust
#[tauri::command]
fn check_ffmpeg_full_status() -> FFmpegFullStatus
```

**返回值：**
```typescript
interface FFmpegFullStatus {
  downloaded: boolean;
  path: string;
  version: string | null;
}
```

### FFmpeg 查找优先级

```rust
fn locate_ffmpeg() -> Result<PathBuf, FfmpegError> {
    // 1. 用户下载的完整版
    if let Some(full) = check_ffmpeg_full_dir() {
        return Ok(full);
    }
    // 2. 捆绑的精简版
    if let Some(lite) = locate_bundled_lite() {
        return Ok(lite);
    }
    // 3. 系统 PATH 兜底
    which("ffmpeg").map_err(|_| FfmpegError::NotFound)
}
```

### 文件存储结构

```
%APPDATA%/字幕生成工作台/
├── settings.json              # 现有
├── models/                    # 现有（whisper 模型）
│   └── ggml-base.bin
└── ffmpeg-full/               # 新增
    ├── ffmpeg.exe
    └── ffprobe.exe
```

## 前端设计

### 文件选择后自动探测

```
用户选择视频
    ↓
调用 probe_format(video_path)
    ↓
┌─────────────────────────────────────────┐
│  supported=true                         │
│  → 正常显示「开始识别」按钮              │
├─────────────────────────────────────────┤
│  supported=false                        │
│  → 显示黄色提示条：                      │
│    「此格式需要完整编解码器支持。         │
│     [下载完整版]」                       │
└─────────────────────────────────────────┘
```

### 设置面板新增卡片

```
┌─────────────────────────────────────────┐
│  编解码器支持                            │
│                                         │
│  当前：精简版（支持 MP4/MOV/MKV/AVI/    │
│        WEBM 常见音频格式）               │
│                                         │
│  如需支持更多格式（如 FLAC/OGG/罕见     │
│  编码），可下载完整版编解码器。           │
│                                         │
│  [下载完整版]                            │
│                                         │
│  下载进度：[████████░░░░] 67%            │
└─────────────────────────────────────────┘
```

### 下载完成后的状态同步

1. 下载完成事件触发
2. 若当前有已选文件，自动重新调用 `probe_format`
3. 若变为 supported，隐藏黄色提示条
4. 显示 toast：「当前格式现已支持，可以开始识别」

## 数据流

```
┌──────────┐    probe_format     ┌──────────┐
│  Frontend│ ──────────────────→ │  Backend │
│          │ ←────────────────── │          │
│          │   {container, codec, │          │
│          │    supported}        │          │
└──────────┘                     └──────────┘
     │                                │
     │  download_ffmpeg_full          │ ffprobe/ffmpeg
     │  ──────────────────→           │ ──────→
     │  ← progress events             │
     │                                │
     ▼                                ▼
┌─────────────────────────────────────────┐
│  GitHub Releases (下载源)               │
│  github.com/<owner>/<repo>/releases     │
└─────────────────────────────────────────┘
```

## 体积预估

| 组件 | 体积 |
|------|------|
| FFmpeg Lite (ffmpeg-lite + ffprobe-lite) | ~30-40MB |
| Tauri Rust 产物 | ~10MB |
| 前端资源 | ~2MB |
| NSIS 安装包开销 | ~3MB |
| **安装包总计** | **~45MB** |
| 下载完整版后占用（额外） | ~165MB |
| **安装 + 完整版总计** | **~210MB** |

## 维护策略

- **Lite 版**：随 App 版本更新，不单独追 FFmpeg 版本
- **Full 版**：发布到 GitHub Releases，用户自行决定下载时机
- **版本对齐**：Lite 与 Full 必须同大版本（如均为 6.1.x）
- **安全更新**：不追踪，仅大版本（6.x → 7.x）时评估升级

## 风险与缓解

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|----------|
| Lite 版不支持某些主流格式 | 中 | 高 | 白名单基于实际探测，设置中提供下载入口 |
| GitHub Releases 国内访问慢 | 中 | 中 | 可在设置中提供镜像源选项（后续迭代） |
| Lite/Full 版本行为差异 | 低 | 中 | 强制同大版本对齐 |
| 用户不知道需要下载完整版 | 中 | 高 | 选择文件时立即探测并提示 |

## 实现状态

- [x] 后端 `ffmpeg.rs`：新增 `probe_format` / `FormatProbe` / `LITE_SUPPORTED_CODECS` / `check_ffmpeg_full_dir`
- [x] 后端 `lib.rs`：新增 `probe_format` / `check_ffmpeg_full_status` / `download_ffmpeg_full` 三个命令
- [x] 后端 `Cargo.toml`：添加 `sevenz-rust` 依赖
- [x] 前端 `App.tsx`：文件选择后探测 / 提示条 / 设置面板卡片 / 下载进度
- [x] `LITE_SUPPORTED_CONTAINERS` / `LITE_SUPPORTED_CODECS` 与 `build-ffmpeg-lite.sh` 的 demuxer/decoder 白名单由单元测试强制一致（`lite_whitelist_matches_build_script`）
- [x] `download_ffmpeg_full`：解压后校验两个 exe 存在且可运行（`-version` 探测）；成功/失败路径均清理 `ffmpeg.7z` 与 `_extract/`；解压条目名做了路径穿越过滤（`safe_extract_name`）
- [x] 获取 FFmpeg Lite 二进制替换 `binaries/` 下的完整版（已用 `build-ffmpeg-lite.sh` 源码构建，见下）

### FFmpeg Lite 构建记录（2026-08-12）

- 方式：MinGW64 (gcc 15.2.0) + Git Bash 源码构建 FFmpeg 8.1.2（`build-ffmpeg-lite.sh`）
- 产物：`src-tauri/binaries/ffmpeg.exe`（3.28MB）+ `ffprobe.exe`（3.12MB），原 gyan 版各 ~97MB
- 配置：`--disable-everything` + 白名单（aac/mp3/opus/vorbis/flac/pcm_* 解码，pcm_s16le 编码，mov/mp3/ogg/wav/matroska/avi/flv 解封装，wav 封装），`-Os + --gc-sections + strip`
- 构建要点：
  - FFmpeg 8 的 `ffmpeg` CLI 依赖 **avfilter**（`ffmpeg_deps="avcodec avfilter avformat threads"`），`--disable-avfilter` + `--enable-ffmpeg` 会导致 CLI 被静默丢弃（此前失败根因）
  - `-ac/-ar` 重采样走 `aresample` 滤镜 + swresample，需 `--enable-avfilter --enable-filter=aresample`
  - 不能 `--disable-ffprobe` / `--disable-programs`，项目依赖 ffprobe 探测时长
  - Git Bash 从 PowerShell 启动时 PATH 不含 `/usr/bin`（mkdir 失败）；MinGW 发行版无 `make`，用 `mingw32-make`（需 sh 在 PATH）
  - Git Bash fork 子进程偶发 0xC0000142 瞬时死亡，configure 加 3 次重试
- 冒烟测试通过：h264+aac mp4 → `-vn -ac 1 -ar 16000 -acodec pcm_s16le -f wav` 转码成功；ffprobe 时长/流探测正常
- 体积：两 exe 合计 ~6.4MB，打包后安装包预计 ~20MB 量级（远低于 ≤100MB 目标）
- 旧完整版已备份至 `C:\Users\ROG\AppData\Local\Temp\opencode\backup-binaries\`（ffmpeg/ffprobe 各 97MB，供回归对比；如需保留可拷回，或改用 btbn/gyan 完整版作为"下载完整版"渠道来源）

## 后续迭代（不在 v1 范围）

- 镜像源加速下载（阿里云 OSS / 腾讯云 COS）
- FFmpeg 独立更新通道（不跟 App 版本）
- 自定义 FFmpeg 路径（高级用户）
- UPX 进一步压缩 Lite 版（若社区构建不够小）
