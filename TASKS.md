# 任务清单 — 字幕生成工作台 对抗式审查

## 状态说明
- [ ] 待完成
- [x] 已完成
- [~] 进行中（查看 current_tasks/ 目录确认谁在做）

## 审查任务列表

### 核心Rust模块审查

- [ ] 审查 `src-tauri/src/lib.rs` — Tauri命令注册、管线编排、事件发射
  - 重点：命令注入、错误处理、panic路径
- [ ] 审查 `src-tauri/src/asr.rs` — 双后端ASR（Qwen HTTP / whisper-cli子进程）
  - 重点：子进程安全、HTTP请求安全、base64处理、错误传播
- [ ] 审查 `src-tauri/src/srt.rs` — SRT格式化和等分时间戳
  - 重点：时间戳计算溢出、Unicode处理、空输入边界
- [ ] 审查 `src-tauri/src/ffmpeg.rs` — ffmpeg定位与调用
  - 重点：命令注入、路径处理、进程参数转义
- [ ] 审查 `src-tauri/src/convert.rs` — 繁→简高频字映射
  - 重点：映射完整性、Unicode规范化

### 前端代码审查

- [ ] 审查 `src/App.tsx` — 主界面、管线调用
  - 重点：XSS、状态管理、文件处理、事件发射
- [ ] 审查 `src/store.ts` — tauri-plugin-store封装
  - 重点：API Key安全存储、敏感数据处理

### 集成与配置审查

- [ ] 审查 `src-tauri/src/encoding_tests.rs` — UTF-8 BOM往返测试
  - 重点：测试覆盖完整性、边界情况
- [ ] 审查 `src-tauri/tauri.conf.json` — Tauri权限配置
  - 重点：权限过度、危险权限暴露
- [x] 审查 `src-tauri/src/lib.rs` — Tauri命令注册、管线编排、事件发射
  - BLOCKER(1): backend非法值自动云端回退（隐私泄露）
  - HIGH(2): 任意路径写入、音频输出静默覆盖
  - MEDIUM(4): 时长探测错误被吞、进度事件竞态、temp_dir隐式回退、emit静默失败
- [x] 审查 `src-tauri/src/asr.rs` — 双后端ASR
  - HIGH(3): 临时文件竞态、子进程无超时、整文件base64加载OOM
  - MEDIUM(6): 错误信息隐私泄露、unwrap panic路径、输出内存压力、清理不完整、空格未校验、language无白名单
- [x] 审查 `src-tauri/src/srt.rs` — SRT格式化和等分时间戳
  - HIGH(2): 异常片段覆写全部时间戳、NaN/Inf静默流入
  - MEDIUM(4): 负时间钳制、超大时长饱和、无CRLF、单调性无保证
- [x] 审查 `src-tauri/src/ffmpeg.rs` — ffmpeg定位与调用
  - HIGH(2): PATH回退可被二进制劫持（ffmpeg+ffprobe）
  - MEDIUM(3): output全量缓存、无超时控制、ffprobe_x64未尝试
- [x] 审查 `src/App.tsx` — 主界面、管线调用
  - HIGH(1): 事件payload未校验导致状态污染
  - MEDIUM(4): 事件未绑定任务实例、本地模式传apiKey、多处异步无try-catch、拖拽无校验
- [x] 审查 `src-tauri/src/convert.rs` + `src/store.ts` + `tauri.conf.json`
  - BLOCKER(1): csp:null完全关闭CSP
  - HIGH(2): API Key明文前端可读、无加密存储
  - MEDIUM(3): ASR设置无校验、convert缺高频字、缺Unicode规范化

### 潜在问题追踪

<!-- 发现的问题已汇总到 REVIEW_SUMMARY.md，核心问题如下 -->

**BLOCKER:**
- [lib.rs] backend非法值自动走云端 → 本地音频隐私泄露
- [tauri.conf.json] csp:null 关闭CSP → XSS可串联API Key窃取+文件系统访问

**HIGH（需优先修复）:**
- [lib.rs] save_srt任意路径写入
- [lib.rs] 音频输出静默覆盖
- [asr.rs] 临时文件名可预测（并发竞态）
- [asr.rs] whisper子进程无超时
- [asr.rs] 整文件base64一次性加载OOM
- [ffmpeg.rs] PATH回退可被二进制劫持
- [App.tsx] 事件payload未校验
- [store.ts] API Key明文且前端可读
- [store.ts] settings.json明文持久化敏感信息

<!-- 完整报告见 REVIEW_SUMMARY.md -->

