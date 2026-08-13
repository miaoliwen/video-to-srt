# 对抗式代码审查 #2 — 最终汇总报告

> 审查时间：2026-08-13
> 审查范围：本轮未提交变更（whisper 模型检测/下载、FFmpeg 完整版下载与精简版双 tier、probe_format、
> 图标替换、save_srt 路径校验修复）＋ 对 2026-07-21 首轮 45 个问题的修复状态复核
> 审查方法：单 Agent 逐文件审查 + 调用链追踪 + 依赖源码/API 行为验证（@tauri-apps/api、tauri-plugin-store、
> Tauri 2 窗口 drag-drop 行为）+ `cargo test`（22 通过）+ `npm run typecheck`（通过）

---

## 问题总览

| 严重度 | 数量 | 类型 |
|--------|------|------|
| **HIGH** | 1 | 新功能失效（拖拽上传） |
| **MEDIUM** | 7 | 新增代码正确性/鲁棒性（2 个为可见错误） |
| **LOW** | 5 | 边缘/加固项 |
| 遗留未修复 | 6 | 首轮 HIGH/MEDIUM/LOW 复核仍开放 |

---

## 一、新发现问题（本轮新增代码）

### 1. [HIGH] 拖拽上传在 Tauri 2 下静默失效
- **文件**: `src/App.tsx`（`handleDrop`）、`src-tauri/tauri.conf.json`
- **问题**: 双重失效：
  1. `tauri.conf.json` 的窗口配置未设 `dragDropEnabled: false`（默认 `true`）。Tauri 2 在 OS 层拦截文件拖放并改发 `tauri://drag-drop` 事件，**HTML5 的 `onDrop` 根本不触发**。
  2. 即使触发，代码依赖 `File.path`（Tauri v1 注入的非标准属性）。Tauri v2 已移除该注入，`first.path` 恒为 `undefined`，处理函数直接 `return`。
- **影响**: README 主打的「拖拽上传」功能实际不可用，且无任何报错，用户只会看到「什么都没发生」。
- **修复**: 二选一——(a) 窗口配置 `"dragDropEnabled": false` 并改走 `getCurrentWebview().onDragDropEvent`（需补 `core:webview:default` 权限）；(b) 引入 `tauri-plugin-drag-drop` 社区插件。
- **状态（2026-08-13 修复）**: 已改用 `getCurrentWebview().onDragDropEvent`（`@tauri-apps/api/webview` v2 原生 API，drop 事件直接带 `paths`），彻底移除 `File.path` 与 HTML5 `onDrop`/`onDragOver`/`onDragLeave`。拖拽高亮由 `enter`/`leave` 事件驱动，扩展名白名单保留。

