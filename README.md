# 秉笔 · my-bingbi

> 中文公文写作工作台。跨平台桌面应用，对接私有 Obsidian 知识库，可插拔任意大模型。

把**规范**（GB/T 9704-2012 公文格式国标）、**语料**（你自己积累的历史公文库）、**模型**（可自由更换的大模型）三者打通，产出格式合规、事实可溯、少 AI 味、可直接交付的公文。

---

## 它解决什么问题

让通用大模型写公文，通常得到这样的东西：

> 为深入贯彻落实高质量发展要求，进一步凝心聚力、开拓创新、真抓实干，我单位持续强化责任担当，不断完善工作机制，全面推动各项工作走深走实、提质增效。

看着很正式，但谁负责、做了什么、通过什么机制、到什么节点、有什么结果——一个都没有。而且它随时可能编造一个文号、一笔金额、一份政策依据，这在公文里是事故级错误。

秉笔针对这两件事：

| | 做法 |
| --- | --- |
| **格式对不对** | 规则引擎按国标硬校验（字号、层次序数、六角括号〔〕、日期格式），毫秒级、零成本、结果可复现，不靠模型自觉 |
| **数据是不是编的** | 起草前先把材料结构化成事实台账，生成后逐项回查正文里的数字/日期/文号，**未命中的标红并阻断导出** |
| **像不像人写的** | 降 AI 味与 humanizer 二次审稿：意义膨胀、假深刻、模糊主体、规则三连、万能结论，能机检的走词表与正则，其余交模型评审 |
| **像不像本单位写的** | 从你的 Obsidian 库检索同文种历史件作参考，学本单位既有笔法，而非通用官腔 |

---

## 特性

- **跨平台** — Windows / Linux，Tauri 2 构建，安装包体积小、内存占用低
- **对接 Obsidian** — 主通道走 [Fast Note Sync Service](https://github.com/haierkeys/fast-note-sync-service) 的 REST API + WebSocket，复用其 SQLite FTS5 全文检索与双链接口；也支持直读 vault 目录离线兜底
- **模型可插拔** — OpenAI 兼容（DeepSeek / 通义 / Kimi / 智谱 / vLLM 自建）、Anthropic、Ollama 本地模型。可按流水线阶段分配不同模型：便宜模型做提取，强模型做起草
- **涉密可控** — 密级达到秘密及以上时强制走本地模型，密钥存系统 Keyring，全本地存储，无遥测
- **知识包机制** — 公文规范、文种模板、校验规则都是可装卸的 Skill Pack，可自建"本单位公文包"
- **严格排版** — 按 GB/T 9704-2012 导出 docx：版心 156×225mm、天头 37mm、订口 28mm、行距 28pt 固定值、每面 22 行 × 28 字

---

## 文档

| 文档 | 内容 |
| --- | --- |
| [`docs/PLAN.md`](docs/PLAN.md) | 项目计划：架构、模块拆解、数据模型、里程碑、风险 |
| [`docs/ui/workbench.html`](docs/ui/workbench.html) | 界面设计稿（浏览器打开） |
| [`src/styles/tokens.css`](src/styles/tokens.css) | 设计令牌与 GB/T 9704-2012 版面常量 |

---

## 架构一览

```
前端  React + TypeScript          编辑器 / 版式预览 / 质检面板
  │   IPC
内核  Rust · Cargo Workspace
  ├─ odoc-model    公文文档模型（ODoc AST，全系统唯一真相源）
  ├─ odoc-pack     知识包加载
  ├─ odoc-llm      多模型适配（OpenAI 兼容 / Anthropic / Ollama）
  ├─ odoc-kb       知识库（FNS / 本地 vault / MCP）
  ├─ odoc-lint     规则引擎（确定性校验）
  ├─ odoc-export   docx / pdf 排版导出
  ├─ odoc-engine   流水线编排 + 事实台账
  └─ odoc-store    SQLite + Keyring
```

起草流水线共九步，每步可单独重跑：

```
文种路由 → 事实提取 → 知识库检索 → 骨架生成 → 逐节起草
        → 降 AI 味 → 规则校验 → 事实回查 → 质量评分
```

---

## 状态

**规划阶段。** 尚无可运行代码，当前仓库只有计划与设计。开发路线见 [`docs/PLAN.md` §9](docs/PLAN.md)。

---

## 致谢

公文知识底座参考了两个优秀的开源 Skill 项目，二者互补——前者偏格式规范，后者偏语言质量：

- [KaguraNanaga/official-document-writing-skill](https://github.com/KaguraNanaga/official-document-writing-skill)（MIT）— GB/T 9704-2012 格式详解、文种模板、质量检查清单
- [Liuxiangjian-ai/official-document-skill](https://github.com/Liuxiangjian-ai/official-document-skill)（MIT）— 文种决策、人民日报表达规律、降 AI 味与 humanizer 审稿

本项目的工作是把其中**可机器判定**的部分从提示词转化为配置常量与校验规则，其余保留为按需检索的知识包。转换对照见 [`docs/PLAN.md` 附录](docs/PLAN.md)。
