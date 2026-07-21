# ffmpeg.rs 深度审查报告

审查文件：`src-tauri/src/ffmpeg.rs`

## 1) 问题列表

| 行号 | 严重程度 | 问题 |
|---|---|---|
| 59 | HIGH | `locate_ffmpeg()` 在找不到捆绑二进制时无条件回退 `PATH`，可被 `PATH` 劫持执行恶意 `ffmpeg` |
| 119 | HIGH | `locate_ffprobe()` 同样无条件回退 `PATH`，存在同类二进制劫持风险 |
| 68-77 | MEDIUM | `extract_audio()` 使用 `.output()` 全量缓存 stdout/stderr，异常场景下日志可过大导致内存压力/DoS 风险 |
| 68-77 | MEDIUM | `extract_audio()` 无执行超时控制，异常输入或卡死子进程可导致流程长期阻塞 |
| 105-119 | MEDIUM | `locate_ffprobe()` 未尝试 `ffprobe_x64.exe`（与 `locate_ffmpeg()` 侧载逻辑不一致），可能在侧载命名场景下定位失败 |
| 88-103 | LOW | `probe_duration()` 返回 `Option<f64>` 并吞掉全部错误上下文，不利于诊断（文件不存在/权限/ffprobe失败无法区分） |
| 27-35,41-55,107-118 | LOW | 路径检查仅用 `exists()`，未校验 `is_file()`（目录/异常目标也会被当作可执行候选） |

---

## 2) 详细分析与修复建议

### [HIGH] 行 59：`locate_ffmpeg()` 的 PATH 回退可被劫持
**分析**
- 当前顺序：资源目录/开发目录查找失败后，`which::which(exe_name)` 从 `PATH` 解析可执行文件。
- 在桌面应用环境中，`PATH` 可能受用户环境、启动器、恶意软件影响。
- 若捆绑 ffmpeg 缺失或被移除，将执行未知来源 ffmpeg，属于高风险供应链/本地提权入口（至少可导致任意程序执行于当前用户权限）。

**建议修复**
1. **生产模式禁用 PATH 回退**：仅开发模式允许 `which`。
2. 对捆绑二进制做 **完整性校验**（hash/签名）。
3. 若必须回退 PATH，至少限制在白名单目录并记录告警日志。

---

### [HIGH] 行 119：`locate_ffprobe()` 的 PATH 回退同类风险
**分析**
- 与 ffmpeg 相同的执行劫持面。
- `probe_duration()` 可能在每次字幕生成前调用，触发面更高。

**建议修复**
- 与 `locate_ffmpeg()` 同步策略：生产禁用 PATH 回退 + 完整性校验。

---

### [MEDIUM] 行 68-77：`.output()` 全量缓存输出
**分析**
- `Command::output()` 会把 stdout/stderr 全部读入内存。
- ffmpeg 在错误场景（损坏媒体、复杂探测）可能输出大量日志。
- 可能造成内存峰值过高，影响稳定性。

**建议修复**
1. 添加 `-v error -hide_banner` 降噪。
2. 改为 `spawn` + 管道流式读取，限制 stderr 最大字节数（如 64KB 截断）。
3. 仅保留尾部日志用于错误提示。

---

### [MEDIUM] 行 68-77：缺少超时控制
**分析**
- 当前 `.output()` 为阻塞等待，若 ffmpeg 卡住，调用线程会一直等待。
- 对桌面应用表现为“卡住/无响应”。

**建议修复**
1. 使用 `spawn` + `wait_timeout`（或轮询 + kill）实现超时（如 2~5 分钟可配置）。
2. 超时时返回专门错误类型（例如 `FfmpegError::Timeout`）并清理子进程。

---

### [MEDIUM] 行 105-119：`locate_ffprobe()` 缺失 `_x64` 侧载兼容
**分析**
- `locate_ffmpeg()` 已处理 `ffmpeg_x64.exe`，`locate_ffprobe()` 未处理 `ffprobe_x64.exe`。
- 在 Tauri sidecar 命名约定下可能导致 ffprobe 定位失败，从而 `probe_duration()` 恒为 `None`。

**建议修复**
- 在资源目录查找增加 `ffprobe_x64.exe` 分支（与 ffmpeg 一致）。

---

### [LOW] 行 88-103：`probe_duration()` 吞错误
**分析**
- `Option<f64>` + 多处 `.ok()?` 会丢失失败原因。
- 调试时无法区分：ffprobe 未找到、执行失败、输出解析失败、权限问题。

**建议修复**
- 返回 `Result<Option<f64>, FfmpegError>` 或新增 `ProbeError`，保留 stderr/exit code。
- 对“无 duration 值”与“命令失败”做语义区分。

---

### [LOW] 行 27-35,41-55,107-118：仅 `exists()` 不足
**分析**
- `exists()` 对目录也返回 true。
- 后续 `Command::new` 才会报错，错误语义不清晰。

**建议修复**
- 改为 `p.is_file()`，并可在 Windows 下进一步检查扩展名/可执行属性（基础校验）。

---

## 3) 其它审查结论（针对题目关注点）

- **命令注入**：未发现。`Command::new(...).arg(...)` 未经过 shell，参数按原子传递。
- **路径中空格/特殊字符**：处理正确，`arg(Path)` 可安全传递，不依赖手工拼接转义。
- **panic 路径**：未发现显式 `unwrap/expect/panic!`；主要风险在阻塞/错误语义而非 panic。
- **资源管理（句柄泄漏）**：未见明显泄漏；`output()` 生命周期结束后句柄会释放。当前问题是“阻塞与输出缓存”，非泄漏。

## 4) 整体质量评估

`ffmpeg.rs` 在“参数传递安全（抗注入）”方面表现良好，基础错误枚举也较清晰；但在**可执行文件信任边界（PATH 回退）**与**子进程鲁棒性（超时、输出限流）**上存在实质风险。综合评估：**中等偏上（可用，但存在需优先修复的高风险点）**。建议优先关闭两个 HIGH 问题，再处理 MEDIUM 级稳定性问题。
