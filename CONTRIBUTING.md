# 贡献指南

感谢你对「秉笔 · my-bingbi」的关注!这是一个本地优先的中文公文写作工作台,欢迎任何形式的贡献。

## 报告问题

- 提交 Bug 报告或功能建议前,先搜索 [Issues](https://github.com/hellomrli/my-bingbi/issues),确认是否已有相同内容;
- Bug 报告请尽量包含:操作系统、应用版本、复现步骤、期望行为与实际行为;
- 涉及崩溃的问题,可在应用「设置」页查看本地崩溃诊断记录,附上相关条目会有帮助。

## 提交代码

1. Fork 仓库并创建特性分支;
2. 遵循现有代码风格,提交前确保通过:
   ```bash
   npm run check   # lint + typecheck + 单元测试
   npm run build
   cd src-tauri
   cargo test --workspace --exclude my-bingbi
   cargo clippy --workspace --exclude my-bingbi --all-targets -- -D warnings
   cargo fmt --all -- --check
   ```
   壳 crate `my-bingbi` 需要 Tauri 2 的桌面系统依赖(见 README「本地开发」),本地可排除,CI 会做完整编译检查;
3. 修改 Rust 公共类型后,重新生成前端声明(见 README「本地开发」的绑定生成命令);
4. 提交信息采用 [Conventional Commits](https://www.conventionalcommits.org/) 风格(`feat` / `fix` / `docs` / `style` / `chore` 等);
5. 发起 Pull Request,说明改动动机与影响范围。

## 代码结构

见 README「架构一览」。核心约定:

- 文档模型以 `odoc-model` 的 ODoc AST 为唯一真相源;
- 导出排版集中在 `odoc-export`:DOCX 走 Word 模板,PDF 走内置 Typst 引擎(零外部依赖);
- 模型凭据只存系统 Keyring,不得写入配置文件、日志或提交内容。

## 行为准则

保持专业与尊重。请勿提交含密钥、个人信息或受版权保护的公文语料。

## 许可

参与贡献即表示你同意将你的贡献按项目的 [MIT License](LICENSE) 授权。
