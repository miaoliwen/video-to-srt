# 代码审查归档 (2026-07-21 ~ 2026-08-13)

本目录归档了三轮对抗式代码审查（adversarial review）的完整产物。

## 文件清单

### 第一轮（2026-07-21，多 Agent 并行）

- `REVIEW_SUMMARY.md` — 45 个问题的按严重程度汇总与修复路线图
- `REVIEW_lib.md` — lib.rs（Tauri 命令注册、管线编排）
- `REVIEW_asr.md` — asr.rs（双后端 ASR）
- `REVIEW_srt.md` — srt.rs（SRT 格式化）
- `REVIEW_ffmpeg.md` — ffmpeg.rs（FFmpeg 定位与调用）
- `REVIEW_app.md` — App.tsx（前端主界面）
- `REVIEW_security.md` — convert.rs / store.ts / tauri.conf.json 安全审查

### 第二轮（2026-08-13）

- `REVIEW_ROUND2.md` — 25 个问题（1 BLOCKER / 3 HIGH / 6 MEDIUM / 15 LOW），含第一轮修复状态复核与回归发现

### 第三轮（2026-08-13）

- `REVIEW_ROUND3.md` — 复查 Round 2 修复引入的新问题：2 MEDIUM（transcribe 无条件删除任意路径、settings.json 明文密钥被 store 插件复活）+ 4 LOW/INFO

## 状态

这些文件不再是 active TODO，仅作历史参考。
BLOCKER/HIGH 问题的修复需另立任务追踪。
