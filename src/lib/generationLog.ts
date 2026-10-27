const STORAGE_KEY = "bingbi.generation-log.v1";
const MAX_ENTRIES = 100;
const EVENT_NAME = "bingbi:generation-log";

export type GenerationLogLevel = "info" | "error";

export interface GenerationLogEntry {
  id: string;
  at: number;
  level: GenerationLogLevel;
  event: "start" | "stage" | "complete" | "error" | "blocked";
  message: string;
  stage?: string | null;
  code?: string | null;
  providerId?: string | null;
  pipeline?: string | null;
  documentId?: string | null;
  sessionId?: string | null;
}

export type GenerationLogInput = Omit<GenerationLogEntry, "id" | "at">;

function readRaw(): GenerationLogEntry[] {
  if (typeof window === "undefined") return [];
  try {
    const parsed: unknown = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "[]");
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((entry): entry is GenerationLogEntry => (
      typeof entry === "object"
      && entry !== null
      && typeof (entry as GenerationLogEntry).message === "string"
      && typeof (entry as GenerationLogEntry).at === "number"
    ));
  } catch {
    return [];
  }
}

/** Appends one diagnostic entry; capped and best-effort so logging never breaks a draft. */
export function appendGenerationLog(input: GenerationLogInput): void {
  if (typeof window === "undefined") return;
  try {
    const entries = readRaw();
    const entry: GenerationLogEntry = {
      ...input,
      id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
      at: Date.now(),
    };
    const next = [...entries, entry].slice(-MAX_ENTRIES);
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
    window.dispatchEvent(new Event(EVENT_NAME));
  } catch {
    // Local storage may be unavailable or full; generation must continue regardless.
  }
}

/** Newest first, so the failure that just happened is at the top. */
export function readGenerationLogs(): GenerationLogEntry[] {
  // Storage order is append order; reverse it instead of sorting by `at` so entries written in
  // the same millisecond keep their real sequence.
  return readRaw().reverse();
}

export function clearGenerationLogs(): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.removeItem(STORAGE_KEY);
    window.dispatchEvent(new Event(EVENT_NAME));
  } catch {
    // Nothing useful to do when storage is unavailable.
  }
}

export function subscribeGenerationLogs(listener: () => void): () => void {
  if (typeof window === "undefined") return () => {};
  const onStorage = (event: StorageEvent) => {
    if (event.key === STORAGE_KEY) listener();
  };
  window.addEventListener(EVENT_NAME, listener);
  window.addEventListener("storage", onStorage);
  return () => {
    window.removeEventListener(EVENT_NAME, listener);
    window.removeEventListener("storage", onStorage);
  };
}

export function formatGenerationLogs(entries: GenerationLogEntry[]): string {
  return entries.map((entry) => {
    const time = new Date(entry.at).toLocaleString("zh-CN", { hour12: false });
    const bits = [time, entry.level.toUpperCase(), entry.event];
    if (entry.stage) bits.push(entry.stage);
    bits.push(entry.message);
    if (entry.code) bits.push(`[${entry.code}]`);
    if (entry.sessionId) bits.push(`session=${entry.sessionId}`);
    return bits.join(" ");
  }).join("\n");
}
