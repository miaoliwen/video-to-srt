# 对抗式代码审查 #3 — 最终汇总报告

> 审查时间：2026-08-13（第二轮）
> 审查范围：工作区未提交变更的全量复核（FFmpeg 精简版双 tier、完整版下载与 SHA-256 校验、whisper
> 模型检测/下载、拖拽上传、save_srt 校验、时间戳等分回填等）＋ 对 2026-07-21 首轮（45 项）与
> 2026-08-13 第二轮（8 项新问题 + 首轮遗留）修复状态的复核
> 审查方法：单 Agent 逐文件 + 调用链追踪 + 依赖 API 行为验证（GitHub Release API、Tauri v2 官方文档）
> + **实机验证**（运行打包的 lite ffmpeg/ffprobe 做 WAV 提取与探测）+ `cargo test`（22 通过）、
> `cargo clippy`（仅 4 个既有风格警告）、`npm run typecheck`（通过）

---

## 问题总览

| 严重度 | 数量 | 类型 |
|--------|------|------|
| **MEDIUM** | 5 | 主线程阻塞 / 失败路径残留 / 云端英文分句失效 / 下载源可达性 / 状态误报 |
| **LOW** | 12 | 边界与加固项 |
| 遗留未修复 | 6 | 前两轮 HIGH/MEDIUM/LOW 复核仍开放（多为已认可的中期项） |

未发现 BLOCKER / HIGH。核心管线（提取→识别→SRT）实测可用，前两轮的严重问题均已实质修复。

---

## 一、新发现问题（本轮新增/复核代码）

### 1. [MEDIUM] `probe_format` 在主线程同步执行 ffprobe，无超时，可冻结 UI
- **文件**: `src-tauri/src/ffmpeg.rs:177`（`run_ffprobe` 用 `std::process::Command::output()`）、
  `src-tauri/src/lib.rs`（`probe_format` 为同步 `#[tauri::command]`）
- **问题**: 按 Tauri v2 官方文档，「Commands without the async keyword are executed on the main
  thread」。`probe_format` 每次选片/拖拽即被调用，内部连续两次同步 `Command::output()` 阻塞主线程。
  对体积大、损坏或网络盘上的视频，ffprobe 可能耗时数秒 → **整个 UI 冻结**；对损坏的 ffprobe 二进制
  则无限阻塞。
- **背景**: 这正是第二轮修复 #5 已经处理过的问题类别（`extract_audio`/`probe_duration`/`transcribe_local`
  全部改为 `tokio::process` + 超时 + `kill_on_drop`），但**新增的 `probe_format` 又回归了同步阻塞写法**。
- **修复**: `probe_format` 改为 `async fn`，`run_ffprobe` 改用 `tokio::process::Command` +
  15s 超时 + `kill_on_drop`（复用 `probe_duration` 的模式）。

### 2. [MEDIUM] `check_ffmpeg_full_status` 同步执行 `ffmpeg -version`，无超时
- **文件**: `src-tauri/src/lib.rs:264-266`
- **问题**: 同为同步命令在主线程运行。应用**启动时**即调用一次（前端 `useEffect`）；若
  `%APPDATA%/字幕生成工作台/ffmpeg-full/ffmpeg.exe` 损坏或被杀软扫描挂起，`Command::output()`
  阻塞主线程，**启动即白屏/卡死**。
- **修复**: 改 async + `tokio::process` + 短超时（如 10s）+ `kill_on_drop`；或干脆改为纯文件存在性
  检查（版本号展示不 critical）。

