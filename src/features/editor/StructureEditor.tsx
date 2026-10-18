import { AlignLeft, Heading1, Plus, Sparkles, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { Block, HeadingLevel } from "../../lib/odoc";
import { useWorkbenchStore } from "../../store/workbench";
import { BlockRewriteDialog } from "./BlockRewriteDialog";

const headingLabels: Record<HeadingLevel, string> = {
  l1: "一级 · 一、",
  l2: "二级 · （一）",
  l3: "三级 · 1.",
  l4: "四级 · （1）",
};

function EditableBlock({
  block,
  index,
  modelReady,
  onRewrite,
}: {
  block: Block;
  index: number;
  modelReady: boolean;
  onRewrite: (block: { id: string; content: string }) => void;
}) {
  const selectedNodeId = useWorkbenchStore((state) => state.selectedNodeId);
  const selectNode = useWorkbenchStore((state) => state.selectNode);
  const updateBlockText = useWorkbenchStore((state) => state.updateBlockText);
  const updateHeadingLevel = useWorkbenchStore((state) => state.updateHeadingLevel);
  const removeBlock = useWorkbenchStore((state) => state.removeBlock);
  const unverifiedTokens = useWorkbenchStore((state) => state.unverifiedTokens);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const isTextBlock = block.type === "paragraph" || block.type === "heading" || block.type === "quote";
  const unverified = unverifiedTokens[block.id] ?? [];

  useEffect(() => {
    if (selectedNodeId === block.id) textareaRef.current?.focus();
  }, [block.id, selectedNodeId]);

  if (!isTextBlock) {
    return (
      <article className="editor-block" data-node-id={block.id}>
        <span>此 M0 编辑器暂只编辑文字块。</span>
        <button className="icon-button small danger-action" type="button" onClick={() => removeBlock(block.id)} title="删除块">
          <Trash2 size={14} />
        </button>
      </article>
    );
  }

  return (
    <article
      className={selectedNodeId === block.id ? "editor-block is-selected" : "editor-block"}
      data-node-id={block.id}
      onClick={() => selectNode(block.id)}
    >
      <div className="block-meta">
        <span>{String(index + 1).padStart(2, "0")}</span>
        {block.type === "heading" ? (
          <select
            aria-label="标题层级"
            value={block.level}
            onChange={(event) => updateHeadingLevel(block.id, event.target.value as HeadingLevel)}
          >
            {Object.entries(headingLabels).map(([value, label]) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </select>
        ) : (
          <span>{block.type === "quote" ? "引用" : "正文"}</span>
        )}
      </div>
      <textarea
        ref={textareaRef}
        rows={Math.max(2, Math.ceil(block.content.length / 34))}
        value={block.content}
        onChange={(event) => updateBlockText(block.id, event.target.value)}
        placeholder={block.type === "heading" ? "输入标题，不含层次序号" : "输入正文内容"}
      />
      {unverified.length > 0 && (
        <span className="fact-unverified-badge" title="这些数字/日期/文号未在已核实事实台账中找到">
          <span aria-hidden="true">⚠</span> 待核实：{unverified.join("、")}
        </span>
      )}
      <button
        className="icon-button small rewrite-action"
        type="button"
        onClick={(event) => {
          event.stopPropagation();
          onRewrite(block);
        }}
        disabled={!modelReady}
        title={modelReady ? "用模型改写本段，应用前可预览" : "选择并启用模型服务后可改写"}
        aria-label={`AI 改写第 ${index + 1} 段`}
      >
        <Sparkles size={14} />
      </button>
      <button
        className="icon-button small danger-action"
        type="button"
        onClick={(event) => {
          event.stopPropagation();
          removeBlock(block.id);
        }}
        title="删除块"
        aria-label="删除块"
      >
        <Trash2 size={14} />
      </button>
    </article>
  );
}

interface StructureEditorProps {
  providerId?: string;
  providerName?: string;
  modelReady?: boolean;
}

export function StructureEditor({
  providerId = "",
  providerName = "",
  modelReady = false,
}: StructureEditorProps) {
  const document = useWorkbenchStore((state) => state.document);
  const updateTitle = useWorkbenchStore((state) => state.updateTitle);
  const updateBlockText = useWorkbenchStore((state) => state.updateBlockText);
  const addBlock = useWorkbenchStore((state) => state.addBlock);
  const selectNode = useWorkbenchStore((state) => state.selectNode);
  const unverifiedTokens = useWorkbenchStore((state) => state.unverifiedTokens);
  const titleTokens = unverifiedTokens[document.body.title.id] ?? [];
  const [rewriteTarget, setRewriteTarget] = useState<{ id: string; content: string } | null>(null);
  const headings = document.body.blocks.filter(
    (block): block is Extract<Block, { type: "heading" }> => block.type === "heading",
  );

  function jumpToBlock(blockId: string) {
    selectNode(blockId);
    requestAnimationFrame(() => {
      window.document
        .querySelector<HTMLElement>(`[data-node-id="${blockId}"]`)
        ?.scrollIntoView({ block: "center", behavior: "smooth" });
    });
  }

  return (
    <div className="structure-editor" data-node-id={document.body.id}>
      <label className="title-editor" data-node-id={document.body.title.id}>
        <span>公文标题</span>
        <textarea
          rows={2}
          value={document.body.title.text}
          onChange={(event) => updateTitle(event.target.value)}
          placeholder="输入公文标题"
        />
        {titleTokens.length > 0 && (
          <span className="fact-unverified-badge" title="标题中的数字/日期/文号未在已核实事实台账中找到">
            <span aria-hidden="true">⚠</span> 待核实：{titleTokens.join("、")}
          </span>
        )}
      </label>
      {headings.length > 0 && (
        <details className="editor-outline">
          <summary>文档大纲 · {headings.length} 个标题</summary>
          <ol>
            {headings.map((heading) => (
              <li className={`outline-level-${heading.level}`} key={heading.id}>
                <button type="button" onClick={() => jumpToBlock(heading.id)}>
                  {heading.content.trim() || "（未命名标题）"}
                </button>
              </li>
            ))}
          </ol>
        </details>
      )}
      <div className="block-list">
        {document.body.blocks.map((block, index) => (
          <EditableBlock
            block={block}
            index={index}
            key={block.id}
            modelReady={modelReady}
            onRewrite={(target) => setRewriteTarget({ id: target.id, content: target.content })}
          />
        ))}
      </div>
      <div className="add-block-actions">
        <button className="command-button" type="button" onClick={() => addBlock("paragraph")}>
          <AlignLeft size={15} />
          正文段落
          <Plus size={13} />
        </button>
        <button className="command-button" type="button" onClick={() => addBlock("heading")}>
          <Heading1 size={15} />
          层次标题
          <Plus size={13} />
        </button>
      </div>
      <BlockRewriteDialog
        open={rewriteTarget !== null}
        blockId={rewriteTarget?.id ?? ""}
        original={rewriteTarget?.content ?? ""}
        document={document}
        providerId={providerId}
        providerName={providerName}
        onApply={(blockId, text) => updateBlockText(blockId, text)}
        onClose={() => setRewriteTarget(null)}
      />
    </div>
  );
}
