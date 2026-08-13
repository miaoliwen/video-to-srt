# 对抗式审查 · 第三轮（Round 3）

> 审查对象：commit `2455f66` 引入的 Round 2 修复（BLOCKER/HIGH/MEDIUM/LOW 全量修复 + 凭据管理器 + 繁简转换 + NFKC + 39 个测试）
> 审查目标：**复查本轮修复是否引入新问题**
> 审查日期：2026-08-13
> 基线：`cargo test` 39/39 通过

## 结论

Round 2 的修复整体质量高、验证充分，**没有发现新的 BLOCKER/HIGH**。但发现 **2 个 MEDIUM（其中一个会悄悄抵消凭据迁移的明文清除保证）、4 个 LOW/INFO**。核心教训：修复"清理临时文件"与"擦除明文"时，把清理权柄交给了不受约束的调用方（IPC 任意路径删除），以及低估了 tauri-plugin-store 的整文件持久化语义。

---

## 修复状态

- M-1 ✅ 已修复（2026-08-13）：新增 `is_in_temp_subdir` 守卫 + 单测，删除前校验父目录为应用临时子目录，校验失败则不删。
- M-2 ✅ 已修复（2026-08-13）：`getAsrSettings` 引导时 `store.delete("apiKey")`，删除成功则显式 `save()`——同时清插件内存与磁盘；已核实插件 `delete` 触发防抖 auto-save、`save` 整文件回写 `self.cache`。
- L-2 ✅ 已修复（2026-08-13）：`probe_duration` 改为 async + `kill_on_drop` + 内部 30s 超时，移除 spawn_blocking 假超时。
- L-3 ✅ 已修复（2026-08-13）：启动时 `cleanup_stale_temp_wavs_in` 清理 `<temp>/video-to-srt` 中超过 24h 的陈旧 WAV（单测覆盖）。
- L-1 ✅ 已修复（2026-08-13）：`onDragDropEvent` effect 加 `cancelled` 标志——注册在 cleanup 后解析时立即注销，StrictMode 双挂载不再泄漏第一个监听器。
- L-4 为接受项（Windows-only 密钥存储）。

## 🟡 MEDIUM

### M-1. `transcribe` 无条件删除调用方传入的任意路径（Round 2 新增）——✅ 已修复

**位置**：`src-tauri/src/lib.rs` — `transcribe` 末尾

```rust
let _ = tokio::fs::remove_file(&audio_path).await;   // 无条件、无约束
let segments = segments_result?;
```

- **实证**：对比 `git show 6f8ed8d:src-tauri/src/lib.rs`（Round 2 之前），修复前的 `transcribe` **没有任何删除逻辑**。这段 `remove_file` 是 Round 2「WAV 中间文件清理」修复引入的。
- **问题**：删除发生在 backend match 之后、错误传播之前——**无论 backend 值是否合法、识别是否成功，只要 invoke 了 `transcribe`，传入的 `audio_path` 就被删除**。该路径完全由 IPC 调用方（前端）控制，没有任何"必须是临时目录内 WAV"的约束。
- **当前影响**：现在的前端只会传 `extract_audio` 返回的 `%TEMP%\video-to-srt\...wav`，正常流程无用户数据风险。但这是一个隐患巨大的 IPC 契约：任何未来调用方（批量处理功能、改版 UI、注入的脚本）若误传原始视频路径或其他文件路径，该文件会被静默删除——且删除发生在"无效 backend"等所有错误路径上，防不胜防。
- **修复建议**：删除权柄应跟随所有权。由 `extract_audio` 创建文件、由 `transcribe` 删除，中间隔着一次 IPC，应显式约束：删除前校验 `audio_path` 的规范化父目录 == `%TEMP%/video-to-srt`（或让 `transcribe` 只接受 `ExtractResult` 返回的路径句柄/临时目录令牌）。

### M-2. 凭据迁移后 settings.json 明文 apiKey 会被 tauri-plugin-store 复活（升级路径）

**位置**：`src/store.ts`（`getAsrSettings`/`setAsrSettings`）+ `src-tauri/src/lib.rs`（`scrub_settings_key_in`）

- **实证**：旧版前端通过同一插件把密钥存在 `settings.json` 顶层（`git show 6f8ed8d:src/store.ts`：`store.set("apiKey", key)`）。tauri-plugin-store 的 `save()` 会把**整个内存 map** 序列化回磁盘，且前端 `load("settings.json")` 在应用启动时就已把文件内容（含 apiKey）读入内存。
- **复活时序**：
  1. 老用户升级 → 启动时 `getAsrSettings()` 触发 `load()`，settings.json（含 apiKey）进入 store 插件内存；
  2. 首次读取密钥 → Rust 迁移到凭据管理器 → `scrub_settings_key_in` 把磁盘上的 settings.json 擦掉 apiKey；
  3. 用户之后任意一次"保存设置" → `store.save()` 把**内存里的完整 map**（含 apiKey）写回磁盘 → **明文密钥复活**；
  4. 此后凭据管理器为主存储，读取链不会再触发擦除（除非 set/clear），明文会一直留在前端可读的 settings.json 里。
