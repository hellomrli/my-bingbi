# ODoc 0.1 规范

ODoc（Official Document Model）是秉笔内部唯一的公文数据模型。编辑器、质检、流水线、知识库引用和导出器都读写同一棵 JSON AST；Markdown、纯文本和 docx 只是导入导出格式。

本规范对应 Rust crate `src-tauri/crates/odoc-model`，当前 schema 版本为 `0.1.0`。

## 1. 设计约束

1. 每个可定位节点必须有稳定、非空的 `id`。质检项和来源引用只通过节点 ID 关联，不依赖数组下标。
2. JSON 字段统一使用 `camelCase`，枚举值统一使用 `snake_case`。
3. `schemaVersion` 仅在发生不兼容结构变化时升级；内容编辑只增加 `revision`。
4. 缺省要素使用 `null` 或空数组，不使用含义不明的空对象。
5. 金额、日期、文号等事实保留原始文本，不在 AST 层做有损归一化。标准化值由事实提取或校验层附加。
6. 密级为 `secret` 或 `confidential` 时，内核必须拒绝远程模型 Provider。

## 2. 顶层结构

```jsonc
{
  "schemaVersion": "0.1.0",
  "id": "doc-...",
  "revision": 0,
  "meta": {},
  "header": {},
  "body": {},
  "footer": null,
  "facts": { "facts": [] },
  "provenance": []
}
```

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `schemaVersion` | string | ODoc schema 版本 |
| `id` | string | 文档稳定 ID |
| `revision` | uint32 | 每次内容变更递增，便于乐观并发与版本记录 |
| `meta` | `DocMeta` | 文种、密级、紧急程度和状态 |
| `header` | `Header \| null` | 版头 |
| `body` | `Body` | 主体，必填 |
| `footer` | `Footer \| null` | 版记 |
| `facts` | `FactLedger` | 事实白名单 |
| `provenance` | `Citation[]` | 节点来源记录 |

## 3. 元数据与版头

`DocumentType` 当前支持：`request`、`notice`、`report`、`letter`、`minutes`、`summary`、`decision`、`announcement`、`other`。扩充文种只追加枚举值，不改变已有含义。

`SecurityLevel`：`public`、`internal`、`secret`、`confidential`。

`Header` 包含：

- `issuingAuthorityMark`：发文机关标志，例如“×××县×××局文件”。
- `documentNumber`：完整发文字号，例如“×××〔2026〕27号”。
- `signer`：上行文签发人姓名。

## 4. 主体与正文块

`Body` 固定包含标题、主送机关、正文块、附件说明、署名、成文日期和附注。

正文块 `Block` 使用带 `type` 的判别联合：

| `type` | 必要字段 | 用途 |
| --- | --- | --- |
| `heading` | `id`, `level`, `content` | 四级层次标题；`content` 不含自动序号 |
| `paragraph` | `id`, `content` | 普通正文段落 |
| `list` | `id`, `style`, `items` | 有序或无序列表 |
| `table` | `id`, `rows` | 表格 |
| `quote` | `id`, `content` | 引文或原文摘录 |

标题层级到国标样式的固定映射：

| 层级 | 自动序号 | 字体 | 字号 |
| --- | --- | --- | --- |
| `l1` | 一、 | 黑体 | 3 号 / 16pt |
| `l2` | （一） | 楷体_GB2312 | 3 号 / 16pt |
| `l3` | 1. | 仿宋_GB2312 | 3 号 / 16pt |
| `l4` | （1） | 仿宋_GB2312 | 3 号 / 16pt |

序号由渲染器和导出器按文档顺序计算，不存入 `content`，以避免移动节点后序号失真。

## 5. 事实台账

每条 `Fact` 至少包含：

- `kind`：主体、客体、动作、数据、时间、地点、问题、依据、责任、反馈或其他。
- `value` / `unit`：原始事实值和可选单位。
- `source`：`user_input`、`kb_retrieved` 或 `model_generated`。
- `sourceRef`：知识库路径、输入材料 ID 或其他可追溯引用。
- `verified`：是否经过用户或可信来源确认。

`model_generated` 默认必须为 `verified: false`。事实回查产生的 error 在用户确认、补录来源或删除正文表述前不得解除。

## 6. 来源引用

`Citation.nodeId` 指向标题、正文块或其他稳定节点。`source` 支持 `user`、`knowledge_base`、`model`、`imported`；`sourceRef` 保存可回查标识，`excerpt` 可保存最小必要摘录。

## 7. 类型生成

Rust 是模型定义源。更新模型后运行：

```bash
cd src-tauri
cargo run -p odoc-model --example export_odoc_bindings
```

生成文件为 `src/lib/generated/bindings.ts`，前端只从 `src/lib/odoc.ts` 导入公共类型。

## 8. 兼容性

- 读取方必须忽略未知字段。
- 同一主版本内新增可选字段和枚举值属于兼容变更。
- 删除字段、修改字段类型、修改已有枚举语义属于不兼容变更，必须升级 `schemaVersion` 并提供迁移函数。
- JSON Patch 的路径以本规范字段名为准；涉及数组节点时应同时携带节点 ID，内核不得只依赖易漂移的数组下标。
