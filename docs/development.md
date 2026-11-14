# 本地开发、检查和绑定生成

[返回文档入口](index.md)

## 准备环境

从仓库根目录执行下列命令；先克隆 `https://github.com/hellomrli/my-bingbi.git` 并进入目录。

要求 Node.js 20.19+（或 22.12+）、npm 10+、Rust 1.85+。先运行浏览器版工作台：

```bash
npm ci
npm run dev
```

打开 `http://127.0.0.1:1420`。浏览器模式会使用与 Rust IPC 同结构的本地 ODoc 工厂，适合前端开发；草稿保存在浏览器本地存储。

运行测试和生产构建：

```bash
npm run check             # lint + typecheck + 单元测试（提交前跑这一条即可）
npm run lint              # ESLint（TypeScript + React Hooks 规则）
npm test                  # 前端单元测试（vitest + testing-library）
npm run test:e2e          # Playwright 端到端（浏览器模式）
npm run build             # tsc -b + vite build（同时做类型检查）
(cd src-tauri
cargo test --workspace --exclude my-bingbi   # 除 Tauri 壳外的全部 crate（壳需桌面系统依赖，见下）
cargo clippy --workspace --exclude my-bingbi --all-targets -- -D warnings
cargo fmt --all -- --check)
```

桌面版还需要 Tauri 2 的平台依赖。Ubuntu/Debian 安装 `libwebkit2gtk-4.1-dev`、`libayatana-appindicator3-dev`、`librsvg2-dev`、`libxdo-dev`、`libssl-dev`、`patchelf` 和构建工具后运行：

```bash
npm run tauri dev
```

修改 Rust 公共类型后重新生成前端声明：

```bash
(cd src-tauri
cargo run -p odoc-model --example export_odoc_bindings
cargo run -p odoc-store --example export_store_bindings
cargo run -p odoc-llm --example export_llm_bindings
cargo run -p odoc-pack --example export_pack_bindings
cargo run -p odoc-lint --example export_lint_bindings
cargo run -p odoc-kb --example export_kb_bindings
cargo run -p odoc-engine --example export_engine_bindings)
```

知识包和 `pipelines/draft.yaml` 会在桌面应用启动时从开发目录或安装包资源目录加载并校验。浏览器预览不读取本地知识包，也不会保存包内凭据；浏览器模式也不会发起模型调用。

## 构建安装包和发布

运行桌面构建前按 [CI](../.github/workflows/ci.yml) 安装平台依赖。维护者发布时同步 package.json、Tauri 配置和 Cargo.toml 的版本，检查通过后推送尚未使用的 v 标签。

[release.yml](../.github/workflows/release.yml) 在平台 runner 上生成 Windows NSIS、Linux Debian/AppImage、签名与 latest.json。签名私钥通过既有发布 Secrets 配置，不能提交到源码。发布内容与版本以 Releases 实际资产为准。
