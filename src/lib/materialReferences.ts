/** Material reference markers shared by the composer and the draft pipeline. */
export interface MaterialReference {
  kind: "知识库" | "上传文件";
  label: string;
  start: number;
  end: number;
}

export const MAX_DRAFT_MATERIAL_BYTES = 2 * 1024 * 1024;
export const REFERENCE_START_PREFIX = "【参考来源｜";
export const REFERENCE_END_MARKER = "【参考来源结束】";

export function materialByteLength(value: string): number {
  return new TextEncoder().encode(value).length;
}

export function safeReferenceLabel(value: string): string {
  return value
    // eslint-disable-next-line no-control-regex -- strip control characters from filenames
    .replace(/[\u0000-\u001f\u007f]/g, " ")
    .replaceAll("【", "［")
    .replaceAll("】", "］")
    .trim()
    .slice(0, 160) || "未命名参考文件";
}

function safeReferenceContent(value: string): string {
  return value
    .replaceAll(REFERENCE_START_PREFIX, "［参考来源｜")
    .replaceAll(REFERENCE_END_MARKER, "［参考来源结束］");
}

export function parseMaterialReferences(value: string): MaterialReference[] {
  const pattern = /【参考来源｜(知识库|上传文件)：([^\r\n】]+)】\r?\n[\s\S]*?\r?\n【参考来源结束】/g;
  return Array.from(value.matchAll(pattern), (match) => ({
    kind: match[1] as MaterialReference["kind"],
    label: match[2],
    start: match.index,
    end: match.index + match[0].length,
  }));
}

export interface ReferenceAppendResult {
  material: string;
  error: string | null;
}

/** Appends one quoted source; the markers keep the source distinct from the user's own words. */
export function appendMaterialReference(
  material: string,
  kind: MaterialReference["kind"],
  label: string,
  content: string,
): ReferenceAppendResult {
  const safeLabel = safeReferenceLabel(label);
  const cleaned = safeReferenceContent(content.trim());
  if (!cleaned) {
    return { material, error: `${safeLabel} 没有可读取的文字。` };
  }
  if (cleaned.includes("\0")) {
    return { material, error: `${safeLabel} 含有不可处理的控制字符，请先转换为纯文本。` };
  }
  const marker = `${REFERENCE_START_PREFIX}${kind}：${safeLabel}】`;
  if (material.includes(marker)) {
    return { material, error: `${safeLabel} 已经加入写作材料。` };
  }
  const source = `${marker}\n${cleaned}\n${REFERENCE_END_MARKER}`;
  const next = material.trim() ? `${material.trim()}\n\n${source}` : source;
  if (materialByteLength(next) > MAX_DRAFT_MATERIAL_BYTES) {
    return { material, error: "加入后将超过 2 MiB 上限，请先摘取所需段落。" };
  }
  return { material: next, error: null };
}

export function removeMaterialReference(material: string, reference: MaterialReference): string {
  const before = material.slice(0, reference.start).trimEnd();
  const after = material.slice(reference.end).trimStart();
  return before && after ? `${before}\n\n${after}` : before || after;
}
