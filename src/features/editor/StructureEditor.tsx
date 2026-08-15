import { AlignLeft, Heading1, Plus, Trash2 } from "lucide-react";
import { useEffect, useRef } from "react";
import type { Block, HeadingLevel } from "../../lib/odoc";
import { useWorkbenchStore } from "../../store/workbench";

const headingLabels: Record<HeadingLevel, string> = {
  l1: "一级 · 一、",
  l2: "二级 · （一）",
  l3: "三级 · 1.",
  l4: "四级 · （1）",
};

function EditableBlock({ block, index }: { block: Block; index: number }) {
  const selectedNodeId = useWorkbenchStore((state) => state.selectedNodeId);
  const selectNode = useWorkbenchStore((state) => state.selectNode);
  const updateBlock = useWorkbenchStore((state) => state.updateBlock);
  const removeBlock = useWorkbenchStore((state) => state.removeBlock);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const isTextBlock = block.type === "paragraph" || block.type === "heading" || block.type === "quote";

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
            onChange={(event) => updateBlock(block.id, { level: event.target.value as HeadingLevel })}
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
        onChange={(event) => updateBlock(block.id, { content: event.target.value })}
        placeholder={block.type === "heading" ? "输入标题，不含层次序号" : "输入正文内容"}
      />
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

export function StructureEditor() {
  const document = useWorkbenchStore((state) => state.document);
  const updateTitle = useWorkbenchStore((state) => state.updateTitle);
  const addBlock = useWorkbenchStore((state) => state.addBlock);

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
      </label>
      <div className="block-list">
        {document.body.blocks.map((block, index) => (
          <EditableBlock block={block} index={index} key={block.id} />
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
    </div>
  );
}
