# 对抗式代码审查 — 第二轮报告（Round 2）

> 审查时间：2026-08-13
> 审查范围：lib.rs / asr.rs / srt.rs / ffmpeg.rs / convert.rs / App.tsx / store.ts / capabilities/default.json / tauri.conf.json / Cargo.toml
> 审查方式：单 Agent 对抗式深度审查（对照第一轮 45 项问题的修复状态逐项复核 + 全新路径挖掘）
> 基线：`6f8ed8d`（第一轮 BLOCKER/HIGH 与 MEDIUM 已修复之后的 HEAD）

---

## 问题总览（按严重程度）

| 严重度 | 数量 | 核心文件 |
|--------|------|----------|
| **BLOCKER** | 1 | lib.rs |
| **HIGH** | 3 | lib.rs, asr.rs, ffmpeg.rs, App.tsx |
| **MEDIUM** | 6 | asr.rs, lib.rs, capabilities |
| **LOW** | 15 | 多文件 |

**总计：25 个问题**（其中第一轮遗留未修复 12 个，本次新发现 13 个）

---

## BLOCKER（需立即修复）

### 1. `lib.rs:185` — save_srt 对不存在的文件 canonicalize 必然失败 → 导出功能整体回归

- **文件**: `src-tauri/src/lib.rs:185`
- **现象**: `p.canonicalize()` 对**尚不存在的文件**返回 `NotFound`，被 `map_err` 转成 `"路径无效: ..."`。保存对话框（save dialog）返回的几乎总是新文件名——文件尚不存在——因此主流程「导出…」对新建文件 **100% 失败**；只有覆盖已存在的 .srt 文件时才能成功。
- **根因**: 第一轮修复「任意路径写入」（HIGH #3）时引入的回归。`canonicalize` 只能用于已存在的路径，校验新文件的写入目标应 canonicalize **父目录**，而非文件本身。
- **复现**: 点击「导出…」→ 保存对话框输入新文件名 → 报 `路径无效: The system cannot find the file specified (os error 2)`。
- **修复建议**:
  ```rust
  // 1) 扩展名校验用原始路径（大小写不敏感）
  let ext_ok = p.extension()
      .and_then(|e| e.to_str())
      .map(|e| e.eq_ignore_ascii_case("srt"))
      .unwrap_or(false);
  if !ext_ok { return Err("只允许保存 .srt 文件".into()); }
  // 2) canonicalize 父目录（解析符号链接），再拼回文件名
  let parent = p.parent().unwrap_or_else(|| Path::new("."));
  let canonical_parent = parent.canonicalize().map_err(|e| format!("路径无效: {e}"))?;
  let final_path = canonical_parent.join(p.file_name().ok_or("路径无效")?);
  // 3) 之后 create_dir_all + 写 final_path
  ```
- **附带问题**: 大小写敏感——用户输入 `foo.SRT` 时 `extension() == Some("SRT") != Some("srt")` 被拒。

---

## HIGH（高优先级）

### 2. API Key 仍被前端明文持有 + 明文落盘（第一轮 HIGH #10/#11 未修复）

- **文件**: `src/store.ts:12` / `src/App.tsx:58` / `src/App.tsx:106`
- **现象**: `getApiKey()` 依然把明文密钥返回给 React state；设置弹窗提供「显示/隐藏」明文查看；`settings.json` 明文持久化。第一轮路线图明确要求「API Key 移除出前端，改用后端持有 + keyring」，但 commit `1acc890` 只做了 `api_key: Option<String>` 的类型调整，store.ts 一行未改。
- **风险**: WebView 一旦被注入（CSP 再严格也有纵深需求），明文密钥唾手可得；同时第三方可读到磁盘上的 settings.json。
- **修复建议**: 前端只保留 `hasApiKey: boolean`；密钥读写全部移到 Rust 命令内（命令从 store 读取，`transcribe` 内取用）；长期使用 Windows Credential Manager。

### 3. 「超时杀进程」是假超时，子进程实际泄漏（第一轮 HIGH #6/#8 修复无效，重新打开）

- **文件**: `src-tauri/src/asr.rs:315-328`（whisper）/ `src-tauri/src/ffmpeg.rs:82-95`（ffmpeg）
- **现象**:
  - **asr.rs**: `tokio::time::timeout` 包裹的是 `spawn_blocking + std::process::Command::output()`。超时后 JoinHandle 被丢弃，但 **blocking 任务会继续运行直到子进程退出**——whisper-cli 根本未被杀，继续占 CPU；若它之后正常退出，写出的临时 SRT 残留且永远不会被解析/清理。
  - **ffmpeg.rs**: `tokio::process::Command::output()` 超时后 future 被 drop，而 tokio `Child` 默认 `kill_on_drop = false`——**ffmpeg 进程继续运行并继续写输出文件**。应用已经报「超时」，5 分钟后文件却「写完」了。
