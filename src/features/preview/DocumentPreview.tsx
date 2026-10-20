import type { ReactNode } from "react";
import type { Block } from "../../lib/odoc";
import { createHeadingCounters, formatHeading } from "../../lib/headingNumbering";
import { markUnverifiedSegments } from "../../lib/lint";
import { useWorkbenchStore } from "../../store/workbench";

/** Wraps every unverified fact token in a red mark; plain text otherwise. */
function renderMarked(text: string, tokens: string[] | undefined): ReactNode {
  if (!tokens || tokens.length === 0) return text;
  return markUnverifiedSegments(text, tokens).map((segment, index) =>
    segment.unverified ? (
      <mark className="fact-unverified" key={index}>{segment.text}</mark>
    ) : (
      segment.text
    ),
  );
}

function renderBlocks(blocks: Block[], tokens: Record<string, string[]>) {
  const counters = createHeadingCounters();

  return blocks.map((block) => {
    if (block.type === "heading") {
      const content = formatHeading(block.content || "未填写标题", block.level, counters);
      return (
        <p className={`official-heading ${block.level}`} id={`preview-${block.id}`} key={block.id}>
          {renderMarked(content, tokens[block.id])}
        </p>
      );
    }

    if (block.type === "paragraph" || block.type === "quote") {
      return (
        <p className={block.type === "quote" ? "official-paragraph quote" : "official-paragraph"} id={`preview-${block.id}`} key={block.id}>
          {renderMarked(block.content || "（空段落）", tokens[block.id])}
        </p>
      );
    }

    if (block.type === "list") {
      return (
        <ol className={block.style === "unordered" ? "official-list unordered" : "official-list"} key={block.id}>
          {block.items.map((item) => (
            <li key={item.id}>{renderMarked(item.content, tokens[block.id])}</li>
          ))}
        </ol>
      );
    }

    return (
      <table className="official-table" key={block.id}>
        <tbody>
          {block.rows.map((row) => (
            <tr key={row.id}>
              {row.cells.map((cell) => (
                <td key={cell.id}>{renderMarked(cell.content, tokens[block.id])}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    );
  });
}

export function DocumentPreview() {
  const document = useWorkbenchStore((state) => state.document);
  const showGrid = useWorkbenchStore((state) => state.showGrid);
  const unverifiedTokens = useWorkbenchStore((state) => state.unverifiedTokens);
  const header = document.header;
  const body = document.body;
  const footer = document.footer;

  return (
    <div className="preview-scroll">
      <article className={showGrid ? "official-sheet show-grid" : "official-sheet"} aria-label="A4 公文版式预览">
        <div className="official-banxin">
          {(header?.issuingAuthorityMark || header?.documentNumber || header?.signer) && (
            <header className="official-header">
              <div className="authority-mark">{header.issuingAuthorityMark || "发文机关文件"}</div>
              <div className="header-meta">
                <span>{header.documentNumber ? renderMarked(header.documentNumber, unverifiedTokens[header.id]) : "机关代字〔年份〕序号"}</span>
                {header.signer && <span>签发人：{header.signer}</span>}
              </div>
              <div className="red-rule" />
            </header>
          )}

          <h1 className="official-title">{renderMarked(body.title.text || "未命名公文", unverifiedTokens[body.title.id])}</h1>
          {body.mainRecipients.length > 0 && <p className="official-recipient">{body.mainRecipients.join("、")}：</p>}
          <div className="official-content">{renderBlocks(body.blocks, unverifiedTokens)}</div>

          {(body.signer || body.date) && (
            <div className="official-signoff">
              {body.signer && <div>{body.signer.authority}</div>}
              {body.date && <div>{renderMarked(body.date.value, unverifiedTokens[body.date.id])}</div>}
            </div>
          )}

          {body.attachments.length > 0 && (
            <div className="official-attachments">
              <span>附件：</span>
              {body.attachments.map((attachment) => (
                <div key={attachment.id}>
                  {attachment.order}. {attachment.title}
                </div>
              ))}
            </div>
          )}
        </div>

        {footer && (
          <footer className="official-footer">
            <div className="footer-row">抄送：{footer.copyRecipients.join("，")}。</div>
            <div className="footer-row footer-split">
              <span>{footer.printingAuthority}</span>
              <span>{footer.printingDate ? `${footer.printingDate}印发` : ""}</span>
            </div>
          </footer>
        )}
        <span className="page-number">— 1 —</span>
      </article>
    </div>
  );
}
