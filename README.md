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
| **别人写的这份行不行** | 审核模式：打开或导入已有公文，规则硬校验 + 模型语义评审 + 评分量表，出一份可逐条定位的体检报告，**全程不改动原文** |

---

## 特性

- **跨平台** — Windows / Linux，Tauri 2 构建，安装包体积小、内存占用低
- **对接 Obsidian** — 主通道走 [Fast Note Sync Service](https://github.com/haierkeys/fast-note-sync-service) REST API，复用其 SQLite FTS5 全文检索与双链接口；也支持直读 vault 目录离线兜底，WebSocket 增量订阅待补
- **模型可插拔** — OpenAI 兼容（DeepSeek / 通义 / Kimi / 智谱 / vLLM 自建）、Anthropic、Ollama 本地模型。可按流水线阶段分配不同模型：便宜模型做提取，强模型做起草
- **涉密可控** — 密级达到秘密及以上时强制走本地模型，密钥存系统 Keyring，全本地存储，无遥测
- **知识包机制** — 公文规范、文种模板、校验规则都是可装卸的 Skill Pack，可自建"本单位公文包"
- **严格排版** — 按 GB/T 9704-2012 导出 docx：版心 156×225mm、天头 37mm、订口 28mm、行距 28pt 固定值、每面 22 行 × 28 字

---

## 文档

| 文档 | 内容 |
| --- | --- |
| [`docs/PLAN.md`](docs/PLAN.md) | 项目计划：架构、模块拆解、数据模型、里程碑、风险 |
| [`docs/ODOC-SPEC.md`](docs/ODOC-SPEC.md) | ODoc 0.1 文档模型与兼容性规范 |
| [`docs/PACK-SPEC.md`](docs/PACK-SPEC.md) | 声明式知识包清单、规则与安全边界 |
| [`docs/STORE-SPEC.md`](docs/STORE-SPEC.md) | SQLite 事务、版本历史、配置和系统 Keyring 边界 |
| [`docs/ui/workbench.html`](docs/ui/workbench.html) | 界面设计稿（浏览器打开） |
| [`src/styles/tokens.css`](src/styles/tokens.css) | 设计令牌与 GB/T 9704-2012 版面常量 |

---

## 本地开发

要求 Node.js 20.19+（或 22.12+）、npm 10+、Rust 1.85+。先运行浏览器版工作台：

```bash
npm install
npm run dev
```

打开 `http://127.0.0.1:1420`。浏览器模式会使用与 Rust IPC 同结构的本地 ODoc 工厂，适合前端开发；草稿保存在浏览器本地存储。

运行测试和生产构建：

```bash
npm test
npm run build
cd src-tauri
cargo test -p odoc-model
cargo test -p odoc-store
cargo test -p odoc-llm
cargo test -p odoc-pack
cargo test -p odoc-lint
cargo test -p odoc-export
cargo test -p odoc-kb
cargo test -p odoc-engine
cargo clippy -p odoc-lint -p odoc-export -p odoc-kb -p odoc-engine -p odoc-store --all-targets -- -D warnings
```

桌面版还需要 Tauri 2 的平台依赖。Ubuntu/Debian 安装 `libwebkit2gtk-4.1-dev`、`libayatana-appindicator3-dev`、`librsvg2-dev`、`libxdo-dev`、`libssl-dev`、`patchelf` 和构建工具后运行：

```bash
npm run tauri dev
```

修改 Rust 公共类型后重新生成前端声明：

```bash
cd src-tauri
cargo run -p odoc-model --example export_odoc_bindings
cargo run -p odoc-store --example export_store_bindings
cargo run -p odoc-llm --example export_llm_bindings
cargo run -p odoc-pack --example export_pack_bindings
cargo run -p odoc-lint --example export_lint_bindings
cargo run -p odoc-kb --example export_kb_bindings
cargo run -p odoc-engine --example export_engine_bindings
```

知识包和 `pipelines/draft.yaml` 会在桌面应用启动时从开发目录或安装包资源目录加载并校验。浏览器预览不读取本地知识包，也不会保存包内凭据；浏览器模式也不会发起模型调用。

---

## GitHub 发布

推送版本 tag 会自动在 GitHub Actions 的原生 runner 上构建 Linux `.deb` 和 Windows NSIS `.exe`，并创建对应的 GitHub Release：

```bash
git tag v0.1.0
git push origin v0.1.0
```

发布工作流位于 [`.github/workflows/release.yml`](.github/workflows/release.yml)，只匹配 `v*` tag；构建完成前不会创建 Release。

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

审核流水线共五步，只出意见、不改原文：

```
文种复核 → 规则校验 → 事实回查 → 语义评审 → 质量评分
```

---

## 状态

**M2 知识库接入进行中。** 当前已有可运行的 React 工作台、A4 国标版心预览、结构化正文编辑、首批确定性质检、事实台账、最近文档、自动保存、版本快照与版本 diff；桌面版可在内核完成 lint/factcheck 后导出简化 DOCX，浏览器版保留 ODoc JSON 导出。桌面版另有**审核模式**：对已打开或导入的公文跑一遍只读的五步体检流水线，产出可逐条定位的报告。Rust 侧已建立 Tauri 壳、`odoc-model` AST、`odoc-store` SQLite 持久化、`odoc-llm` 的 OpenAI 兼容/Ollama 普通及流式调用，以及 FNS/本地 Vault 知识库配置、Keyring token、素材检索和起草阶段有界召回。模型凭据只存系统 Keyring，秘密及以上公文在内核层禁止远程调用；FNS 远程检索同样受密级边界保护。Linux `.deb` 已在本机生成并校验，CI 已加入原生 Linux/Windows 壳检查与安装包产物。

首次 CI 运行完成 Windows NSIS 产物验证后，M0 即可闭环；M1 的知识包、流水线、规则引擎和导出按 [`docs/PLAN.md` §9](docs/PLAN.md) 继续实现。

---

## 致谢

公文知识底座参考了两个优秀的开源 Skill 项目，二者互补——前者偏格式规范，后者偏语言质量：

- [KaguraNanaga/official-document-writing-skill](https://github.com/KaguraNanaga/official-document-writing-skill)（MIT）— GB/T 9704-2012 格式详解、文种模板、质量检查清单
- [Liuxiangjian-ai/official-document-skill](https://github.com/Liuxiangjian-ai/official-document-skill)（MIT）— 文种决策、人民日报表达规律、降 AI 味与 humanizer 审稿

本项目的工作是把其中**可机器判定**的部分从提示词转化为配置常量与校验规则，其余保留为按需检索的知识包。转换对照见 [`docs/PLAN.md` 附录](docs/PLAN.md)。
