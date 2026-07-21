# 安全/质量审查报告（对抗式）

审查文件：
1. `src-tauri/src/convert.rs`
2. `src/store.ts`
3. `src-tauri/tauri.conf.json`

审查时间：2026-07-21

---

## 1) 问题清单（含行号与严重级别）

### [BLOCKER] `src-tauri/tauri.conf.json:25` — `csp: null` 完全关闭 CSP
- **位置**：`app.security.csp = null`
- **风险**：一旦前端存在 XSS/第三方脚本注入，攻击脚本可直接访问前端内存中的 API Key、调用 Tauri IPC、配合文件系统权限进行本地数据读取/写入。

### [HIGH] `src/store.ts:12-16, 18-22` — API Key 以明文形式从前端可读写
- **位置**：`getApiKey()` 返回明文；`setApiKey()` 明文写入 store。
- **风险**：API Key 对任意前端 JS 可见，且默认持久化到 `%APPDATA%/.../settings.json`（明文）。若前端被注入脚本/恶意插件，密钥可被直接窃取。

### [HIGH] `src/store.ts:7` — 使用 `plugin-store` 持久化敏感信息但无加密/无系统密钥库
- **位置**：`load("settings.json", { autoSave: true })`
- **风险**：`settings.json` 为可读明文配置文件，不适合长期保存云 API Key。

### [MEDIUM] `src/store.ts:44-56, 59-62` — ASR 设置缺少运行时校验与约束
- **位置**：`getAsrSettings()` 直接 `...(v ?? {})`；`setAsrSettings()` 直接写入。
- **风险**：被篡改的 `settings.json` 可注入异常结构（类型错、超长字符串、非法 backend），导致前端状态异常，或把异常路径传给后端命令执行链（稳定性/安全边界变弱）。

### [MEDIUM] `src-tauri/src/convert.rs:18-83` — 高频繁简映射覆盖不足（常见字缺失）
- **位置**：映射表未覆盖大量高频繁体字（示例：`國/学/習/開發`中的 `國/學/習` 等，`國`、`學`、`習` 当前缺失）
- **风险**：输出“简体化”不稳定，常见 ASR 文本仍残留繁体，违背“输出始终简体”的产品目标。

### [MEDIUM] `src-tauri/src/convert.rs:96-105` — 缺少 Unicode 规范化步骤
- **位置**：`t2s()` 逐字符直接映射，无 NFC/NFKC 预处理。
- **风险**：兼容区汉字、全角/兼容字符、组合序列可能绕过映射，导致“视觉同字不转换”。

### [LOW] `src-tauri/src/convert.rs:24, 48, 57, 70, 71, 81-82` — 映射表重复条目
- **位置**：如 `於/應/變/處/準/間/後` 重复插入
- **风险**：功能上无立即错误（后写覆盖前写），但可维护性差、易掩盖真实缺项。

### [LOW] `src-tauri/src/convert.rs:100-101` — 每字符构造 `String` 查表有额外分配
- **位置**：`let key: String = ch.to_string(); t.get(key.as_str())`
- **风险**：长字幕文本下有不必要分配开销。

---

## 2) 详细分析与修复建议

## A. CSP 关闭（BLOCKER）
- **分析**：桌面端并不意味着可忽略 XSS。当前同时存在“前端可读 API Key + 文件系统能力 + IPC 命令”，`csp: null` 会把这些风险串联成完整攻击链。
- **修复建议**：
  1. 将 `csp` 改为严格白名单，至少限制为：
     - `default-src 'self'`
     - `script-src 'self'`
     - `style-src 'self' 'unsafe-inline'`（如 Tailwind 运行模式确实需要）
     - `connect-src 'self' https://dashscope.aliyuncs.com`（按实际域名收敛）
  2. 禁止 `eval` 与任意远程脚本源。
  3. 配套做一次前端依赖与 `dangerouslySetInnerHTML` 全量扫描。

## B. API Key 明文且前端可读（HIGH）
- **分析**：`store.ts` 将密钥作为普通配置处理；`getApiKey()` 直接回传给 React state。任何前端注入都可读。
- **修复建议**：
  1. **不要把 API Key 暴露给前端业务状态**。改为：
     - 前端仅调用 `set_api_key` / `clear_api_key` 命令；
     - 实际密钥只在 Rust 侧持有与使用（调用 ASR 时由后端读取）。
  2. 密钥存储改为系统密钥库（Windows Credential Manager / keyring）。
  3. 前端 UI 若需“已配置”状态，只返回布尔值 `hasApiKey`，不返回密钥明文。
  4. 关闭日志中任何可能输出 key 的路径（包括错误拼接）。

## C. ASR 设置缺少 schema 校验（MEDIUM）
- **分析**：`...(v ?? {})` 会无条件信任持久化内容。手工篡改配置可造成类型污染（例如 `backend: 123`）。
- **修复建议**：
  1. 在 `getAsrSettings()` 做显式字段校验与白名单修复（backend 仅 `qwen|local`）。
  2. 对路径字段长度、非法字符、空值语义做约束。
  3. `setAsrSettings()` 写入前执行同一套校验（前后端双重校验更稳妥）。

## D. convert 映射完整性不足（MEDIUM）
- **分析**：当前表为“精选”策略，但缺少多个超高频字会明显影响结果一致性（尤其口语+技术文案混合场景）。
- **修复建议**：
  1. 先补齐高频核心字集（如 `國學習業務門車書實際...` 等高频缺口）。
  2. 引入语料回归：用历史转写样本统计“未转换繁体TopN”，持续增量补表。
  3. 若体积可接受，评估用更小规模词典+按需加载替代手写表。

## E. Unicode 规范化缺失（MEDIUM）
- **分析**：视觉一致字符可能编码不同；仅按单码点硬匹配会漏转。
- **修复建议**：
  1. 在 `t2s` 前做 NFKC（或至少 NFC）规范化。
  2. 增加测试：兼容区字符、全角符号、组合序列样例。

## F. 重复条目与分配优化（LOW）
- **分析**：重复键降低可维护性；`String` 临时分配影响性能。
- **修复建议**：
  1. 映射改为 `HashMap<char, &'static str>`，避免 `to_string()`。
  2. 清理重复键，并加“重复键检测脚本/测试”防回归。

---

## 3) 整体代码质量评估

- **总体结论**：当前实现可用，但安全基线不足，尤其是 **CSP 关闭 + API Key 明文且前端可读** 形成高风险组合，需优先整改。
- **优先级建议**：
  1. **立即修复**：`csp: null`、API Key 存储与暴露模型（BLOCKER/HIGH）。
  2. **随后修复**：`convert.rs` 的高频缺字与 Unicode 规范化（MEDIUM）。
  3. **持续改进**：映射维护性与性能小优化（LOW）。

- **综合评级**：**Request Changes（不建议按当前状态通过安全审查）**。