### 3. [MEDIUM] `download_ffmpeg_full` 失败路径残留已复制的 exe，形成「假完整版」持久坏状态
- **文件**: `src-tauri/src/lib.rs`（复制循环 ~430-445、冒烟测试 ~455-470）
- **问题**: `cleanup()` 只删除 `_extract/` 与 `ffmpeg.7z`，**不清除已复制到
  `ffmpeg-full/` 的 `ffmpeg.exe`/`ffprobe.exe`**。当复制中途失败或冒烟测试失败（hash 已过但被杀软拦截
  执行等）时：
  - 磁盘残留 `ffmpeg-full/ffmpeg.exe`；
  - `check_ffmpeg_full_status` 据此报告「已安装完整版（未知版本）」；
  - `locate_binary` 优先返回 `ffmpeg-full` 目录 → **应用从此使用残缺/不可用的 ffmpeg**，且没有任何
    恢复路径（UI 认为已装好，不再显示下载按钮）。
- **修复**: 失败路径删除已复制的 exe；冒烟失败时删除 `ffmpeg-full/` 目录并回落到捆绑版；成功路径不变。

### 4. [MEDIUM] 云端 Qwen 英文转写不分句（缺 `.` 分隔符），整段成一条字幕
- **文件**: `src-tauri/src/asr.rs:187-215`（`split_into_cues`）
- **问题**: 切分标点仅含 `。！？!?;；` 与换行，**不含英文句号 `.`**。云端英文内容（qwen3-asr-flash
  输出含 `.` 的英文文本）会得到 1 个 cue → `distribute_timestamps` 把整段英文均摊到全片时长 → 字幕
  基本不可用。本地 whisper 模式有真实时间戳不受影响，但云端英文是主打路径。
- **修复**: 加入 `.`（注意排除小数点/缩写如 `3.14`、`e.g.`——可仅对「句号后跟空白或结尾」切分）；
  补充英文分句单测。

### 5. [MEDIUM] whisper 模型下载源 `huggingface.co` 在国内网络不可达
- **文件**: `src-tauri/src/asr.rs:347`
- **问题**: 产品面向国内用户（阿里云百炼），但模型下载硬编码
  `https://huggingface.co/...`（HF 域名在国内需代理）。实测本环境直连 HF 无响应。点击「开始下载」
  将大概率报「模型下载失败: error sending request」，且无任何提示告诉用户是网络原因。
- **修复**: 提供镜像源（如 `https://hf-mirror.com`，可用环境变量 `HF_ENDPOINT` 或失败自动回退）；
  或在 UI 明确提示需要代理/手动放置模型；README 补充说明。

---

## 二、LOW / 加固建议

1. **`check_whisper_status_impl` 目录冒充 .exe**（`asr.rs:283-292`）：`exe_ok` 用 `p.exists()`
   未校验 `is_file()`——名为 `whisper-cli.exe` 的**目录**会显示绿勾；`LocalConfig::validate` 同样只查
   `exists()`，运行期才报错。改为 `is_file()`。
2. **whisper `.txt` 临时文件残留**（`asr.rs` `transcribe_local`）：whisper.cpp 的 `-osrt` 会同时写出
   `{out_base}.txt`（`-otxt` 默认开启），代码只清理 `.srt`，每次本地识别在 `%TEMP%` 留一个
   `whisper_out_*.txt`。失败/超时路径同样残留。改为同时清理 `.txt`（或加 `-otxt false`）。
3. **`save_srt` 扩展名校验的残余面**（`lib.rs`）：仅 `.srt` 扩展名 + 父目录创建，无目录范围限制——
  可写任意位置任意 `.srt`；且 Windows 上既存 `.srt` 符号链接/交接点可把写入重定向到任意目标文件。
  与弹窗结合是当前认可的设计，但建议记录残余风险（理想方案：Rust 侧记录用户所选目录并做前缀校验，
  对「已存在目标」用 canonicalize 二次校验）。
4. **`probe_format` 非 UTF-8 路径退化**（`ffmpeg.rs:216/228`）：`video_path.to_str().unwrap_or("")`
   仍把非常规路径退化为空参传给 ffprobe。直接 `Command::arg(&path)`。
5. **capabilities 多余权限**（`capabilities/default.json`）：`fs:allow-read-text-file` /
   `fs:allow-write-text-file` / `fs:allow-read-file` / `fs:allow-exists` 前端均未使用，属未用攻击面。
