import { load, Store } from "@tauri-apps/plugin-store";

let cached: Store | null = null;

export async function getStore(): Promise<Store> {
  if (!cached) {
    cached = await load("settings.json", { autoSave: true });
  }
  return cached;
}

export async function getApiKey(): Promise<string> {
  const store = await getStore();
  const v = await store.get<string>("apiKey");
  return v ?? "";
}

export async function setApiKey(key: string): Promise<void> {
  const store = await getStore();
  await store.set("apiKey", key);
  await store.save();
}

export async function clearApiKey(): Promise<void> {
  const store = await getStore();
  await store.delete("apiKey");
  await store.save();
}

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
  return {
    backend: "qwen",
    language: "",
    enableItn: false,
    model: "qwen3-asr-flash",
    whisperExe: "",
    whisperModel: "",
    whisperLanguage: "",
    ...(v ?? {}),
  };
}

export async function setAsrSettings(s: AsrSettings): Promise<void> {
  const store = await getStore();
  await store.set(KEY, s);
  await store.save();
}
