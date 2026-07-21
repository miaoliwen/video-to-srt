import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { getApiKey, setApiKey, clearApiKey, getAsrSettings, setAsrSettings, type AsrSettings, type AsrBackend } from "./store";

type Stage = "idle" | "extracting" | "transcribing" | "done" | "error";

interface ProgressEvent {
  stage: Stage | string;
  message: string;
  progress?: number | null;
}

interface Segment {
  start: number;
  end: number;
  text: string;
}

const STAGE_LABEL: Record<string, string> = {
  idle: "待开始",
  extracting: "提取音频",
  transcribing: "语音识别",
  done: "完成",
  error: "出错",
};

export default function App() {
  const [apiKey, setApiKeyState] = useState<string>("");
  const [settings, setSettings] = useState<AsrSettings>({
    backend: "qwen",
    language: "",
    enableItn: false,
    model: "qwen3-asr-flash",
    whisperExe: "",
    whisperModel: "",
    whisperLanguage: "",
  });
  const [videoPath, setVideoPath] = useState<string>("");
  const [duration, setDuration] = useState<number>(0);
  const [segments, setSegments] = useState<Segment[]>([]);
  const [srt, setSrt] = useState<string>("");
  const [stage, setStage] = useState<Stage>("idle");
  const [progress, setProgress] = useState<number>(0);
  const [log, setLog] = useState<string[]>([]);
  const [running, setRunning] = useState<boolean>(false);
  const [settingsOpen, setSettingsOpen] = useState<boolean>(false);
  const [draftKey, setDraftKey] = useState<string>("");
  const [keyVisible, setKeyVisible] = useState<boolean>(false);
  const [ffmpegPath, setFfmpegPath] = useState<string>("");
  const [dropping, setDropping] = useState<boolean>(false);
  const logRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const k = await getApiKey();
        setApiKeyState(k);
      } catch (e) {
        console.warn("load api key failed", e);
      }
      try {
        const s = await getAsrSettings();
        setSettings(s);
      } catch (e) {
        console.warn("load asr settings failed", e);
      }
      try {
        const p = await invoke<string>("locate_ffmpeg_binary");
        setFfmpegPath(p);
      } catch (_) { /* no ffmpeg yet */ }
    })();
  }, []);

  useEffect(() => {
    const un = listen<ProgressEvent>("pipeline-progress", (e) => {
      const ev = e.payload;
      if (ev.progress != null) setProgress(ev.progress);
      if (typeof ev.stage === "string") setStage(ev.stage as Stage);
      setLog((prev) => [...prev, `[${new Date().toLocaleTimeString()}] ${ev.message}`]);
    });
    return () => { un.then((f) => f()); };
  }, []);

  useEffect(() => {
    logRef.current?.scrollTo({ top: logRef.current.scrollHeight });
  }, [log]);

  const appendLog = useCallback((line: string) => {
    setLog((p) => [...p, `[${new Date().toLocaleTimeString()}] ${line}`]);
  }, []);

  const chooseVideo = useCallback(async () => {
    const sel = await openDialog({
      multiple: false,
      title: "选择视频文件",
      filters: [
        { name: "视频", extensions: ["mp4", "mov", "mkv", "avi", "flv", "webm", "wmv", "m4v"] },
      ],
    });
    if (typeof sel === "string") setVideoPath(sel);
  }, []);

  // Generic file/path picker used inside the settings dialog.
  const pickPath = useCallback(
    async (field: "whisperExe" | "whisperModel", title: string, filters: { name: string; extensions: string[] }[]) => {
      const sel = await openDialog({ multiple: false, title, filters });
      if (typeof sel === "string") {
        setSettings((s) => ({ ...s, [field]: sel }));
      }
    },
    [],
  );

  const handleDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setDropping(false);
    const files = Array.from(e.dataTransfer.files || []);
    if (files.length === 0) return;
    const first = files[0] as File & { path?: string };
    const p: string | undefined = first.path;
    if (p) setVideoPath(p);
  }, []);

  const runPipeline = useCallback(async () => {
    if (!videoPath) return;
    if (settings.backend === "qwen" && !apiKey) {
      setSettingsOpen(true);
      appendLog("请先在设置中填写 API Key");
      return;
    }
    if (settings.backend === "local" && (!settings.whisperExe || !settings.whisperModel)) {
      setSettingsOpen(true);
      appendLog("本地模式请先在设置中配置 whisper-cli 路径与模型路径");
      return;
    }
    if (!ffmpegPath) {
      appendLog("未检测到 ffmpeg，请在 src-tauri/binaries/ 放置 ffmpeg.exe");
      return;
    }
    setRunning(true);
    setSegments([]); setSrt(""); setProgress(0); setLog([]); setStage("extracting");
    try {
      const ext = await invoke<{ audio_path: string; duration_secs: number }>(
        "extract_audio",
        { videoPath },
      );
      setDuration(ext.duration_secs);
      appendLog(`音频已生成: ${ext.audio_path}（${ext.duration_secs.toFixed(1)}s）`);

      const tx = await invoke<{ segments: Segment[]; srt: string }>("transcribe", {
        audioPath: ext.audio_path,
        backend: settings.backend,
        apiKey,
        model: settings.model,
        language: settings.backend === "qwen" ? (settings.language.trim() || null) : null,
        enableItn: settings.enableItn,
        whisperExe: settings.whisperExe || null,
        whisperModel: settings.whisperModel || null,
        whisperLanguage: settings.whisperLanguage.trim() || null,
        totalDurationSecs: ext.duration_secs,
      });
      setSegments(tx.segments);
      setSrt(tx.srt);
      setStage("done");
      appendLog(`字幕生成完成，共 ${tx.segments.length} 段`);
    } catch (err: unknown) {
      setStage("error");
      const msg = typeof err === "string" ? err : (err as { message?: string })?.message ?? JSON.stringify(err);
      appendLog(`失败: ${msg}`);
    } finally {
      setRunning(false);
    }
  }, [videoPath, apiKey, ffmpegPath, settings, appendLog]);

  const exportSrt = useCallback(async () => {
    if (!srt) return;
    const parts = videoPath.split(/[\\/]/);
    const defaultName = videoPath
      ? (parts.pop() || "subtitle").replace(/\.[^.]+$/, "") + ".srt"
      : "subtitle.srt";
    const target = await saveDialog({
      title: "保存 SRT 字幕",
      defaultPath: defaultName,
      filters: [{ name: "SRT 字幕", extensions: ["srt"] }],
    });
    if (!target) return;
    try {
      const written = await invoke<string>("save_srt", { outputPath: target, content: srt });
      appendLog(`已保存: ${written}`);
    } catch (err) {
      appendLog(`保存失败: ${String(err)}`);
    }
  }, [srt, videoPath, appendLog]);

  const downloadSrt = useCallback(() => {
    if (!srt) return;
    // Prepend UTF-8 BOM so Chinese captions display correctly in players and
    // editors that default to ANSI when no BOM is present.
    const bom = new Uint8Array([0xef, 0xbb, 0xbf]);
    const encoder = new TextEncoder();
    const payload = encoder.encode(srt);
    const blob = new Blob([bom, payload], { type: "application/x-subrip;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    const parts = videoPath.split(/[\\/]/);
    const base = videoPath ? (parts.pop() || "subtitle").replace(/\.[^.]+$/, "") : "subtitle";
    a.download = `${base}.srt`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }, [srt, videoPath]);

  const totalDurationLabel = useMemo(() => {
    if (!duration) return "—";
    const mm = Math.floor(duration / 60);
    const ss = Math.round(duration % 60);
    return `${mm}m ${ss}s`;
  }, [duration]);

  return (
    <div className="min-h-full flex flex-col">
      <header className="flex items-center justify-between px-6 py-3 border-b border-white/10 bg-gradient-to-b from-[#101935] to-[#0b1020]">
        <div className="flex items-center gap-3">
          <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-sky-500 to-indigo-600 flex items-center justify-center text-white font-bold">S</div>
          <div>
            <div className="font-semibold tracking-wide">字幕生成工作台</div>
            <div className="text-xs text-white/50">视频 → 16k 音频 → Qwen ASR → SRT</div>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-xs text-white/40">{ffmpegPath ? "ffmpeg ✓" : "ffmpeg ✗"}</span>
          <button
            className="text-sm px-3 py-1.5 rounded-md bg-white/5 hover:bg-white/10 border border-white/10"
            onClick={() => { setDraftKey(apiKey); setSettingsOpen(true); }}
          >
            ⚙ 设置
          </button>
        </div>
      </header>

      <main className="flex-1 grid grid-cols-12 gap-4 p-4">
        <section className="col-span-12 lg:col-span-5 space-y-4">
          <div
            className={`rounded-xl border-2 border-dashed p-6 transition-colors cursor-pointer ${
              dropping ? "border-sky-400 bg-sky-400/10" : "border-white/15 bg-white/5 hover:bg-white/[0.07]"
            }`}
            onClick={chooseVideo}
            onDragOver={(e) => { e.preventDefault(); setDropping(true); }}
            onDragLeave={() => setDropping(false)}
            onDrop={handleDrop}
          >
            <div className="text-sm text-white/60 mb-2">{videoPath ? "已选择视频：" : "拖入视频或点击选择文件"}</div>
            <div className="text-xs break-all font-mono text-white/80 bg-black/30 p-2 rounded max-h-24 overflow-auto">
              {videoPath || "（未选择）"}
            </div>
            <div className="mt-3 text-xs text-white/50">
              支持 MP4 / MOV / MKV / AVI / FLV / WEBM 等。时长：{totalDurationLabel}
            </div>
          </div>

          <div className="rounded-xl bg-white/5 border border-white/10 p-4">
            <div className="flex items-center justify-between mb-2">
              <div className="text-sm text-white/70">处理进度</div>
              <div className="text-xs text-white/50">阶段：{STAGE_LABEL[stage] ?? stage} · {Math.round(progress * 100)}%</div>
            </div>
            <div className="w-full h-2 rounded-full bg-white/10 overflow-hidden">
              <div
                className={`h-full transition-all duration-300 ${
                  stage === "error" ? "bg-rose-500" : "bg-gradient-to-r from-sky-400 to-indigo-500"
                }`}
                style={{ width: `${Math.max(2, Math.round(progress * 100))}%` }}
              />
            </div>
            <div className="mt-3 grid grid-cols-4 gap-2 text-xs">
              <Step n="1" label="上传" done={!!videoPath} active={stage === "extracting"} />
              <Step n="2" label="拆音频" done={["transcribing","done"].includes(stage)} active={stage === "extracting"} />
              <Step n="3" label="ASR 识别" done={stage === "done"} active={stage === "transcribing"} />
              <Step n="4" label="导出 SRT" done={false} active={stage === "done"} />
            </div>
            <div className="mt-4 flex gap-2">
              <button
                disabled={!videoPath || running}
                className="flex-1 py-2 rounded-md bg-sky-500 hover:bg-sky-400 disabled:bg-white/10 disabled:text-white/40 transition-colors text-sm font-medium"
                onClick={runPipeline}
              >
                {running ? "正在处理…" : "开始识别"}
              </button>
              <button
                disabled={!srt}
                className="px-4 py-2 rounded-md bg-white/10 hover:bg-white/20 disabled:opacity-40 text-sm"
                onClick={downloadSrt}
              >
                直接下载
              </button>
              <button
                disabled={!srt}
                className="px-4 py-2 rounded-md bg-emerald-500 hover:bg-emerald-400 disabled:opacity-40 text-sm font-medium"
                onClick={exportSrt}
              >
                导出…
              </button>
            </div>
          </div>

          <div className="rounded-xl bg-black/40 border border-white/10 p-3">
            <div className="text-xs text-white/50 mb-2">运行日志</div>
            <div ref={logRef} className="h-40 overflow-auto text-xs font-mono text-white/70 leading-relaxed">
              {log.length === 0 ? <div className="text-white/30">暂无日志</div> : log.map((l, i) => <div key={i}>{l}</div>)}
            </div>
          </div>
        </section>

        <section className="col-span-12 lg:col-span-7 rounded-xl bg-white/5 border border-white/10 p-4 flex flex-col">
          <div className="flex items-center justify-between mb-3">
            <div className="text-sm text-white/70">字幕预览（共 {segments.length} 段）</div>
            <div className="flex gap-2">
              <button
                className="text-xs px-2 py-1 rounded bg-white/10 hover:bg-white/20 disabled:opacity-40"
                disabled={!srt}
                onClick={() => navigator.clipboard.writeText(srt)}
              >
                复制 SRT
              </button>
            </div>
          </div>
          {segments.length === 0 ? (
            <div className="flex-1 flex items-center justify-center text-white/40 text-sm">
              还没有字幕，先选个视频点"开始识别"
            </div>
          ) : (
            <div className="flex-1 overflow-auto space-y-1 pr-1">
              {segments.map((seg, i) => (
                <div key={i} className="grid grid-cols-12 gap-2 px-3 py-2 rounded hover:bg-white/5 group">
                  <div className="col-span-2 text-xs font-mono text-sky-300 select-none">#{i + 1}</div>
                  <div className="col-span-3 text-xs font-mono text-white/50 select-none">{fmt(seg.start)} → {fmt(seg.end)}</div>
                  <div className="col-span-9 lg:col-span-7 text-sm text-white/90 whitespace-pre-wrap break-words">{seg.text}</div>
                </div>
              ))}
            </div>
          )}
          {srt && (
            <details className="mt-3 text-xs">
              <summary className="cursor-pointer text-white/50 hover:text-white/80">查看 SRT 原文</summary>
              <pre className="mt-2 p-3 bg-black/40 rounded max-h-60 overflow-auto whitespace-pre-wrap text-white/70 font-mono">{srt}</pre>
            </details>
          )}
        </section>
      </main>

      {settingsOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur" onClick={() => setSettingsOpen(false)}>
          <div className="w-[560px] max-w-[94vw] rounded-xl bg-[#0f172a] border border-white/10 p-5 shadow-2xl" onClick={(e) => e.stopPropagation()}>
            <div className="flex items-center justify-between mb-3">
              <div className="text-lg font-semibold">设置</div>
              <button className="text-white/50 hover:text-white" onClick={() => setSettingsOpen(false)}>✕</button>
            </div>

            <div className="flex gap-1 mb-4 rounded-md bg-black/40 p-1 border border-white/10">
              <button
                className={`flex-1 py-1.5 rounded text-sm ${settings.backend === "qwen" ? "bg-sky-500 text-white" : "text-white/60 hover:text-white/90"}`}
                onClick={() => setSettings({ ...settings, backend: "qwen" as AsrBackend })}
              >
                Qwen ASR（云端）
              </button>
              <button
                className={`flex-1 py-1.5 rounded text-sm ${settings.backend === "local" ? "bg-sky-500 text-white" : "text-white/60 hover:text-white/90"}`}
                onClick={() => setSettings({ ...settings, backend: "local" as AsrBackend })}
              >
                本地 Whisper.cpp
              </button>
            </div>

            {settings.backend === "qwen" ? (
              <>
                <label className="block text-sm text-white/70 mb-1">阿里云百炼 API Key</label>
                <div className="flex gap-2">
                  <input
                    type={keyVisible ? "text" : "password"}
                    value={draftKey}
                    onChange={(e) => setDraftKey(e.target.value)}
                    placeholder="sk-xxxxxxxxxxxxxxxxxxxxxxxx"
                    className="flex-1 px-3 py-2 rounded bg-black/40 border border-white/10 focus:outline-none focus:border-sky-500 font-mono text-sm"
                  />
                  <button className="px-3 rounded bg-white/10 hover:bg-white/20 text-xs" onClick={() => setKeyVisible((v) => !v)}>
                    {keyVisible ? "隐藏" : "显示"}
                  </button>
                </div>
                <div className="text-xs text-white/50 mt-2">
                  密钥仅保存在本机（{`%APPDATA%/字幕生成工作台/settings.json`}），不会上传任何服务器。
                </div>

                <div className="mt-4 grid grid-cols-2 gap-3">
                  <div>
                    <label className="block text-sm text-white/70 mb-1">音频语种（可选）</label>
                    <select
                      value={settings.language}
                      onChange={(e) => setSettings({ ...settings, language: e.target.value })}
                      className="w-full px-3 py-2 rounded bg-black/40 border border-white/10 focus:outline-none focus:border-sky-500 text-sm"
                    >
                      <option value="">自动识别</option>
                      <option value="zh">中文（含普通话/四川话/闽南语/吴语）</option>
                      <option value="yue">粤语</option>
                      <option value="en">英文</option>
                      <option value="ja">日语</option>
                      <option value="ko">韩语</option>
                      <option value="de">德语</option>
                      <option value="fr">法语</option>
                      <option value="ru">俄语</option>
                      <option value="es">西班牙语</option>
                      <option value="pt">葡萄牙语</option>
                      <option value="it">意大利语</option>
                    </select>
                  </div>
                  <div>
                    <label className="block text-sm text-white/70 mb-1">ITN（数字归一化）</label>
                    <label className="flex items-center gap-2 px-3 py-2 rounded bg-black/40 border border-white/10 text-sm cursor-pointer">
                      <input
                        type="checkbox"
                        checked={settings.enableItn}
                        onChange={(e) => setSettings({ ...settings, enableItn: e.target.checked })}
                        className="accent-sky-500"
                      />
                      <span>开启（仅中英文生效）</span>
                    </label>
                  </div>
                </div>
              </>
            ) : (
              <>
                <label className="block text-sm text-white/70 mb-1">whisper-cli.exe 路径</label>
                <PathRow
                  value={settings.whisperExe}
                  onPick={async () => pickPath("whisperExe", "请选择 whisper-cli.exe", [{ name: "EXE", extensions: ["exe"] }])}
                  onChange={(v) => setSettings({ ...settings, whisperExe: v })}
                  placeholder="D:\\tools\\whisper.cpp\\build\\bin\\Release\\whisper-cli.exe"
                />

                <label className="block text-sm text-white/70 mb-1 mt-3">whisper 模型路径（.bin）</label>
                <PathRow
                  value={settings.whisperModel}
                  onPick={async () => pickPath("whisperModel", "请选择 ggml 模型文件", [{ name: "GGML model", extensions: ["bin"] }])}
                  onChange={(v) => setSettings({ ...settings, whisperModel: v })}
                  placeholder="D:\\whisper-models\\ggml-base.bin"
                />

                <label className="block text-sm text-white/70 mb-1 mt-3">Whisper 语言（可选）</label>
                <input
                  type="text"
                  value={settings.whisperLanguage}
                  onChange={(e) => setSettings({ ...settings, whisperLanguage: e.target.value })}
                  placeholder="留空自动识别；中文填 zh、英文 en 等"
                  className="w-full px-3 py-2 rounded bg-black/40 border border-white/10 focus:outline-none focus:border-sky-500 text-sm"
                />
                <div className="text-xs text-white/50 mt-2">
                  本地模式调用 <span className="font-mono">whisper-cli.exe</span>，完全离线，无需 API Key。
                </div>
              </>
            )}

            <div className="flex justify-end gap-2 mt-5">
              {settings.backend === "qwen" && (
                <button
                  className="px-3 py-2 rounded text-sm text-rose-300 hover:bg-rose-500/10"
                  onClick={async () => { await clearApiKey(); setApiKeyState(""); setDraftKey(""); }}
                >
                  清除密钥
                </button>
              )}
              <div className="flex-1" />
              <button className="px-3 py-2 rounded bg-white/10 hover:bg-white/20 text-sm" onClick={() => setSettingsOpen(false)}>取消</button>
              <button
                className="px-4 py-2 rounded bg-sky-500 hover:bg-sky-400 text-sm font-medium"
                onClick={async () => {
                  if (settings.backend === "qwen") {
                    const k = draftKey.trim();
                    await setApiKey(k);
                    setApiKeyState(k);
                  }
                  await setAsrSettings(settings);
                  setSettingsOpen(false);
                }}
              >
                保存
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function Step({ n, label, done, active }: { n: string; label: string; done: boolean; active: boolean }) {
  return (
    <div className={`flex flex-col items-center gap-1 py-2 rounded ${active ? "bg-sky-500/15" : ""}`}>
      <div className={`w-6 h-6 rounded-full flex items-center justify-center text-xs font-bold ${
        done ? "bg-emerald-500 text-black" : active ? "bg-sky-500 text-white" : "bg-white/10 text-white/60"
      }`}>{done ? "✓" : n}</div>
      <div className="text-white/70">{label}</div>
    </div>
  );
}

interface PathRowProps {
  value: string;
  onChange: (v: string) => void;
  onPick: () => void | Promise<void>;
  placeholder?: string;
}

function PathRow({ value, onChange, onPick, placeholder }: PathRowProps) {
  return (
    <div className="flex gap-2">
      <input
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="flex-1 px-3 py-2 rounded bg-black/40 border border-white/10 focus:outline-none focus:border-sky-500 font-mono text-xs"
      />
      <button
        className="px-3 py-2 rounded bg-white/10 hover:bg-white/20 text-xs"
        onClick={() => { void onPick(); }}
      >
        浏览…
      </button>
    </div>
  );
}

function fmt(secs: number) {
  if (!isFinite(secs) || secs < 0) return "00:00.0";
  const mm = Math.floor(secs / 60);
  const ss = (secs - mm * 60);
  return `${String(mm).padStart(2, "0")}:${ss.toFixed(1).padStart(4, "0")}`;
}
