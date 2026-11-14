# 文档模型与生成流水线

[返回文档入口](index.md)

## 分层

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
  ├─ odoc-store    SQLite + Keyring
  └─ src-tauri     Tauri 壳：commands/{document,export,config,kb,model,engine,lint,update}
```

起草流水线共十步，内核记录每阶段的输入快照，可单阶段重跑：

```
文种路由 → 事实提取 → 知识库检索 → 骨架生成 → 逐节起草
        → 降 AI 味 → 规则校验 → 文本校对 → 事实回查 → 质量评分
```

仿写流水线在检索与骨架之间多一步笔法分析：指定一篇范文，提炼其结构、句式、用词与惯用表述，再按新材料起草。

审核流水线共五步，只出意见、不改原文：

```
文种复核 → 规则校验 → 事实回查 → 语义评审 → 质量评分
```

## 本地状态与网络边界

ODoc 是正文、元数据、事实和引用的统一结构，公共 TypeScript 声明由 Rust 生成。SQLite 存文档、版本、会话和非敏感配置；Keyring 保存模型及知识库凭据。配置目录沿用 my-wordediter 名称，不要因项目显示名变化而手动改路径。

秘密及以上任务拒绝远程模型，上传材料和知识库引用的密级并入任务检查。这个软件边界不能代替组织的保密管理要求或系统环境验证。

浏览器开发模式使用本地工厂和 mock IPC，不读取桌面知识包或发起真实模型调用；正式操作在 Tauri 桌面内核执行。