- **影响**：这正是凭据管理器迁移要消灭的暴露面——迁移测试 `migrates_from_settings_json_and_scrubs_it` 只验证了 Rust 侧擦除磁盘文件，**没有覆盖 store 插件的内存→磁盘回写**，所以测试全绿而真实场景失效。
- **修复建议**：前端引导时一次性清除：`getAsrSettings()` 开头执行 `await store.delete("apiKey")`（幂等，同时清内存与磁盘）；这是唯一能同时覆盖两处的点，Rust 侧无法控制插件内存。

---

## ⚪ LOW / INFO

### L-1. StrictMode 下拖拽监听重复注册（仅 dev）

`src/App.tsx` 的 `onDragDropEvent` effect：注册是异步的（`.then` 才拿到 unlisten），而 StrictMode 在 dev 下 mount→unmount→mount，首次注册的 unlisten 可能在 cleanup 执行时尚未赋值 → 第一个监听器泄漏 → dev 下出现两个监听器。事件处理器是幂等的（`setVideoPath` 同路径、`appendLog` 可能重复一条日志），生产构建不受影响（StrictMode 双调用仅 dev）。修复：effect 内加 cancelled 标记，`.then` 里检查后统一注销。

### L-2. `probe_duration` 的超时是同类"假超时"

`lib.rs` `extract_audio` 里 `spawn_blocking(probe_duration)` 外包 30s timeout——超时后 ffprobe 在阻塞线程里继续跑（与 Round 2 修掉的 whisper/ffmpeg 问题同类）。影响面小：仅探测时长、不写文件、结果直接丢弃。建议与子进程修复保持一致（`tokio::process` + `kill_on_drop`），或明确注释为"尽力而为探测"。

### L-3. 崩溃路径残留 `%TEMP%/video-to-srt` 文件

Round 2 保证的是正常流程（含错误路径）清理；若应用在 extract 后、transcribe 前被杀掉，临时 WAV 会残留。建议启动时 best-effort 清理该目录下超过 24h 的陈旧文件（一行 `remove_dir_all` + ignore 即可，非必须）。

### L-4. （INFO，接受项）keyring 无后端时非 Windows 无法保存密钥

`keyring` 仅启用 `windows-native` feature，在非 Windows 平台 feature 为 no-op → `Entry::new` 失败 → `set_api_key` 报"无法访问系统凭据管理器"。应用 bundle 仅 `nsis`（Windows-only），属接受项；建议在 README 注明"密钥存储仅支持 Windows"，避免未来多平台计划踩坑。

---

## 已验证无问题的点（Round 2 修复抽查）

| 项 | 结论 |
|---|---|
| `kill_on_drop(true)` + timeout | 语义正确：timeout 触发时 future 被 drop → child 被杀。`wait_with_output` 消费式借用使超时分支无法再手动 kill，`kill_on_drop` 是 tokio 官方推荐模式 ✓ |
| whisper/ffmpeg 失败路径清理 | 超时、非零退出均删除残缺输出 ✓（与 M-1 无关，那些删除的是**子进程写入的固定路径**）|
| `save_srt` canonicalize 父目录 | 不存在文件不再报错；`.SRT` 大小写兼容；`file_name()` 仅取末段，无路径穿越 ✓ |
| 拖拽 `onDragDropEvent` | 事件类型 enter/over/drop/leave 匹配；扩展名白名单保留；生产环境注册/注销正确 ✓ |
| 密钥迁移链 | 三层读取顺序、尽力而为迁移、NoEntry 视为成功、单测覆盖充分 ✓（缺陷仅在 M-2 的插件回写，不在 Rust 层）|
| job_id 过滤 | `ev.job_id != null && !== current` 丢弃旧任务事件；save_srt 事件 job_id=null 不被过滤 ✓ |
| NFKC 标点保留 | 保留集合理（U+3000–303F / FF01–FF0F / FF1A–FF20 / FF3B–FF40 / FF5B–FF60 / FFE0–FFE6），逐字符等价于整串（兼容映射均单字符级）✓ |
| 输入校验统一 | transcribe 入口 `exists()+is_file()` 对两条路径共享同一错误 ✓ |
| CSP `connect-src 'self'` | 前端无任何直连请求（全部 HTTP 在 Rust 侧），dev 下 Vite HMR 同源不受影响 ✓ |

---

## 复现/验证命令

```bash
# M-1：删除逻辑为 Round 2 新增（修复前无 remove_file）
git show 6f8ed8d:src-tauri/src/lib.rs | grep -n remove_file   # 无输出

# M-2：旧版确以顶层 apiKey 写入 settings.json
git show 6f8ed8d:src/store.ts | grep -n 'store.set("apiKey"'
```

基线：`cargo test` 39/39 通过；`cargo clippy` 仅剩 pre-existing `too_many_arguments`。
