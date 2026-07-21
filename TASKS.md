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
- [ ] 审查 `src-tauri/capabilities/default.json` — 能力声明
  - 重点：最小权限原则、安全边界

### 潜在问题追踪

<!-- 发现的问题记录在这里，格式：- [问题描述] (严重程度) -->