- **影响**: 长时间运行的 ffmpeg/whisper 在超时场景下成为僵尸进程；重复触发可堆积多个进程同时吃 CPU/磁盘。
- **修复建议**: 统一改为 `tokio::process::Command::new(...).spawn()` 拿到 `Child`，`timeout(child.wait())`，超时后 `child.kill().await`（whisper 同理，去掉 spawn_blocking 或把 child 句柄传出来）。

### 4. 拖拽上传在 Tauri 2 中静默失效（新发现）

- **文件**: `src/App.tsx:130`（`handleDrop`）
- **现象**: `handleDrop` 依赖 HTML5 `File.path`（Tauri v1 注入的特性）。**Tauri 2 出于安全原因移除了该属性**，须通过 `getCurrentWebview().onDragDropEvent`（或后端 `drag-drop` 事件）获取真实路径。当前实现中 `first.path` 恒为 `undefined` → `if (!p) return;` 静默返回，拖入文件后 UI 无任何反馈。「拖拽上传」是 AGENTS.md 明确的核心工作流，功能已坏。
- **修复建议**: 监听 `getCurrentWebview().onDragDropEvent` 的 `drop` 分支取 `payload.paths[0]`（保留现有扩展名白名单校验）；或关闭 `dragDropEnabled` 走纯 HTML5 路径（但拿不到路径，不可取）。

---

## MEDIUM（中优先级）

### 5. 错误回显泄露转写内容（第一轮 MEDIUM #16 未修复）

- **文件**: `src-tauri/src/asr.rs:181`
- **现象**: JSON 解析失败时拼接 `raw={前400字符}` 回显原始响应——而 Qwen 的 200 响应体就是**完整音频转写文本**（可能含隐私对话内容），直接进入前端日志。非 2xx 时 `AsrError::Api { message: text }` 回显整个错误体。
- **修复建议**: 错误信息只保留错误类别 + 截断的响应摘要（去正文），或仅保留状态码与 request id。

### 6. capabilities 中无 scope 约束的宽泛 fs 权限（新发现）

- **文件**: `src-tauri/capabilities/default.json`
- **现象**: `fs:allow-read-text-file` / `fs:allow-write-text-file` / `fs:allow-read-file` / `fs:allow-exists` 这四条是无路径 scope 限制的宽松授权；而**前端代码从未使用 tauri-plugin-fs 的任何 API**（全部文件 I/O 在 Rust 侧完成）。属于纯攻击面：一旦 WebView 被注入，可任意读写文本文件。
- **修复建议**: 移除这四条，只保留 `fs:default`。

### 7. 提取的 WAV 中间文件不清理，磁盘持续膨胀（新发现）

- **文件**: `src-tauri/src/lib.rs:48-56`
- **现象**: 16kHz mono s16 WAV ≈ **115 MB/小时视频**，写在与视频同目录，转录完成后从不删除。反复处理大量视频 → 磁盘被中间产物填满；且视频位于只读卷/受限目录（如 DVD、只读共享）时直接失败。
- **修复建议**: 输出到系统临时目录（`temp_dir()`），任务结束（含失败路径）后清理；或在退出/下次运行时清扫。

### 8. 进度事件无 job_id，旧任务事件可污染 UI（第一轮 MEDIUM #13 未修复）

- **文件**: `src-tauri/src/lib.rs:19-25` / `src/App.tsx:78-94`
- **现象**: 事件 `pipeline-progress` 不带任务标识，前端全局 `listen` 无法区分来源。`running` 标志只能挡住常规 UI 路径，延迟/异常并发时旧任务事件会覆盖新任务状态。
- **修复建议**: `StepEvent` 增加 `job_id`（前端生成 UUID 传入各命令），前端按 id 过滤。

### 9. transcribe 未校验 audio_path 存在性（新发现）

- **文件**: `src-tauri/src/lib.rs:132-158`
- **现象**: `transcribe` 直接 `PathBuf::from(audio_path)` 交给 whisper/云端，无 `exists()` 校验。local 模式下文件缺失时 whisper-cli 报晦涩的自身错误；cloud 模式报 IO 错误——两条路径错误形态不一致、不友好。
- **修复建议**: 命令入口统一 `exists() + is_file()` 校验并返回中文错误。

---

## LOW（建议改进；含第一轮遗留）

