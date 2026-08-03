# 项目结构整理与开发规范化 — doc.md

## 需求场景

当前仓库是 v0.1.0 功能完整的 Tauri 桌面应用「字幕生成工作台」，但根目录混入大量对抗式审查的 Agent 工作流临时文件（`AGENT_PROMPT.md` / `HUMAN_INPUT.md` / `TASKS.md` / `REVIEW_*.md` / `current_tasks/` / `agent_logs/`），使新贡献者难以分辨"产品代码"与"过程产物"。同时缺少 `.editorconfig`、格式化配置、`LICENSE` 等常规开源工程文件；README 中目录速览与实际略有出入。

**目标**：清理与产品无关的过程产物、把有历史价值的审查报告归档到 `docs/audit/`、补齐现代 TypeScript + Rust 项目的常规开发规范配置，让根目录只留必要文件。

**非目标**：
- 不改动任何业务代码逻辑（`src/`、`src-tauri/src/` 内的 `.tsx`/`.rs`）。
- 不修复 `REVIEW_SUMMARY.md` 列出的 45 个安全/健壮性问题（另立任务）。
- 不修改 Cargo 依赖、npm 依赖版本。
- 不改动 Tauri 打包配置。

## 架构与技术方案

### 分类原则

把根目录条目分成 4 类，按类处理：

| 类别 | 处理方式 | 示例 |
|---|---|---|
| **产品代码** | 保留原位 | `src/`、`src-tauri/src/`、`index.html`、`package.json`、`vite.config.ts`、`tsconfig*.json`、`README.md`、`AGENTS.md` |
| **有历史价值** | 归档到 `docs/audit/` | `REVIEW_SUMMARY.md`、`REVIEW_app.md`、`REVIEW_asr.md`、`REVIEW_ffmpeg.md`、`REVIEW_lib.md`、`REVIEW_security.md`、`REVIEW_srt.md` |
| **Agent 过程产物** | 删除 | `AGENT_PROMPT.md`、`HUMAN_INPUT.md`（空文件）、`TASKS.md`（内容全部指审查任务）、`current_tasks/`、`agent_logs/` |
| **开发规范补齐** | 新增 | `.editorconfig`、`src-tauri/rustfmt.toml`、`LICENSE`（MIT）、`docs/audit/README.md`（审查归档索引） |

### 影响文件

**删除（4 个文件 + 2 个空目录）**
- `d:\vibecoding\字幕生成工作台\AGENT_PROMPT.md` — 对抗式审查 Agent 指令，任务已完成
- `d:\vibecoding\字幕生成工作台\HUMAN_INPUT.md` — 仅 1 行空内容
- `d:\vibecoding\字幕生成工作台\TASKS.md` — 已全部标记 `[x]`，内容与审查任务耦合
- `d:\vibecoding\字幕生成工作台\current_tasks\` — 空目录（仅含 `.gitkeep`）
- `d:\vibecoding\字幕生成工作台\agent_logs\` — 空目录（.gitignore 已忽略）
- `d:\vibecoding\字幕生成工作台\public\` — 空目录，Vite 用不到（`index.html` 直接在根目录）

**移动（7 个文件 → `docs/audit/`）**
- `REVIEW_SUMMARY.md` → `docs/audit/REVIEW_SUMMARY.md`
- `REVIEW_app.md` → `docs/audit/REVIEW_app.md`
- `REVIEW_asr.md` → `docs/audit/REVIEW_asr.md`
- `REVIEW_ffmpeg.md` → `docs/audit/REVIEW_ffmpeg.md`
- `REVIEW_lib.md` → `docs/audit/REVIEW_lib.md`
- `REVIEW_security.md` → `docs/audit/REVIEW_security.md`
- `REVIEW_srt.md` → `docs/audit/REVIEW_srt.md`

**新增**
- `d:\vibecoding\字幕生成工作台\.editorconfig` — 统一缩进/换行/字符集
- `d:\vibecoding\字幕生成工作台\src-tauri\rustfmt.toml` — Rust 格式化配置（默认 rustfmt + edition 2021）
- `d:\vibecoding\字幕生成工作台\LICENSE` — MIT License（Windows 桌面工具类常见选择，可协商）
- `d:\vibecoding\字幕生成工作台\docs\audit\README.md` — 归档索引，说明这批文件的来源与状态

**修改**
- `d:\vibecoding\字幕生成工作台\.gitignore` — 移除 `agent_logs/` 条目（目录已删），补充 `.env`、`.env.local`、`*.log`、`src-tauri/gen/schemas/`（已在子 .gitignore 中）保持一致；不重复
- `d:\vibecoding\字幕生成工作台\README.md` — 更新「目录速览」章节以反映实际结构（增加 `docs/`），补充"贡献指南 / 开发规范"简要段落引用 `AGENTS.md`
- `d:\vibecoding\字幕生成工作台\package.json` — 在 `scripts` 中补充 `"typecheck": "tsc --noEmit"` 与 `"lint:rust": "cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check"`（可选，只提供命令入口，不强制新增依赖）

### `.editorconfig` 内容示意

```ini
root = true

