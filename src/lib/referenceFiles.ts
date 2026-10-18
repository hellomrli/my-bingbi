import { extractDocxText, extractReferenceText } from "./ipc";

/** Text formats that can be decoded in the webview without a desktop parser. */
export const TEXT_UPLOAD_EXTENSIONS = new Set(["txt", "md", "markdown", "csv", "json", "yaml", "yml"]);

/** Binary formats parsed by the Rust kernel before being stored as Markdown. */
export const BINARY_UPLOAD_EXTENSIONS = new Set(["pdf", "ofd", "doc", "docx", "xlsx", "xls", "wps"]);

export const MAX_KB_UPLOAD_BYTES = 32 * 1024 * 1024;
export const MAX_KB_NOTE_BYTES = 4 * 1024 * 1024;

export function fileExtension(name: string): string {
  const index = name.lastIndexOf(".");
  return index >= 0 ? name.slice(index + 1).toLowerCase() : "";
}

/** A filename stem is the default note title; the backend enforces the 120-char limit. */
export function titleFromFileName(name: string): string {
  const base = name.replace(/\.[^.]+$/, "").trim();
  return (base || "未命名资料").slice(0, 120);
}

export async function readUploadBytes(file: File): Promise<ArrayBuffer> {
  if (typeof file.arrayBuffer === "function") {
    try {
      return await file.arrayBuffer();
    } catch {
      // Fall back to FileReader for WebViews with a partial Blob implementation.
    }
  }
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      if (reader.result instanceof ArrayBuffer) resolve(reader.result);
      else reject(new Error("文件读取失败"));
    };
    reader.onerror = () => reject(reader.error ?? new Error("文件读取失败"));
    reader.readAsArrayBuffer(file);
  });
}

export function decodeUploadText(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  if (bytes.length >= 2 && bytes[0] === 0xff && bytes[1] === 0xfe) {
    return new TextDecoder("utf-16le").decode(bytes);
  }
  if (bytes.length >= 2 && bytes[0] === 0xfe && bytes[1] === 0xff) {
    return new TextDecoder("utf-16be").decode(bytes);
  }
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    // Windows-produced TXT/CSV files in Chinese offices are commonly GBK/GB18030.
    return new TextDecoder("gb18030", { fatal: true }).decode(bytes);
  }
}

/** Uploaded Markdown keeps its body but not a second YAML frontmatter block. */
export function stripMarkdownFrontmatter(text: string): string {
  if (!text.startsWith("---\n")) return text;
  const end = text.indexOf("\n---\n", 4);
  return end >= 0 ? text.slice(end + "\n---\n".length).replace(/^\n+/, "") : text;
}

export function textByteLength(text: string): number {
  return new TextEncoder().encode(text).length;
}

/**
 * Reads one uploaded file into plain text: text formats are decoded in the webview, binary
 * formats are parsed by the desktop kernel. The result is what gets stored as a Markdown note.
 */
export async function extractUploadedFileText(file: File, desktop: boolean): Promise<string> {
  if (file.size > MAX_KB_UPLOAD_BYTES) {
    throw new Error(`${file.name} 超过 32 MiB 上限`);
  }
  const extension = fileExtension(file.name);
  if (BINARY_UPLOAD_EXTENSIONS.has(extension)) {
    if (!desktop) {
      throw new Error(`${file.name} 需要桌面内核解析，浏览器预览不可用`);
    }
    const bytes = new Uint8Array(await readUploadBytes(file));
    const text = extension === "docx"
      ? await extractDocxText(bytes)
      : await extractReferenceText(file.name, bytes);
    return text.trim();
  }
  if (TEXT_UPLOAD_EXTENSIONS.has(extension) || file.type.startsWith("text/")) {
    return stripMarkdownFrontmatter(decodeUploadText(await readUploadBytes(file))).trim();
  }
  throw new Error(`${file.name} 的格式暂不支持`);
}

export function uploadAccept(): string {
  return [...TEXT_UPLOAD_EXTENSIONS, ...BINARY_UPLOAD_EXTENSIONS]
    .map((extension) => `.${extension}`)
    .join(",");
}