| # | 文件 | 问题 |
|---|------|------|
| 10 | lib.rs:222 | `run(...).expect(...)` 启动 panic 点，release 配置 `panic=abort`（第一轮 #36/#37 未修复） |
| 11 | lib.rs:22 | `let _ = app.emit(...)` 事件发送失败静默（第一轮 #15 未修复） |
| 12 | asr.rs / ffmpeg.rs | 子进程 stdout/stderr 全量收集，无输出上限（第一轮 #27 部分修复） |
| 13 | App.tsx:234 | `Math.round(duration % 60)` 可为 60 → 显示 "0m 60s"；预览 `fmt` 同理 "00:60.0"（第一轮 #44 未修复） |
| 14 | App.tsx:70 | ffmpeg 路径仅挂载时检测一次，运行期放置后不刷新（第一轮 #45 未修复） |
| 15 | srt.rs:63-66 | `build_srt` 残留空 `iter_mut` 死循环；`text.trim()` 删除有意空白（第一轮 #40 未修复） |
| 16 | ffmpeg.rs | `locate_ffmpeg` 只查 `exists()` 不校验 `is_file()`（第一轮 #43 未修复） |
| 17 | Cargo.toml:29 | `anyhow` 为未使用的直接依赖（源码中零引用） |
| 18 | lib.rs:163 | `check_local_whisper` 已注册但前端从未调用，死命令 |
| 19 | lib.rs:186 | save_srt 扩展名大小写敏感（`.SRT` 被拒，见 BLOCKER #1） |
| 20 | convert.rs | 表内重复键（於/應/處/後/間/頻/變/準 等重复 insert，值相同无害但应清理）；`乾→干` 无条件转换会把「乾隆」转成「干隆」（多对一误转，需上下文或剔除） |
| 21 | convert.rs:131 | NFKC 会把 `™→TM`、全角标点→半角等用户文本改写（行为边界，确认是否符合预期） |
| 22 | lib.rs:142 | `_model: Option<String>` 参数被忽略，云端分支恒传 `None` → 设置里的模型选择无效（死参数） |
| 23 | store.ts:33 | `getAsrSettings` 不校验存储的 backend 值；被篡改/损坏的 settings.json 可造成 UI 与后端行为不一致 |
| 24 | tauri.conf.json | CSP `connect-src` 含 `https://dashscope.aliyuncs.com`，但前端从不直连（多余面）；dev 模式 react-refresh 内联 preamble 与 `script-src 'self'` 的兼容性需实测验证 |

---

## 第一轮修复状态复核

| 第一轮编号 | 结论 |
|-----------|------|
| BLOCKER #1 backend 非法值 | ✅ 已修复（显式 match，非法值报错） |
| BLOCKER #2 csp:null | ✅ 已修复（严格白名单 CSP） |
| HIGH #3 save_srt 任意路径写入 | ⚠️ 已修复但**引入新 BLOCKER**（canonicalize 回归，见 #1） |
| HIGH #4 音频输出覆盖 | ✅ 已修复（PID+nano 唯一后缀） |
| HIGH #5 临时文件竞态 | ✅ 已修复（唯一后缀） |
| HIGH #6 whisper 子进程无超时 | ❌ 修复无效——超时不杀进程，见 HIGH #3 |
| HIGH #7 base64 OOM | ✅ 已修复（50MB 上限） |
| HIGH #8 PATH 回退劫持 | ⚠️ 基本修复（dev 门控），残留环境变量边缘路径 |
| HIGH #9 事件 payload 校验 | ✅ 已修复（clamp + 白名单） |
| HIGH #10 API Key 前端可读 | ❌ **未修复**，见 HIGH #2 |
| HIGH #11 明文存储 | ❌ **未修复**，见 HIGH #2 |
| MEDIUM #12/#14/#17/#19/#20/#21/#23-#26/#28-#35 | ✅ 已修复 |
| MEDIUM #13 job_id / #15 emit / #16 错误回显 | ❌ 未修复，见 MEDIUM #8 / LOW #11 / MEDIUM #5 |
| LOW #36-#45 | ❌ 大部分未修复（见 LOW 表） |

---

## 优先修复路线图

### 立即（BLOCKER + HIGH）
1. `lib.rs:185` — canonicalize 改父目录，恢复导出功能（阻塞所有用户）
2. `asr.rs` / `ffmpeg.rs` — 超时后真正 kill 子进程
3. `App.tsx:130` — 改用 `onDragDropEvent` 修复拖拽上传
4. `store.ts` — 密钥移出前端（长期：Credential Manager）

### 短期（安全纵深 + 隐私）
5. capabilities — 移除多余 fs 权限
6. `asr.rs:181` — 错误回显去正文

### 中期（鲁棒性）
7. WAV 中间文件清理 / 临时目录化
8. 进度事件 job_id
9. transcribe 输入校验

---

## 审查结论

**整体评级：Request Changes（同上轮）**

第一轮的安全修复方向正确（CSP、backend 枚举、payload 校验均已落地），但存在三类实质问题：
1. **修复引入回归**（save_srt canonicalize 使导出主流程不可用）；
2. **「修复」无效**（两个子进程超时都不杀进程）；
3. **核心安全项被跳过**（API Key 仍前端明文 + 明文落盘，占上轮 HIGH 的 2/10）。

另有两项 Tauri 2 平台级事实被遗漏：`File.path` 已移除导致拖拽失效、无 scope 的 fs 权限扩大 XSS 攻击面。建议按路线图修复后重新审查。
