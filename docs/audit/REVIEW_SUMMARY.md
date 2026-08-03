# 对抗式代码审查 — 最终汇总报告

> 审查时间：2026-07-21  
> 审查范围：lib.rs / asr.rs / srt.rs / ffmpeg.rs / App.tsx / store.ts / convert.rs / tauri.conf.json  
> 审查方法：6 个并行 Agent 独立深度审查

---

## 问题总览（按严重程度）

| 严重度 | 数量 | 文件 |
|--------|------|------|
| **BLOCKER** | 2 | lib.rs(1), tauri.conf.json(1) |
| **HIGH** | 10 | lib.rs(2), asr.rs(3), ffmpeg.rs(2), App.tsx(1), store.ts(2) |
| **MEDIUM** | 22 | lib.rs(4), asr.rs(5), srt.rs(4), ffmpeg.rs(3), App.tsx(4), store.ts(1), convert.rs(2) |
| **LOW** | 11 | lib.rs(2), asr.rs(2), srt.rs(2), ffmpeg.rs(2), App.tsx(2), convert.rs(2) |

**总计：45 个问题**

---

## BLOCKER（需立即修复）

### 1. `lib.rs:77-113` — backend 非法值自动回退云端（隐私泄露）
- **文件**: `src-tauri/src/lib.rs`
- **问题**: `match backend.as_str()` 仅 `"local"` 特判，其余全部走云端。当前端拼写错误、被篡改或传入未来新增值时，本地音频会被送到云端识别，造成隐私泄露。
- **修复**: 使用显式枚举 `"local" | "cloud"`，其余返回 `Err("无效 backend")`。

### 2. `tauri.conf.json:25` — `csp: null` 完全关闭 Content Security Policy
- **文件**: `src-tauri/tauri.conf.json`
- **问题**: CSP 为 null 导致前端 XSS 风险可串联 API Key 窃取 + 文件系统读写。
- **修复**: 改为严格白名单 CSP，至少包含：
  ```
  default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self' https://dashscope.aliyuncs.com
  ```

---

## HIGH（高优先级）

### 3. `lib.rs:150-163` — save_srt 任意路径写入
- **问题**: 无目录白名单，可覆盖用户可写任意文件。
- **修复**: 限制到用户选择目录 + canonicalize 前缀校验。

### 4. `lib.rs:35-46` — 音频输出文件名固定可覆盖
- **问题**: `{stem}_16k_mono.wav` 重复处理同名视频会静默覆盖。
- **修复**: 写入前检查 `exists()`，存在则追加时间戳/随机后缀。

### 5. `asr.rs:278-280,321-329` — 临时文件名可预测（竞态）
- **问题**: `whisper_out_{stem}` 并发时互相覆盖/读到他人结果。
- **修复**: 使用 UUID/随机后缀，每次任务独立 temp 目录。

### 6. `asr.rs:298-306` — whisper 子进程无超时
- **问题**: `Command::output()` 无限阻塞，可能永久挂死。
- **修复**: 改用 `tokio::process::Command` + `timeout`，超时 kill 并返回明确错误。

### 7. `asr.rs:125-129` — 整文件 base64 一次性加载
- **问题**: 大文件 OOM，请求体过大被拒。
- **修复**: 增加文件大小上限检查，超限提前报错。

### 8. `ffmpeg.rs:59,119` — PATH 回退可被二进制劫持
- **问题**: 捆绑二进制缺失时无条件回退 PATH，可执行恶意 ffmpeg。
- **修复**: 生产模式禁用 PATH 回退，或对捆绑二进制做 hash 校验。

### 9. `App.tsx:79-80` — 事件 payload 未校验
- **问题**: `progress` 可为负数/NaN，`stage` 强转无校验，可污染 UI 状态。
- **修复**: `progress` 做 `isFinite + clamp(0..1)`，`stage` 做白名单校验。

### 10. `store.ts:12-22` — API Key 明文且前端可读
- **问题**: `getApiKey()` 直接返回明文，前端注入脚本可窃取。
- **修复**: 密钥仅在 Rust 侧持有，前端只返回 `hasApiKey: bool`。

### 11. `store.ts:7` — 敏感信息无加密存储
- **问题**: `settings.json` 明文持久化 API Key。
- **修复**: 改用 Windows Credential Manager / keyring 存储。

