# 项目结构整理与开发规范化 — 任务计划

- [x] Task 1: 归档代码审查报告到 docs/audit/
    - 1.1: 创建 `docs/audit/` 目录
    - 1.2: 用 `git mv` 迁移 7 份 REVIEW 文件（REVIEW_SUMMARY / app / asr / ffmpeg / lib / security / srt）保留 rename 历史
    - 1.3: 新建 `docs/audit/README.md` 归档索引，说明来源日期、文件清单、"非 active TODO"状态
    - 1.4: 校验 `git status` 中 7 个 rename 被正确识别，无内容改动

- [x] Task 2: 删除 Agent 工作流过程产物
    - 2.1: 删除 `AGENT_PROMPT.md`（对抗式审查 Agent 指令）
    - 2.2: 删除 `HUMAN_INPUT.md`（空文件）
    - 2.3: 删除 `TASKS.md`（审查任务已全部 `[x]`，关键结论已在 docs/audit/REVIEW_SUMMARY.md）
    - 2.4: 删除 `current_tasks/`（含 `.gitkeep`）与 `agent_logs/`、`public/` 三个空目录，删除前确认目录内无未提交文件

- [x] Task 3: 补齐编辑器与格式化规范配置
    - 3.1: 新建根目录 `.editorconfig`（utf-8 / lf / 末尾换行 / 通用 2 空格 / rs+toml 4 空格 / md 保留行尾空格）
    - 3.2: 运行 `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` 探测现有 Rust 代码与默认 rustfmt 的差异量
    - 3.3: 若差异可接受则新建 `src-tauri/rustfmt.toml`（edition 2021 + max_width 100）；差异过大则跳过该文件并在 AGENTS.md 记录约定，不改业务代码
    - 3.4: 增量修改根 `.gitignore`：移除已删除的 `agent_logs/` 条目，补充 `.env` / `.env.local` / `*.tsbuildinfo`，保持既有条目顺序不变

- [x] Task 4: 补充 LICENSE 与 package.json 脚本入口
    - 4.1: 新建根目录 `LICENSE`（MIT，年份 2026）
    - 4.2: 在 `package.json` 的 `scripts` 补充 `typecheck`（tsc --noEmit）
    - 4.3: 在 `package.json` 的 `scripts` 补充 `fmt:rust` / `lint:rust`（cargo fmt --check），不新增任何 npm 依赖

- [x] Task 5: 更新项目文档使其与新结构一致
    - 5.1: 更新 `README.md` 的「目录速览」章节，加入 `docs/audit/`、`LICENSE`、`.editorconfig`，移除不存在的条目
    - 5.2: 在 `README.md` 新增「开发规范」小节：指向 `AGENTS.md`、说明 `npm run typecheck` / `cargo fmt` 使用方式、许可证声明
    - 5.3: 更新 `AGENTS.md` 的「目录与约定」以反映 docs/ 目录与格式化约定
    - 5.4: 修正 `AGENTS.md`「当前状态」中"无 git 仓库（尚未 git init）"这一过时表述

- [ ] Task 6: 验证与提交
    - 6.1: 运行 `npm run typecheck` 确认 TypeScript 编译通过
    - 6.2: 运行 `npm run build`（tsc + vite build）确认前端产物可生成
    - 6.3: 运行 `cargo build --manifest-path src-tauri/Cargo.toml` 确认 Rust 编译通过
    - 6.4: `git add -A` 后检查 diff 范围仅限本次计划的文件，确认无业务代码改动
    - 6.5: 提交一个 commit（chore: 整理项目结构与开发规范），不推送远端
