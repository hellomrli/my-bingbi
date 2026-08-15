# 本地存储规范 0.1

`odoc-store` 负责秉笔的本地持久化和密钥抽象。SQLite 保存文档、版本、流水线状态和非敏感配置；模型 API Key、FNS token 等秘密只进入操作系统凭据库。

## 1. 路径

默认数据库文件名为 `bingbi.sqlite3`，位于 `directories` crate 解析出的应用配置目录：

- Windows：用户 Roaming AppData 下的 `Bingbi/my-wordediter/`
- Linux：`$XDG_CONFIG_HOME/my-wordediter/`，未设置时为 `~/.config/my-wordediter/`

测试和嵌入场景可以显式传入数据库路径或使用内存数据库。代码只使用 `Path` / `PathBuf`，不得拼接平台分隔符。

## 2. SQLite 策略

- 启用 `foreign_keys`。
- 文件数据库使用 WAL 日志与 `NORMAL` 同步级别。
- 设置 5 秒 busy timeout，避免短暂并发保存直接失败。
- schema 版本使用 `PRAGMA user_version`。程序拒绝打开高于自身支持版本的数据库。
- SQLite 以 bundled 方式编译，不依赖目标机器的系统 SQLite 版本。

首版 schema 包含计划中约定的 `documents`、`doc_versions`、`sessions`、`stage_runs`、`kb_notes`、`kb_chunks`、`kb_fts`、`findings`、`providers`，并增加 `app_settings`。

## 3. 文档保存事务

一次 `save_document` 在同一事务内完成：

1. 将 ODoc 序列化为 JSON。
2. 插入或更新 `documents` 当前快照及可检索元数据。
3. 当前 JSON 与上次保存内容不同时，向 `doc_versions` 追加连续序号的不可变快照。
4. 提交事务后返回版本号、是否新文档、是否产生新版本和保存时间。

相同内容的重复保存会刷新 `updated_at`，但不会制造重复版本。版本序号由存储层生成，不直接使用 ODoc `revision`；前者表示持久化历史，后者表示编辑器内修改次数。

若提交的 ODoc `revision` 小于当前已保存修订且正文不同，存储层返回 `stale_revision`，不得让迟到的自动保存覆盖较新内容。相同内容的迟到请求仍按幂等保存处理。

删除文档会通过外键级联删除版本、会话和质检项。恢复旧版本必须另存为一次新版本，不允许覆写已有版本记录。

## 4. 非敏感配置

`app_settings` 以稳定 key 和 JSON value 保存。key 只能包含 ASCII 字母、数字、点、下划线和连字符，最长 128 字节。

首批 `AppConfig` 字段：

- `schemaVersion`：配置结构版本。
- `theme`：`system`、`light` 或 `dark`。
- `autosaveEnabled`：是否启用自动保存。
- `fontMappings`：标准公文字体到本机字体的映射。

缺少配置时返回默认值，不把“首次运行”当作错误。

## 5. 密钥边界

`SecretStore` trait 只定义 `set_secret`、`get_secret`、`has_secret` 和 `delete_secret`。生产实现使用：

- Windows Credential Manager
- Linux Secret Service
- macOS Keychain（便于开发和未来兼容）

所有系统凭据操作通过互斥锁串行化。测试使用内存替身，不读写开发者机器的真实凭据库。

Tauri IPC 不暴露 `get_secret`：前端只能设置、检查是否存在或删除指定 Provider 的密钥。模型 Provider 在 Rust 内核内部读取密钥并发起请求。

## 6. IPC 错误

存储错误通过稳定错误码返回前端，显示文案与底层错误分离：

| 错误码 | 含义 |
| --- | --- |
| `store_unavailable` | 数据库无法打开或锁定 |
| `document_not_found` | 文档或版本不存在 |
| `invalid_input` | ID、配置 key 或文档数据不合法 |
| `serialization_failed` | ODoc 或配置 JSON 处理失败 |
| `stale_revision` | 迟到的保存请求试图覆盖较新修订 |
| `secret_store_unavailable` | 系统凭据库不可访问或被锁定 |

错误响应和日志不得包含 API Key、token 或完整涉密正文。
