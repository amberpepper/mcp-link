# ADR：Platform API

## 状态

已采纳。

## 背景

Web renderer 需要同时支持两种本地运行方式：

- Tauri 桌面端，由 `apps/desktop/src-tauri` 的 Rust 后端提供能力。
- 浏览器/Server 模式，由 Rust `mcp-link-server` 通过 HTTP 提供能力。

组件不应该知道当前运行时，也不应该直接调用 Tauri、fetch 或后端命令名。

## 决策

由 `packages/shared/src/types` 暴露统一的 `PlatformAPI` 契约，再由不同运行时适配：

- `apps/web/src/renderer/platform-api/tauri-platform-api.ts` 调用 Tauri
  `platform_call`。
- `apps/web/src/renderer/platform-api/http-platform-api.ts` 调用 Rust server
  HTTP API。
- `apps/web/src/renderer/platform-api/create-platform-api.ts` 将类型化 domain API
  映射到平台方法名。
- `apps/web/src/renderer/platform-api/runtime-platform-api.ts` 根据运行时选择
  Tauri IPC 或 HTTP adapter。

## 后端边界

核心能力由 Rust core 负责。新的平台能力应进入
`apps/desktop/src-tauri/src/platform/mod.rs`，或由它调用的专门 Rust 模块。
Tauri desktop shell 只处理原生窗口、文件选择、打开目录等桌面专属能力；
server shell 只处理 HTTP/MCP 暴露和无界面启动配置。

## 当前 domain

- `accessKeys`
- `servers`
- `settings`
- `logs`
- `workflows`
- `skills`

## 约束

- UI 组件使用统一的 `platformAPI` 或 `usePlatformAPI()`。
- 共享方法签名放在 `packages/shared/src/types/platform-api`。
- 运行时差异留在 adapter 或后端模块内。
- 桌面本地持久化使用 SQLite，不再新增 JSON 状态文件。
