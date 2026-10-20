import { Plus, Trash2 } from "lucide-react";
import { useState, type FormEvent } from "react";
import type { FactKind, FactSource } from "../../lib/odoc";
import { useWorkbenchStore } from "../../store/workbench";

const sourceLabels: Record<FactSource, string> = {
  user_input: "用户提供",
  kb_retrieved: "知识库检索",
  model_generated: "模型提取（未经人工核实）",
};

const kindLabels: Record<FactKind, string> = {
  subject: "主体", object: "客体", action: "动作", data: "数据", time: "时间", place: "地点",
  problem: "问题", basis: "依据", responsibility: "责任", feedback: "反馈", other: "其他",
};

export function FactLedger() {
  const facts = useWorkbenchStore((state) => state.document.facts.facts);
  const addFact = useWorkbenchStore((state) => state.addFact);
  const updateFact = useWorkbenchStore((state) => state.updateFact);
  const removeFact = useWorkbenchStore((state) => state.removeFact);
  const [value, setValue] = useState("");
  const [unit, setUnit] = useState("");
  const [kind, setKind] = useState<FactKind>("data");
  const [sourceRef, setSourceRef] = useState("");
  const [verified, setVerified] = useState(false);

  function submit(event: FormEvent) {
    event.preventDefault();
    if (!value.trim()) return;
    addFact({ kind, value: value.trim(), unit, source: "user_input", sourceRef, verified });
    setValue("");
    setUnit("");
    setSourceRef("");
    setVerified(false);
  }

  return (
    <section className="fact-ledger" aria-label="事实台账">
      <h3>事实台账 {facts.length} 项</h3>
      <p>模型提取不等于核实。请对照原始材料逐条确认；修改事实值、单位或依据后须重新确认。导入文档可手动补充缺失的数据和日期。</p>
      <div className="fact-ledger-list">
        {facts.map((fact) => (
          <fieldset className="fact-ledger-item" key={fact.id}>
            <legend>{fact.value}{fact.unit ?? ""} · {fact.verified ? "已核实" : "待核实"}</legend>
            <span className="fact-origin">来源：{sourceLabels[fact.source]}</span>
            <div className="fact-ledger-fields">
              <label>类型<select value={fact.kind} onChange={(event) => updateFact(fact.id, { kind: event.target.value as FactKind })}>
                {Object.entries(kindLabels).map(([key, label]) => <option key={key} value={key}>{label}</option>)}
              </select></label>
              <label>事实值<input value={fact.value} onChange={(event) => updateFact(fact.id, { value: event.target.value })} /></label>
              <label>单位<input value={fact.unit ?? ""} onChange={(event) => updateFact(fact.id, { unit: event.target.value })} /></label>
            </div>
            <label>核实依据 / 来源<input value={fact.sourceRef ?? ""} placeholder="原始材料、文件路径或核实说明" onChange={(event) => updateFact(fact.id, { sourceRef: event.target.value })} /></label>
            <div className="fact-ledger-actions">
              <label className="fact-verify"><input type="checkbox" checked={fact.verified} disabled={!fact.value.trim()} onChange={(event) => updateFact(fact.id, { verified: event.target.checked })} />已对照材料人工核实</label>
              <button className="icon-button small danger-action" type="button" aria-label={`移除事实 ${fact.value}${fact.unit ?? ""}`} onClick={() => removeFact(fact.id)}><Trash2 size={14} /></button>
            </div>
          </fieldset>
        ))}
        {facts.length === 0 && <p>尚无事实。可在这里补充正文中需要核实的数据。</p>}
      </div>
      <form className="fact-ledger-add" onSubmit={submit} aria-label="补充事实">
        <h4>补充事实</h4>
        <div className="fact-ledger-fields">
          <label>类型<select value={kind} onChange={(event) => setKind(event.target.value as FactKind)}>
            {Object.entries(kindLabels).map(([key, label]) => <option key={key} value={key}>{label}</option>)}
          </select></label>
          <label>事实值<input value={value} onChange={(event) => setValue(event.target.value)} /></label>
          <label>单位<input value={unit} onChange={(event) => setUnit(event.target.value)} /></label>
        </div>
        <label>核实依据 / 来源<input value={sourceRef} onChange={(event) => setSourceRef(event.target.value)} /></label>
        <label className="fact-verify"><input type="checkbox" checked={verified} onChange={(event) => setVerified(event.target.checked)} />已对照材料人工核实</label>
        <button className="command-button" type="submit" disabled={!value.trim()}><Plus size={14} />加入事实台账</button>
      </form>
    </section>
  );
}
