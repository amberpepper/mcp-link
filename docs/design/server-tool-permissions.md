# MCP Server Tool 权限设计

## 目标

- 用户可以在服务详情里启用或禁用单个 MCP tool。
- `toolPermissions` 随 server 配置持久化，重启后仍生效。
- 聚合后的 tool 列表不展示被禁用的 tool，运行时也禁止调用。

## 当前实现入口

- Web UI：`apps/web/src/renderer/components/mcp/server/`
- Platform API：`packages/shared/src/types/platform-api/domains/server-api.ts`
- Rust 后端：`apps/desktop/src-tauri/src/platform/mod.rs`
- MCP 运行时过滤：`apps/desktop/src-tauri/src/mcp/server.rs`

## 数据模型

`toolPermissions` 是 server 对象上的 map：

```typescript
type ToolPermissions = Record<string, boolean>;
```

- `false` 表示显式禁用。
- 缺失或 `true` 表示允许。

桌面端通过 SQLite 的 `store_state` 保存 server 状态，不再使用旧桌面数据库表。

## 运行时规则

- `list_tools` 只返回 `toolPermissions[name] !== false` 的 tool。
- `call_tool` 遇到被禁用的 tool 时返回 not found/拒绝调用。
- access key 的 `serverAccess` 先过滤 server，再应用 tool 权限。

## 验证

- 切换 tool 权限后保存，重开服务详情仍保持状态。
- 聚合 `/mcp` 列表不包含禁用 tool。
- 直接调用禁用 tool 会失败。
