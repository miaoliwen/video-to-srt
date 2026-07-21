# AGENTS.md — 字幕生成工作台

## 定位

Windows 桌面应用：视频 → FFmpeg 提取 16kHz mono WAV → ASR 语音识别（云端 Qwen / 本地 Whisper.cpp）→ 繁简转换 → 生成 SRT 字幕文件。

## 启动

```powershell
npm install
npm run tauri dev      # 开发（前端 localhost:1420 + Rust 后端）
npm run tauri build    # 打包 NSIS 安装包 → src-tauri/target/release/bundle/
```

前置：Rust toolchain、Node.js ≥ 18、VS C++ Build Tools、WebView2 Runtime。
FFmpeg 已捆绑在 `src-tauri/binaries/`（ffmpeg.exe + ffprobe.exe，~165MB 各）。

## 技术栈

- 桌面框架：Tauri 2（Rust 后端 + 系统 WebView）
- 前端：React 19 + TypeScript + Vite 7 + Tailwind CSS 4
- ASR 云端：阿里云百炼 DashScope OpenAI 兼容模式，模型 `qwen3-asr-flash`
- ASR 本地：whisper-cli.exe（用户自行指定路径 + ggml 模型）
- 持久化：tauri-plugin-store → `%APPDATA%/字幕生成工作台/settings.json`
- 音频：FFmpeg（PCM s16le, 16kHz, mono）

## 目录与约定

```
src/                  前端（React 单页）
  App.tsx             主界面 + 管线调用 + 设置弹窗
  store.ts            tauri-plugin-store 封装（API Key + ASR 设置）
src-tauri/
  src/lib.rs          Tauri 命令注册、管线编排、事件发射
  src/ffmpeg.rs       ffmpeg/ffprobe 定位与调用
  src/asr.rs          双后端 ASR（Qwen HTTP / whisper-cli 子进程）
  src/convert.rs      繁→简精选映射（~200 高频字）
  src/srt.rs          SRT 格式化、等分时间戳
  src/encoding_tests.rs  中文 UTF-8 BOM 往返测试
  binaries/           ffmpeg.exe / ffprobe.exe（打包资源）
  capabilities/       Tauri 权限声明
```

约定：
- Rust 错误统一用 thiserror 枚举，命令层 `.map_err(|e| e.to_string())` 转前端。
- 前端进度通过 Tauri event `pipeline-progress` 推送，不轮询。
- SRT 导出始终带 UTF-8 BOM（save_srt 命令 + 前端 Blob 下载均加 BOM）。
- 繁简转换在 transcribe 命令内、build_srt 之前执行。

## 当前状态与下一步

- v0.1.0 功能完整：双后端 ASR、拖拽上传、进度条、字幕预览、导出/下载。
- 无 git 仓库（尚未 `git init`）。
- 潜在改进：Qwen 云端目前无真实时间戳（等分兜底），可考虑换用带时间戳的 ASR 模型或 API 参数；打包体积优化（FFmpeg 占 ~330MB）。