### 2. [MEDIUM] Whisper 模型下载目录拼接错误，且与文档描述不一致
- **文件**: `src/App.tsx`（`const targetDir = \`${dir}models\``）
- **问题**: `appDataDir()` 返回 `${dataDir}/${bundleIdentifier}` = `%APPDATA%\com.vibecoding.video-to-srt`，**无尾部分隔符**。拼接后得到 `%APPDATA%\com.vibecoding.video-to-srtmodels`（缺 `\`）。功能上能用（`create_dir_all` 会建目录），但路径丑陋且与文档冲突：
  - README/AGENTS 宣称模型在 `%APPDATA%/字幕生成工作台/models/`；
  - FFmpeg 完整版（Rust 侧硬编码）在 `%APPDATA%/字幕生成工作台/ffmpeg-full/`；
  - 实际 settings.json（tauri-plugin-store 相对路径）在 `%APPDATA%/com.vibecoding.video-to-srt/`。
  - **三个数据目录基座并存**，文档与实现互相矛盾。
- **修复**: 用 `join(appDataDir(), "models")`（`@tauri-apps/api/path` 已导出 `join`）；并统一文档口径（建议全部收敛到 `appDataDir()`，Rust 侧改用 `app.path().app_data_dir()` 而非硬编码 APPDATA+产品名）。
- **状态（2026-08-13 修复）**: 已用 `await join(appDataDir(), "models")` 拼接，消除 `video-to-srtmodels` 错误目录。文档口径统一（收敛到 appDataDir）暂未做，属可选重构。

### 3. [MEDIUM] 云端 Qwen 模式：字幕预览时间戳全部为 00:00.0
- **文件**: `src-tauri/src/lib.rs`（`transcribe`）、`src/App.tsx`（预览渲染）
- **问题**: 云端路径 `transcribe_file` 返回的 `SubtitleSegment` 全部 `start=0, end=0`（等分时间轴只在 `build_srt` 内生成）。`transcribe` 命令把原始 `segments` 原样返回，前端直接 `fmt(seg.start) → fmt(seg.end)` 渲染，导致**每行预览都是 00:00.0 → 00:00.0**；只有 SRT 原文/导出文件时间轴正确。
- **影响**: 主要预览视图对云端模式完全失真，用户会误以为时间轴坏了。
- **修复**: 前端改用 SRT 原文解析，或后端把 `build_srt` 等分后的时间轴回填到返回的 segments。
- **状态（2026-08-13 修复）**: 已把 `build_srt` 的等分逻辑提取为 `srt::distribute_timestamps`，`transcribe` 命令返回前对 segments 应用等分（云端补时间戳；本地真实时间戳的 cue 不受影响），预览与 SRT 导出一致。

### 4. [MEDIUM] ffprobe 缺失/损坏时对所有视频误报「需要下载完整版」
- **文件**: `src-tauri/src/ffmpeg.rs`（`probe_format` / `run_ffprobe`）
- **问题**: `run_ffprobe(...).unwrap_or_default()` 把「ffprobe 定位失败/执行失败」全部吞成空字符串 → `container=""`、`codec=""` → `supported=false`。前端把「探测失败」渲染成「此格式需要完整编解码器支持」，误导用户去下载完整版（实际是二进制缺失或路径问题）。
- **修复**: 区分「探测失败（Err）」与「确实不支持（Ok + supported=false）」；`probe_format` 在 locate 失败时直接返回 `Err`，前端对 Err 显示「探测失败」而非「格式不支持」。
- **状态（2026-08-13 修复）**: `run_ffprobe` 改为传播错误（`locate_ffprobe` 失败/ffprobe 非零退出即 Err）；无音频流（codec 探测空）仍视为 unsupported。前端 `probeVideo` catch 时写日志「格式探测失败」而非误报需要下载完整版。

### 5. [MEDIUM] 子进程超时后不 kill，留下僵尸进程与残留文件
- **文件**: `src-tauri/src/asr.rs`（`transcribe_local`）、`src-tauri/src/lib.rs`（`extract_audio`）
- **问题**: 两处均用 `timeout(…).await` 包裹子进程执行。超时只放弃等待：
  - `spawn_blocking` + `std::process::Command::output()`（whisper）——阻塞线程不可取消，**whisper-cli 继续在后台跑**，可能持续占用 CPU；
  - `tokio::process::Command::output()`（ffmpeg）——`tokio::process::Child` 默认不 `kill_on_drop`，超时 drop future 后 **ffmpeg 进程仍在运行**。
  - 二者都会在 `%TEMP%` 留下 `whisper_out_*.srt` / 半成品 WAV。
- **修复**: 用 `tokio::process::Command` + `kill_on_drop(true)`（或显式 `child.kill()`），并在超时分支清理已知临时文件。
- **状态（2026-08-13 修复）**: `extract_audio`/`transcribe_local`/`probe_duration` 全部改用 `tokio::process::Command` + `kill_on_drop(true)` + 内部超时；超时分支清理半截 wav / `whisper_out_*.srt`。

### 6. [MEDIUM] 下载的 FFmpeg 无完整性校验即落盘执行
- **文件**: `src-tauri/src/lib.rs`（`download_ffmpeg_full`）
- **问题**: 从固定 GitHub URL（HTTPS）下载后仅做「>1MB」与「`ffmpeg -version` 能跑」两项冒烟检查。无 hash/签名校验，且：
  - 冒烟执行无超时（`std::process::Command::output()` 同步阻塞，损坏的 PE 可永久挂起 `spawn_blocking` 线程）；
  - 只冒烟 `ffmpeg.exe`，不冒烟 `ffprobe.exe`；
  - 7z 解压同样无超时。
- **影响**: 一旦发布源被劫持/归档损坏，应用会静默执行任意二进制（供应链风险）。
- **修复**: 固定 URL 旁路校验期望 SHA-256 并比对；冒烟加超时；ffprobe 一并冒烟。
- **状态（2026-08-13 修复）**: 已从 gyan.dev 官方获取 `ffmpeg-8.1.2-essentials_build.7z` 的 SHA-256（`e25b6826…3528f6`）硬编码比对，不匹配即删除并报错；冒烟 `-version` 改为 `tokio::process` + 20s 超时 + `kill_on_drop`。ffprobe 冒烟未加（解压时已检查两文件均存在，ffmpeg 可运行即视为归档有效）。

### 7. [MEDIUM] 云端 ASR 的 50MB / 180s 硬限制，超长视频必然失败
- **文件**: `src-tauri/src/asr.rs`（`transcribe_file`）
- **问题**: 16kHz mono s16le ≈ 31.25 KB/s，50MB 上限 ≈ **26.7 分钟音频**。更长视频提取成功后转写必然报「音频文件过大」，且该限制在 UI 中无任何前置提示。另：180s 是 reqwest 总超时（含上传+推理），50MB → base64 ≈ 67MB 请求体在弱网上传就可能超时。
- **修复**: 前端在选片后按时长预估并提前提示；超时拆分（connect/read 分离）或按文件大小动态放宽。
- **状态（2026-08-13 修复）**: 前端在 `extract_audio` 成功后、调用 ASR 前按 `duration_secs > 1638`（≈27.3 分钟/50MB）提前拦截并提示改本地模式或截断。180s 请求超时未动（够用，拆分属可选）。

### 8. [MEDIUM] 模型下载失败残留 `.downloading` 文件；成功后未持久化设置
- **文件**: `src-tauri/src/asr.rs`（`download_ggml_model`）、`src/App.tsx`（下载回调）
- **问题**:
  1. 下载中途出错（网络中断/HTTP 错误）时不删除 `{model}.downloading` 残留；下次重试会覆盖同名临时文件，但失败终止时磁盘留垃圾。
  2. 下载成功后仅 `setSettings({…, whisperModel: result})` 改内存 state，**未写入 store**。用户直接关掉设置弹窗（不点保存），重启应用后模型路径丢失，需重新下载。
- **修复**: 所有错误路径 `remove_file(tmp_path)`；成功后显式 `await setAsrSettings(...)`。
- **状态（2026-08-13 修复）**: 下载主体移入内部 async 块，任何错误（创建/写入/网络/校验）统一 `remove_file(tmp_path)`；rename 失败同样清理。前端下载成功后 `await setAsrSettings(next)` 立即持久化。

---

## 二、首轮审查遗留、复核仍未修复的问题

| 首轮编号 | 严重度 | 问题 | 当前状态 |
|---|---|---|---|
| #8 | HIGH | `ffmpeg.rs::locate_binary` 仍保留 `which::which` PATH 回退，捆绑/完整版缺失时可被环境二进制劫持 | ✅ **2026-08-13 修复**：移除 PATH 回退，仅用捆绑/下载二进制；同时移除 `which` 依赖 |
| #10/#11 | HIGH | API Key 明文存 `settings.json` 且 `store.ts::getApiKey` 返回明文给前端，XSS/本地读取可窃取 | **未修复**（需 Rust 侧 keyring/DPAPI，属中期项） |
| #12 | MEDIUM | `probe_duration` 失败静默吞为 0.0 → 云端字幕时间轴整体失真，无用户提示 | ⚠️ 部分（2026-08-13）：改 async + 超时 + kill_on_drop，进程不再泄漏；吞 0.0 的前端提示仍未加 |
| #16 | MEDIUM | ASR 错误回显原始响应体（含转写文本，隐私泄露面） | **未修复** |
| #38 | LOW | `whisper_language` / `language` 无白名单校验 | **未修复** |
| #44 | LOW | 时长四舍五入可显示 60s；duration=0 与「探测失败」不区分 | **未修复**（次要） |

---

## 三、首轮问题已验证修复（本轮代码确认）

| 首轮编号 | 严重度 | 问题 | 修复状态 |
|---|---|---|---|
| #1 | BLOCKER | backend 非法值回退云端（隐私泄露） | ✅ `match` 显式枚举 `"local"`/`"cloud"\|"qwen"`，其余 `Err` |
| #2 | BLOCKER | `csp: null` | ✅ 已设严格 CSP（`default-src 'self'` + 白名单 connect-src） |
| #3 | HIGH | save_srt 任意路径写入 | ✅ 扩展名白名单（`eq_ignore_ascii_case`）+ 空父目录保护，且有回归测试 |
| #4 | HIGH | 音频输出文件固定名覆盖 | ✅ 存在时追加 pid+纳秒后缀 |
| #5 | HIGH | whisper 临时文件可预测/竞态 | ✅ pid+纳秒；单管线 `running` 守卫并发 |
| #6 | HIGH | whisper 子进程无超时 | ⚠️ 部分：有 300s 超时，但不 kill 进程（见新问题 #5） |
| #7 | HIGH | 整文件 base64 OOM | ✅ 50MB 上限 |
| #9 | HIGH | 事件 payload 未校验 | ✅ 前端 clamp 进度 + stage 白名单 |
| #22/#23/#24/#25 | MEDIUM | srt 时间戳 NaN/Inf/负值/上界 | ✅ `build_srt` 逐项防护 + 单调性钳制 |
| #26 | MEDIUM | SRT 仅 `\n` | ✅ 改 CRLF |
| #35 | MEDIUM | 缺 Unicode 规范化 | ✅ t2s 追加 NFKC |
| #31 | MEDIUM | 本地模式仍传 apiKey | ✅ 前端已置 null |
| #32 | MEDIUM | 异步无 try/catch | ✅ runPipeline/下载均包裹 |

---

## 四、LOW / 加固建议

1. **`probe_format` 非 UTF-8 路径**：`video_path.to_str().unwrap_or("")` 会把非常规路径（未配对代理项）退化成空参数传给 ffprobe。应直接 `Command::arg(&video_path)`（`AsRef<OsStr>`）。
2. **capabilities 多余权限**：`fs:allow-read-text-file` / `fs:allow-write-text-file` / `fs:allow-read-file` / `fs:allow-exists` 前端并未使用（无 `plugin-fs` 引用），属未用攻击面，建议删除。
3. **dev CSP 待验证**：`script-src 'self'` 可能拦截 Vite React Fast Refresh 的内联 preamble；若开发模式出现刷新失效，需配置 `devCsp`。
4. **`check_whisper_status` 非 Windows 平台**：`exe_ok` 强制 `.exe` 后缀，macOS/Linux 下恒 false（当前产品 Windows-only，仅记录）。
5. **7z 解压无超时**：损坏归档可挂起 `spawn_blocking` 线程（与新问题 #6 合并处理）。

---

## 五、审查结论

**整体评级：Request Changes**

- 首轮 2 个 BLOCKER 与多数 HIGH 已实质修复，且新增代码带单元测试保护（22 项全绿、typecheck 通过），方向正确。
- 但本轮新增的「拖拽上传失效（HIGH，主打功能）」需要立即处理；「模型下载目录拼接」「云端预览时间轴失真」是用户可见错误；超时不 kill、下载二进制无校验属于稳定性/供应链风险。
- 安全遗留（API Key 明文、PATH 回退）仍未处理，与 CSP 修复后的整体安全水位不匹配。

### 修复执行记录（2026-08-13 已完成）

已按上述顺序完成 8 项新问题 + 1 项首轮遗留（#8 which PATH 回退）的修复：

1. **拖拽上传** → `getCurrentWebview().onDragDropEvent`（Tauri 2 原生路径）
2. **模型下载目录** → `join(appDataDir(), "models")`
3. **云端预览时间轴** → `srt::distribute_timestamps` 回填
4. **ffprobe 吞错** → `run_ffprobe` 传播错误，前端区分提示
5. **子进程超时** → 三处全部 `kill_on_drop` + 超时清理临时文件
6. **FFmpeg 下载校验** → gyan.dev 官方 SHA-256 硬编码比对 + 冒烟 20s 超时
7. **云端 50MB 上限** → 前端 1638s 前置拦截提示
8. **模型下载残留/持久化** → 错误路径统一清理 + 成功后 `setAsrSettings`
9. **#8 PATH 回退** → 移除 `which` 依赖与回退分支

验证：`cargo test` 22 项全绿、`npm run typecheck` 通过、`npm run build` 成功；clippy 仅剩 4 个既有警告（非本次引入）。

### 建议后续

1. **中期**：API Key 移入 Rust 侧（keyring/DPAPI）——#10/#11 仍开放，安全水位与此前修复不匹配。
2. **中期**：`probe_duration` 吞 0.0 时前端提示（#12 部分修复，提示未加）。
3. **可选**：三个数据目录基座收敛到 `appDataDir()`；ASR 错误响应去隐私化（#16）。
