import type { SecurityLevel } from "./odoc";

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

/** A reference kept in composer state; its content is supplied to the draft, not shown inline. */
export interface DraftReference {
  id: string;
  kind: "知识库" | "上传文件";
  label: string;
  content: string;
  securityLevel?: SecurityLevel;
}

/** Builds the hidden material sent to the kernel: user brief followed by quoted sources. */
export function composeMaterialWithReferences(
  material: string,
  references: readonly DraftReference[],
): ReferenceAppendResult {
  let next = material.trim();
  for (const reference of references) {
    const result = appendMaterialReference(next, reference.kind, reference.label, reference.content);
    if (result.error) return result;
    next = result.material;
  }
  return { material: next, error: null };
}

export function referenceContentBytes(references: readonly DraftReference[]): number {
  return references.reduce((total, reference) => total + materialByteLength(reference.content), 0);
}

/** Mirrors the kernel's conservative handling of explicit, unknown classifications. */
export function sourceSecurityLevel(frontmatter: Record<string, string | undefined>): SecurityLevel {
  const value = frontmatter.security_level?.trim().toLowerCase();
  if (value === undefined || value === "internal" || value === "内部") return "internal";
  if (value === "public" || value === "公开") return "public";
  if (value === "secret" || value === "秘密") return "secret";
  return "confidential";
}

export function referenceSecurityLevel(references: readonly DraftReference[]): SecurityLevel {
  const levels: SecurityLevel[] = ["public", "internal", "secret", "confidential"];
  return references.reduce<SecurityLevel>((level, reference) => {
    // Old knowledge references without metadata must not silently become cloud-safe.
    const source = reference.securityLevel ?? (reference.kind === "知识库" ? "confidential" : "internal");
    return levels[Math.max(levels.indexOf(level), levels.indexOf(source))];
  }, "public");
}
