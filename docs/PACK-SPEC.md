# 秉笔知识包规范 0.1

知识包（Skill Pack）是可安装、可启停、可回归测试的公文能力单元。它承载文种模板、确定性规则、版式参数和供模型按需检索的参考材料，不包含可执行本机代码。

## 1. 目录约定

```text
packs/<pack-id>/
├── pack.toml
├── SKILL.md
├── references/
├── checklists/
├── templates/
├── rules/
├── style/
└── tests/
```

只有 `pack.toml` 必填。清单中的其余目录按能力声明创建；包加载器必须忽略未声明的普通文件和未知扩展字段。

## 2. `pack.toml`

```toml
schema_version = "0.1"

[pack]
id = "gb9704-format"
name = "GB/T 9704-2012 格式规范包"
version = "0.1.0"
description = "公文版式参数与首批确定性校验规则"
license = "MIT"
source = "https://example.invalid/source"

[capabilities]
doc_types = ["request", "notice", "report", "letter"]
provides = ["format_rules", "style"]

[[bindings]]
stage = "lint"
rules = ["rules/format.yaml"]

[[bindings]]
stage = "export"
styles = ["style/gb9704.toml"]
```

约束：

- `pack.id` 必须与目录名相同，格式为小写 ASCII、数字和连字符。
- `pack.version` 使用 SemVer。
- `schema_version` 表示知识包清单格式版本，不等同于包版本。
- 所有资源路径必须是包目录内的相对路径；禁止绝对路径和 `..` 路径穿越。
- `provides` 可用值首批限定为 `format_rules`、`language_rules`、`templates`、`references`、`checklists`、`style`、`tests`。

## 3. Stage 绑定

首批标准 stage ID：`doctype`、`facts`、`retrieve`、`outline`、`draft`、`deai`、`lint`、`factcheck`、`review`、`score`、`export`。

stage ID 的声明顺序即规范执行顺序，流水线必须按此顺序列出各阶段。其中 `review` 只在审核流水线中执行，`facts`、`outline`、`draft`、`deai` 只在起草流水线中执行。

绑定项可包含：

- `inject`：注入模型上下文的 Markdown 或模板资源。
- `rules`：交给规则引擎的 YAML 文件。
- `styles`：交给预览或导出器的 TOML 文件。
- `retrieval`：资源是否采用片段检索，而不是整文件注入。

占位符只允许 `{doc_type}`。替换后仍须通过包目录边界检查。

## 4. 规则文件

规则文件是 YAML 数组，每项至少包含：

```yaml
- id: wenhao-bracket
  name: 发文字号须用六角括号
  severity: error
  type: regex
  scope: header.documentNumber
  pattern: '^.+〔\\d{4}〕\\d+号$'
  match: required
  message: 发文字号应使用六角括号〔〕并包含完整年份和序号
  suggestion: 例如：云应急〔2026〕27号
```

首批规则类型：`regex`、`lexicon_density`、`structure`、`format`、`fact_check`。`severity` 为 `error`、`warn` 或 `info`，其中 `error` 默认阻断导出。

规则 ID 在单个包内必须唯一。载入多个包后，以 `<pack-id>/<rule-id>` 作为全局 ID，避免命名冲突。

## 5. 文种模板

模板文件以 ODoc 的 `Body` 子集表达骨架，不直接存最终正文。同一文种可以有多份场景模板（如“会议通知”“工作部署通知”），以文件名为模板 ID：

```yaml
id: notice-meeting          # 可选，默认取文件名
name: 会议通知              # 选择器显示名
doc_type: notice
scenario: 召开会议，明确时间地点、参会人员与议题
description: 适用于部署会、推进会等会议通知
required:
  main_recipients: true
  closing: 特此通知。
blocks:
  - type: paragraph
    role: purpose
    guidance: 交代会议背景、依据和目的
  - type: heading
    level: l1
    title: 会议安排
  - type: paragraph
    role: closing
    content: 特此通知。
```

- `required.closing` 写入正文末段，不占用附注字段。
- `blocks[].content` 可放固定称谓或固定结语；没有 `content` 的段落由模型起草。
- `role` 是流水线提示和模型输出校验使用的语义标签，不进入最终 ODoc。
- `templates/{doc_type}.yaml` 是该文种的默认模板；其他场景模板必须在 `outline` 绑定的 `inject` 中显式声明。
- 起草请求可携带 `templateId`；内核只加载该模板，未指定时回退到默认模板。模板的 `doc_type` 必须与最终确认文种一致。

## 6. 版式文件

长度统一使用毫米，字号和行距使用 pt，颜色使用 `#RRGGBB`。`gb9704.toml` 中的页面参数必须与 `src/styles/tokens.css` 的 `--gb-*` 常量一致，并由测试比对。

## 7. 安全边界

- M0-M3 知识包只允许声明式 Markdown、YAML、TOML 和静态模板。
- 包内符号链接默认拒绝加载。
- 单文件和整包大小必须设置上限；大参考资料只能片段检索。
- `SKILL.md` 与参考资料是不可信提示内容，不得覆盖密级拦截、事实回查和导出阻断等内核策略。
- 将来增加 WASM 扩展时必须使用独立 capability 和沙箱，不与本规范的声明式包混用。

## 8. 版本与测试

包更新必须运行 `tests/*.yaml` 中的回归用例。规则可机械判断的条件由测试 runner 直接断言；语义条件可配置 LLM judge，但其结果不能绕过确定性 error。

不兼容的清单或资源语义变化提升 `schema_version`。普通内容、模板和规则迭代只提升 `pack.version`。
