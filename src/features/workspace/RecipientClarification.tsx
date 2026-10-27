import { LoaderCircle, MessageCircle } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { chatWithProvider, ipcErrorMessage } from "../../lib/ipc";
import type { ODoc } from "../../lib/odoc";
import { parseRecipientInference, RECIPIENT_CLARIFICATION_PROMPT } from "../../lib/recipient";
import { useWorkbenchStore } from "../../store/workbench";

interface RecipientClarificationProps {
  question: string;
  document: ODoc;
  providerId: string;
  saveDocument: () => Promise<void>;
  onDismiss: () => void;
}

export function RecipientClarification({ question: initialQuestion, document, providerId, saveDocument, onDismiss }: RecipientClarificationProps) {
  const [question, setQuestion] = useState(initialQuestion);
  const [answer, setAnswer] = useState("");
  const [answers, setAnswers] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const requestRef = useRef(0);
  useEffect(() => () => { requestRef.current += 1; }, []);

  async function clarify() {
    if (busy || !answer.trim()) return;
    const request = ++requestRef.current;
    const snapshot = structuredClone(document);
    const nextAnswers = [...answers, answer.trim()].slice(-4);
    setBusy(true);
    setError(null);
    try {
      await saveDocument();
      if (request !== requestRef.current) return;
      if (JSON.stringify(useWorkbenchStore.getState().document) !== JSON.stringify(snapshot)) {
        throw new Error("文档已变更，请基于最新内容重新确认；正文不会被覆盖。");
      }
      const response = await chatWithProvider(providerId, document.id, {
        messages: [
          { role: "system", content: RECIPIENT_CLARIFICATION_PROMPT },
          { role: "user", content: JSON.stringify({ docType: document.meta.docType, question, answers: nextAnswers }) },
        ],
        temperature: 0,
        maxTokens: 800,
        jsonMode: true,
        securityLevel: document.meta.securityLevel,
      });
      if (request !== requestRef.current) return;
      if (JSON.stringify(useWorkbenchStore.getState().document) !== JSON.stringify(snapshot)) {
        throw new Error("确认期间文档已修改，请重新确认；正文不会被覆盖。");
      }
      const inference = parseRecipientInference(response.content, nextAnswers.join("\n"), document.meta.docType);
      if (inference.recipients.length > 0) {
        useWorkbenchStore.getState().updateRecipients(inference.recipients);
        onDismiss();
      } else {
        setQuestion(inference.question ?? initialQuestion);
        setAnswers(nextAnswers);
        setAnswer("");
      }
    } catch (clarificationError) {
      if (request === requestRef.current) setError(ipcErrorMessage(clarificationError));
    } finally {
      if (request === requestRef.current) setBusy(false);
    }
  }

  return (
    <section className="recipient-clarification" aria-label="补充确认收文对象">
      <div className="recipient-question"><MessageCircle size={16} /><strong>{question}</strong></div>
      <p>正文已生成，主送机关暂留空。可以直接回答，也可以跳过，不影响编辑和导出。</p>
      <form onSubmit={(event) => { event.preventDefault(); void clarify(); }}>
        <input aria-label="补充说明" value={answer} maxLength={1000} disabled={busy}
          onChange={(event) => setAnswer(event.target.value)} placeholder="例如：这份是发给各乡镇政府的" />
        <button className="command-button primary" type="submit" disabled={busy || !answer.trim() || !providerId}>
          {busy && <LoaderCircle size={14} className="is-spinning" />}{busy ? "正在理解" : "回复并更新"}
        </button>
        <button className="command-button" type="button" onClick={() => { requestRef.current += 1; onDismiss(); }}>
          先留空，继续编辑
        </button>
      </form>
      {error && <p className="source-message is-error" role="alert">{error}</p>}
    </section>
  );
}
