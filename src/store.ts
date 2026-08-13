import { load, Store } from "@tauri-apps/plugin-store";

let cached: Store | null = null;

export async function getStore(): Promise<Store> {
  if (!cached) {
    cached = await load("settings.json", { autoSave: true });
  }
  return cached;
}

// NOTE: the API key is intentionally NOT managed here. It lives on the Rust
// side (`get_has_api_key` / `set_api_key` / `clear_api_key` commands) so the
// frontend only ever sees whether one is configured, never the key itself.

export type AsrBackend = "qwen" | "local";

export interface AsrSettings {
  backend: AsrBackend;
  language: string;       // Qwen-only language hint (BCP-ish: zh, en, ...)
  enableItn: boolean;
  model: string;          // Qwen model name
  whisperExe: string;     // path to whisper-cli.exe
  whisperModel: string;   // path to ggml model file
  whisperLanguage: string; // '' = auto
}

const KEY = "asrSettings";

export async function getAsrSettings(): Promise<AsrSettings> {
  const store = await getStore();
  const v = (await store.get<AsrSettings>(KEY)) ?? null;
  const merged = {
    backend: "qwen",
    language: "",
    enableItn: false,
    model: "qwen3-asr-flash",
    whisperExe: "",
    whisperModel: "",
    whisperLanguage: "",
    ...(v ?? {}),
  };
  // Normalize a corrupted/tampered backend value: anything that isn't the
  // literal "local" falls back to the default cloud backend, so the UI and
  // the pipeline can never disagree about which backend is selected.
  return {
    ...merged,
    backend: merged.backend === "local" ? "local" : "qwen",
  };
}

export async function setAsrSettings(s: AsrSettings): Promise<void> {
  const store = await getStore();
  await store.set(KEY, s);
  await store.save();
}