6. **下载进度事件 payload 未校验**（`App.tsx`）：`whisper-download-progress` /
   `ffmpeg-download-progress` 直接 `setState(e.payload)`，若 `percent` 缺失/非数，进度条宽与
   `.toFixed()` 会崩（事件监听器内异常）。`pipeline-progress` 已做 clamp/白名单，这两处未同步。
7. **API Key 前后端校验不一致**（`App.tsx` `runPipeline`）：前端仅判 `!apiKey`，纯空白 key 会走到
   Rust 才报「missing api key」。前端 `trim()` 后判空。
8. **两个下载命令无并发防护**（`lib.rs` `tmp_7z`、`asr.rs` `{model}.downloading` 固定名）：双请求
   （连点/多窗口）互相覆盖临时文件。前端按钮有禁用，但 Rust 侧无锁，属纵深加固。
9. **wmv 在文件选择器中但 lite 不支持**（`App.tsx` `VIDEO_EXTENSIONS` 含 `wmv`，lite 未启用 asf
   demuxer）：选中 wmv 会走「需要完整版」横幅（设计内），但选择器文案与提示不一致，建议从选择器移除
   或明确标注。
10. **`extract_audio` 在视频目录累积 `_16k_mono_*.wav`**：重跑同名视频每次生成新后缀文件、从不清理，
    长期污染用户视频目录。建议提取到 `%TEMP%`（音频是中间产物）或成功后清理旧文件。
11. **前端日志数组无界增长**（`App.tsx` `setLog`）：长会话内存增长，可截断到最近 N 条。
12. **时长显示**（`App.tsx`）：`Math.round(duration%60)` 可显示 60s；`duration=0` 与「探测失败」
    不区分（首轮 #44，仍未修）。

---

## 三、前两轮问题复核结论

### 第二轮 8 项新问题 —— 已修复且本轮复核通过

| 第二轮编号 | 问题 | 复核结论 |
|---|---|---|
| #1 拖拽上传失效 | | ✅ `getCurrentWebview().onDragDropEvent`（Tauri 2 原生，`core:default` 覆盖权限），drop 取 `paths[0]` + 扩展名白名单，enter/leave 驱动高亮 |
| #2 模型下载目录拼接 | | ✅ `join(appDataDir(), "models")` |
| #3 云端预览时间戳失真 | | ✅ `srt::distribute_timestamps` 在 `transcribe` 内回填，预览与 SRT 一致；本地真实时间戳不受影响 |
| #4 ffprobe 吞错 | | ✅ `run_ffprobe` 传播错误，前端区分「探测失败」与「不支持」 |
| #5 子进程超时不 kill | | ⚠️ 三处主路径已修复（`kill_on_drop` + 超时 + 清理）；但**新增的 `probe_format`/`check_ffmpeg_full_status` 又回归同步阻塞**（本轮新问题 #1/#2） |
| #6 FFmpeg 下载无校验 | | ✅ SHA-256 硬编码**与 GitHub Release API 返回的官方 asset digest 一致**（`e25b6826…3528f6`），URL 存在且指向 8.1.2 tag（不会漂移）；冒烟带 20s 超时 |
| #7 云端 50MB 硬限制 | | ✅ 前端按时长 1638s 前置拦截 + Rust 侧 50MB 上限双保险 |
| #8 下载残留/未持久化 | | ✅ 下载主体错误路径统一清理 `.downloading`；成功后前端 `setAsrSettings` 立即持久化 |

### 首轮 45 项 —— BLOCKER 与多数 HIGH 已修复；遗留如下（均为已认可的中期项）