[*]
charset = utf-8
end_of_line = lf
insert_final_newline = true
trim_trailing_whitespace = true
indent_style = space
indent_size = 2

[*.{rs,toml}]
indent_size = 4

[*.md]
trim_trailing_whitespace = false
```

### `src-tauri/rustfmt.toml` 内容示意

```toml
edition = "2021"
max_width = 100
```

### `docs/audit/README.md` 内容示意

```markdown
# 代码审查归档 (2026-07-21)

本目录归档了一次多 Agent 并行对抗式代码审查（adversarial review）的完整产物。

- `REVIEW_SUMMARY.md` — 45 个问题的按严重程度汇总与修复路线图
- `REVIEW_lib.md` / `REVIEW_asr.md` / `REVIEW_srt.md` / `REVIEW_ffmpeg.md` / `REVIEW_app.md` / `REVIEW_security.md` — 各模块的详细发现

这些文件不再是 active TODO，仅作历史参考。BLOCKER/HIGH 问题需另立任务追踪。
```

### `LICENSE` 选择

默认 **MIT**（宽松、Tauri/React 生态主流）。若用户希望使用其他协议（Apache-2.0 / GPL-3.0 / 私有），在 tasks.md 阶段可调整。

## 数据流与执行顺序

```
1. mkdir docs/audit/
2. git mv REVIEW_*.md docs/audit/
3. rm AGENT_PROMPT.md HUMAN_INPUT.md TASKS.md
4. rmdir current_tasks/ agent_logs/ public/
5. write .editorconfig / src-tauri/rustfmt.toml / LICENSE / docs/audit/README.md
6. edit .gitignore / README.md / package.json
7. cargo fmt --check + npm run build（验证格式化配置不破坏现有代码）
8. git add -A && commit
```

## 边界条件与异常处理

- `git mv` 而非 `mv`：保留 REVIEW 文件的历史（blame 可追溯到原 commit）。
- `agent_logs/` 与 `current_tasks/` 若被其他未 commit 的 lock 文件占用，删除前先确认目录为空。
- 修改 README 时保持中文语言风格与现有一致。
- `.gitignore` 只做增量修改，不重排既有条目。
- rustfmt 默认规则可能与现有 Rust 源码格式冲突：**先 `cargo fmt --check`，若报错则记录差异但不自动 fix**（业务代码不改）；若冲突较多则本次不落 `rustfmt.toml`，改为在 `AGENTS.md` 里补一条约定即可。
- Windows 换行：`.editorconfig` 强制 `lf`，但 git 已由 `core.autocrlf` 处理；不新增 `.gitattributes`（除非验证发现问题）。

## 预期结果

清理后根目录（顶层可见文件/目录）：

```
.
├── .editorconfig            [新]
├── .gitignore
├── AGENTS.md
├── LICENSE                  [新]
├── README.md                [更新]
├── docs/                    [新]
│   └── audit/               [归档 7 份 review]
├── index.html
├── package-lock.json
├── package.json             [scripts 微调]
├── src/                     [不变]
├── src-tauri/               [仅新增 rustfmt.toml]
│   ├── rustfmt.toml         [新]
│   └── ...
├── tsconfig.json
├── tsconfig.node.json
└── vite.config.ts
```

对比清理前 30+ 顶层条目，清理后仅 13 项，且全部与"产品/构建/规范"直接相关。开发者能立刻定位关键文件；Agent 过程物退出根目录。

## 验证方式

1. `npm install && npm run build` 前端编译通过（tsc + vite）。
2. `cargo build --manifest-path src-tauri/Cargo.toml` Rust 编译通过。
3. `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` 与新配置无冲突（或已记录）。
4. `git status` 干净，`git log --stat` 中 REVIEW 文件的 rename 被正确识别。
