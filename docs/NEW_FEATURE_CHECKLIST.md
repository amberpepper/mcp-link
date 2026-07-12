# 新功能检查清单

添加产品功能时按这份清单走，避免改到旧桌面实现路径。

## 共享类型

- 领域类型放在 `packages/shared/src/types/`。
- Platform API 类型放在 `packages/shared/src/types/platform-api/domains/`。
- 新类型需要从 `packages/shared/src/types/index.ts` 和
  `packages/shared/src/types/platform-api/index.ts` 导出。

## Rust 后端

- Rust core、Tauri desktop shell 和 server shell 都在
  `apps/desktop/src-tauri/src/`。
- Platform API 入口在 `apps/desktop/src-tauri/src/platform/mod.rs`。
- Web 浏览器模式需要的 Platform API 由 Rust server 二进制通过 HTTP 提供。
- 本地持久化使用 SQLite，不再新增 JSON 状态文件。
- 密钥、token、bearer token 不要以明文从列表接口返回。
- 行为变化需要补 Rust 单测。

## Web 前端

- 页面组件放在 `apps/web/src/renderer/components/`。
- Runtime 适配放在 `apps/web/src/renderer/platform-api/`。
- 新页面需要更新 `App.tsx` 路由和 `Sidebar.tsx` 导航。
- 文案放在 `apps/web/src/locales/*.json`。

## Server 二进制

- Server 入口是 `mcp-link-server`，不依赖 Tauri 或 WebView。
- Server shell 只承载 HTTP、MCP 暴露和启动配置，业务逻辑应复用 Rust core。
- 浏览器模式调用 `apps/web/src/renderer/platform-api/http-platform-api.ts`。
- Headless/Server 场景的监听地址由 `MCP_LINK_HTTP_ADDR` 配置，默认
  `127.0.0.1:3284`。

## 验证

- `pnpm --filter @mcp_link/shared typecheck`
- `pnpm --filter @mcp_link/web typecheck`
- `pnpm --filter @mcp_link/web build`
- `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --features desktop --bin mcp-link-desktop`
- `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --no-default-features --features server --bin mcp-link-server`
- Windows Rust：在 `G:\code\mcp-link\apps\desktop\src-tauri` 执行
  `cargo fmt; cargo check --features desktop --bin mcp-link-desktop; cargo check --no-default-features --features server --bin mcp-link-server; cargo test`

## Review 要点

- UI 是否只通过 `usePlatformAPI()` 调用平台能力？
- 持久化数据是否进入 SQLite？
- 权限是否显式且最小化？
- 旧兼容路径是否在新路径生效后移除？
- 文档、脚本、CI 是否指向 `apps/desktop` 和 `apps/web`？