---

## MEDIUM（中优先级）

### lib.rs
- **12**. `probe_duration(...).unwrap_or(0.0)` 吞掉探测错误导致后续字幕时间轴错误
- **13**. 进度事件无 `job_id`，并发命令会互相覆盖状态
- **14**. `video_path.parent()` 为空时隐式回退 `temp_dir()`，行为不可预期
- **15**. `emit` 结果被忽略，事件发送失败无日志

### asr.rs
- **16**. 错误信息回显原始响应片段（隐私泄露）
- **17**. `Regex::new(...).unwrap()` 存在 panic 路径
- **18**. 子进程 stdout/stderr 全量收集，异常日志可内存爆
- **19**. 临时文件清理不完整，失败路径残留垃圾
- **20**. `api_key` 仅判空不判空白符
- **21**. `language` 参数无白名单校验

### srt.rs
- **22**. `needs_distribute` 任意异常片段会覆写全部时间戳
- **23**. `NaN/Inf` 静默流入格式化产生错误时间轴
- **24**. 负起始时间被静默钳制为 0，未保证时间单调性
- **25**. 超大时长转换存在饱和语义无上界检查
- **26**. SRT 换行仅 `\n`，部分 Windows 工具不兼容 CRLF

### ffmpeg.rs
- **27**. `.output()` 全量缓存 stdout/stderr，异常场景内存压力
- **28**. 无执行超时控制，ffmpeg 卡死导致流程永久阻塞
- **29**. `locate_ffprobe()` 未尝试 `ffprobe_x64.exe`

### App.tsx
- **30**. 事件未绑定任务实例，旧任务延迟事件可覆盖当前状态
- **31**. 本地 ASR 模式仍传递 `apiKey`（不必要暴露面）
- **32**. 多处异步操作无统一 try/catch，失败无用户反馈
- **33**. 拖拽文件无类型/扩展名校验，依赖非标准 `File.path`

### convert.rs
- **34**. 高频繁简映射覆盖不足（缺失 `國/學/習` 等高频字）
- **35**. 缺少 Unicode NFKC/NFC 规范化，兼容区字符可能绕过映射

---

## LOW（建议改进）

| # | 文件 | 问题 |
|---|------|------|
| 36 | lib.rs | `run(...).expect(...)` 启动期 panic 点 |
| 37 | lib.rs | 启动时 `expect` panic 路径 |
| 38 | asr.rs | `language` 未做白名单校验 |
| 39 | asr.rs | 每字符构造 String 查表有额外分配 |
| 40 | srt.rs | `text.trim()` 删除有意保留的前后空白 |
| 41 | srt.rs | 字符串构建未预分配容量 |
| 42 | ffmpeg.rs | `probe_duration()` 吞掉全部错误上下文 |
| 43 | ffmpeg.rs | `exists()` 未校验 `is_file()` |
| 44 | App.tsx | `duration=0` 显示为未知；秒数四舍五入可能为 60 |
| 45 | App.tsx | ffmpeg 路径仅初始化检测一次，运行期修复后无法恢复 |

---

## 优先修复路线图

### 立即（Blocker + 高风险）
1. `tauri.conf.json` — 关闭 CSP
2. `store.ts` — API Key 移除出前端，改用后端持有 + keyring
3. `lib.rs` — backend 参数显式枚举 + 路径写入策略

### 短期（高稳定性风险）
4. `ffmpeg.rs` — PATH 回退禁用 + 超时控制
5. `asr.rs` — 子进程超时 + 临时文件竞态修复
6. `App.tsx` — 事件 payload 校验 + 任务实例绑定

### 中期（鲁棒性）
7. `srt.rs` — 时间戳异常处理策略重构（局部修复而非全量覆写）
8. `convert.rs` — 高频缺字补全 + Unicode 规范化
9. `asr.rs` — 文件大小上限 + 错误隐私处理

---

## 审查结论

**整体评级：Request Changes（需修复后再通过）**

代码结构清晰，主流程可读性较好，但安全边界（API Key 暴露、CSP 关闭）和并发鲁棒性（临时文件竞态、子进程超时）存在实质风险。最关键的修复是 `csp: null` + `API Key 前端可读` 这个高风险组合，以及 `backend` 非法值隐私泄露问题。