| 首轮编号 | 严重度 | 问题 | 状态 |
|---|---|---|---|
| #10/#11 | HIGH | API Key 明文存 settings.json，前端可读 | **未修复**（需 keyring/DPAPI，中期项） |
| #12 | MEDIUM | `probe_duration` 失败静默 0.0，无前端提示 | **未修复**（部分：进程已不泄漏；提示未加） |
| #16 | MEDIUM | ASR 错误回显原始响应体 | **未修复** |
| #38 | LOW | `language`/`whisper_language` 无白名单 | **未修复** |
| #44 | LOW | 时长四舍五入 60s / duration=0 不区分 | **未修复**（见本轮 LOW #12） |
| #27 | MEDIUM | 子进程 stdout/stderr 全量缓存 | ⚠️ whisper 已用 `--no-prints` 缓解；ffmpeg 提取仍 piped 全量收集（长视频 stderr 数百 KB，可接受） |

---

## 四、实测验证记录（本轮新做）

1. **SHA-256 校验通过**：`GET https://api.github.com/repos/GyanD/codexffmpeg/releases/tags/8.1.2`
   返回 `ffmpeg-8.1.2-essentials_build.7z` 的 `digest: sha256:e25b6826…3528f6`，与
   `lib.rs::FFMPEG_FULL_SHA256` 完全一致；asset 存在（33.8MB，browser_download_url 与代码 URL 相同）。
2. **lite 二进制与白名单一致**：运行打包的 `ffmpeg.exe -version`，configure 参数
   （`--enable-demuxer=mov,mp3,ogg,wav,matroska,avi,flv`、`--enable-decoder=…`）与
   `LITE_SUPPORTED_CONTAINERS`/`LITE_SUPPORTED_CODECS` 完全一致（`lite_whitelist_matches_build_script`
   单测只校验脚本、不校验实装二进制，这里补验了二进制本身）。
3. **提取/探测端到端可用**：Node 生成 2s 16kHz mono WAV → lite `ffprobe` 读出 duration=2.0 →
   lite `ffmpeg -y -i … -vn -ac 1 -ar 16000 -acodec pcm_s16le -f wav` 重采样成功 → ffprobe 验证
   `pcm_s16le / 16000 Hz / mono`。管线核心路径无碍。
4. **构建检查**：`cargo test` 22 通过；`cargo clippy --all-targets` 仅 4 个既有风格警告
   （冗余闭包 ×1、needless_borrows ×2、函数参数过多 ×1，均非本次引入）；`npm run typecheck` 通过。
5. **拖拽方案**：`onDragDropEvent` 为 Tauri 2 官方 API（dragDropEnabled 默认 true，OS 层事件
   直接送达 webview），与首轮 HIGH 修复一致。

---

## 五、审查结论

**整体评级：Approve with nits（可合入，建议顺手修 MEDIUM）**

- 前两轮 2 个 BLOCKER、绝大多数 HIGH 已实质修复且本次复核通过；下载完整性（SHA-256）、子进程
  生命周期（kill_on_drop）、时间戳一致性、拖拽上传均已就位，且有单测与实测背书。
- 本轮未发现新的 BLOCKER/HIGH。最值得优先处理的是两个**主线程同步阻塞**（`probe_format`、
  `check_ffmpeg_full_status`，均为新增代码对既有模式的回归）与 `download_ffmpeg_full` 失败路径的
  **残留 exe 假完整版**问题。
- 遗留项（API Key 明文、错误响应隐私、语言白名单）与前两轮结论一致，属中期加固。

### 建议修复顺序

1. `probe_format` / `check_ffmpeg_full_status` 改 async + tokio + 超时（回归统一模式）
2. `download_ffmpeg_full` 失败路径清理已复制 exe / 回落到捆绑版
3. `split_into_cues` 支持英文句号分句 + 单测
4. whisper 模型下载镜像/可达性提示；`check_whisper_status`/`validate` 补 `is_file()`
5. 其余 LOW 视排期

### 建议后续（中期，沿袭前两轮）

- API Key 移入 Rust 侧（keyring/DPAPI，首轮 #10/#11）
- 三个数据目录基座收敛到 `appDataDir()`；ASR 错误响应去隐私化（首轮 #16）
- `probe_duration` 失败时前端明确提示（首轮 #12）
