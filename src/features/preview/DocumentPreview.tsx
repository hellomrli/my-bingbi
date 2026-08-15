import type { Block, HeadingLevel } from "../../lib/odoc";
import { useWorkbenchStore } from "../../store/workbench";

const chineseDigits = ["零", "一", "二", "三", "四", "五", "六", "七", "八", "九", "十"];

function chineseNumber(value: number): string {
  if (value <= 10) return chineseDigits[value];
  if (value < 20) return `十${chineseDigits[value - 10]}`;
  const tens = Math.floor(value / 10);
  const ones = value % 10;
  return `${chineseDigits[tens]}十${ones ? chineseDigits[ones] : ""}`;
}

function headingPrefix(level: HeadingLevel, number: number): string {
  if (level === "l1") return `${chineseNumber(number)}、`;
  if (level === "l2") return `（${chineseNumber(number)}）`;
  if (level === "l3") return `${number}. `;
  return `（${number}）`;
}

function renderBlocks(blocks: Block[]) {
  const counters: Record<HeadingLevel, number> = { l1: 0, l2: 0, l3: 0, l4: 0 };

  return blocks.map((block) => {
    if (block.type === "heading") {
      counters[block.level] += 1;
      if (block.level === "l1") counters.l2 = counters.l3 = counters.l4 = 0;
      if (block.level === "l2") counters.l3 = counters.l4 = 0;
      if (block.level === "l3") counters.l4 = 0;
      return (
        <p className={`official-heading ${block.level}`} id={`preview-${block.id}`} key={block.id}>
          {headingPrefix(block.level, counters[block.level])}
          {block.content || "未填写标题"}
        </p>
      );
    }

    if (block.type === "paragraph" || block.type === "quote") {
      return (
        <p className={block.type === "quote" ? "official-paragraph quote" : "official-paragraph"} id={`preview-${block.id}`} key={block.id}>
          {block.content || "（空段落）"}
        </p>
      );
    }

    if (block.type === "list") {
      return (
        <ol className={block.style === "unordered" ? "official-list unordered" : "official-list"} key={block.id}>
          {block.items.map((item) => (
            <li key={item.id}>{item.content}</li>
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
                <td key={cell.id}>{cell.content}</td>
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
                <span>{header.documentNumber || "机关代字〔年份〕序号"}</span>
                {header.signer && <span>签发人：{header.signer}</span>}
              </div>
              <div className="red-rule" />
            </header>
          )}

          <h1 className="official-title">{body.title.text || "未命名公文"}</h1>
          {body.mainRecipients.length > 0 && <p className="official-recipient">{body.mainRecipients.join("、")}：</p>}
          <div className="official-content">{renderBlocks(body.blocks)}</div>

          {(body.signer || body.date) && (
            <div className="official-signoff">
              {body.signer && <div>{body.signer.authority}</div>}
              {body.date && <div>{body.date.value}</div>}
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
