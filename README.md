# 字幕生成工作台

一个 Windows 桌面端应用：将视频自动拆分音频 → 调用 Qwen ASR 识别 → 生成带时间戳的 SRT 字幕文件 → 一键导出下载。

## 技术栈

| 层 | 技术 |
|---|---|
| 桌面框架 | Tauri 2 (Rust 后端 + 系统 WebView) |
| 前端 | React 19 + TypeScript + Vite + Tailwind CSS 4 |
| 音频抽取 | FFmpeg（捆绑 `src-tauri/binaries/`） |
| 语音识别 | 阿里云百炼 DashScope · OpenAI 兼容模式 · `qwen3-asr-flash` |
| 配置持久化 | `tauri-plugin-store` |

## 工作流

```
视频文件 (mp4/mov/mkv/...)
   │
   ▼ ffmpeg (16 kHz · mono · PCM s16le WAV)
audio.wav
   │
   ├─▶ [云端] Qwen ASR (DashScope Chat Completions, base64 data URI)
   │      → 纯文本 → 按标点切分为分段 → 等分时间戳
   │
   └─▶ [本地] whisper-cli.exe (-osrt)
          → 直接输出带真实时间戳的 SRT → 解析为分段
   │
   ▼ 繁→简转换 (convert.rs 精选高频映射)
   │
   ▼ srt::build_srt (HH:MM:SS,mmm)
字幕.srt ──▶ 浏览器下载 (UTF-8 BOM)  /  保存对话框导出
```

## 准备

1. 安装 Rust（[rustup.rs](https://rustup.rs)）。
2. 安装 Node.js ≥ 18。
3. 把 `ffmpeg.exe` 与 `ffprobe.exe` 放到 `src-tauri/binaries/` 目录（任意完整版 ≥ 5.0 即可）。
   也可以选择发布时通过系统的 PATH 提供，但建议捆绑以便开箱即用。
4. （开发 Windows）安装 [Microsoft Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) 与 WebView2 Runtime。

## 配置 API Key

首次运行点击右上角"⚙ 设置"，填入阿里云百炼 API Key（`sk-...`）。
密钥保存在 Windows 凭据管理器（Credential Manager），不会写入前端可读的配置文件，也不会外发。

## 开发

```powershell
npm install
npm run tauri dev
```

## 打包

```powershell
npm run tauri build
```

产物在 `src-tauri/target/release/bundle/` 下，NSIS (`.exe`) 与 MSI (`.msi`) 两种。

## 目录速览

```
.
├── .editorconfig              # 编辑器统一规范（缩进/换行/编码）
├── LICENSE                    # MIT
├── README.md
├── AGENTS.md                  # 开发约定与架构说明
├── index.html
├── package.json
├── vite.config.ts
├── tsconfig.json
├── docs/
│   └── audit/                 # 代码审查归档报告（历史参考）
├── src/                       # 前端 (React + Tailwind)
│   ├── App.tsx
│   ├── main.tsx
│   ├── index.css
│   └── store.ts
└── src-tauri/
    ├── Cargo.toml
    ├── tauri.conf.json
    ├── binaries/              # 放置 ffmpeg.exe / ffprobe.exe
    ├── capabilities/default.json
    └── src/
        ├── main.rs
        ├── lib.rs             # Tauri 命令注册与管线编排
        ├── ffmpeg.rs          # ffmpeg 定位、音频抽取与时长探测
        ├── asr.rs             # Qwen ASR 云端 + 本地 Whisper.cpp 双后端
        ├── convert.rs         # 繁→简高频字映射（Whisper 多语言模型输出修正）
        ├── srt.rs             # SRT 拼接、时间格式与等分时间戳
        └── encoding_tests.rs  # 中文 UTF-8 BOM 往返集成测试
```

## 开发规范

- 详细的架构说明与代码约定见 [`AGENTS.md`](./AGENTS.md)。
- 编辑器规范由 `.editorconfig` 统一（UTF-8 / LF / 通用 2 空格，Rust 与 TOML 4 空格）。
- 常用脚本：
  - `npm run typecheck` — 仅做 TypeScript 类型检查（`tsc --noEmit`）
  - `npm run fmt:rust` — 用 rustfmt 格式化 Rust 代码
  - `npm run lint:rust` — 运行 clippy 静态检查
  - `npm run test:rust` — 运行 Rust 单元/集成测试
- 本项目基于 [MIT License](./LICENSE) 开源。

## 备注

- 设置面板（右上角"⚙ 设置"）可切换 ASR 后端：
  - **Qwen ASR（云端）**：默认模型 `qwen3-asr-flash`，返回纯文本，由 `asr::split_into_cues` 按句末标点切分，再由 `srt::build_srt` 在总时长内等分时间戳。
  - **本地 Whisper.cpp**：指定 `whisper-cli.exe` 与 ggml 模型路径，完全离线，输出带真实时间戳的 SRT。
- 所有 ASR 输出经过 `convert::t2s` 繁→简转换（精选高频字映射），修正 Whisper 多语言模型偶尔输出繁体的问题。
- 导出的 SRT 文件带 UTF-8 BOM，确保中文在记事本和播放器中正确显示。
- 若 DashScope 端点不可用，请确认 API Key 已开通「百炼」平台模型服务权限。
