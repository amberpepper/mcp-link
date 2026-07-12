# 安全说明

这份文档记录当前 Rust/Tauri 和 Web 实现中的安全敏感点。

## Access Key

- Access key 是独立管理对象，不是 Settings 里的单个配置项。
- 明文 key 只在创建时返回一次。
- 持久化时只保存 SHA-256 hash 和短 prefix。
- 每个 key 都有独立的 `serverAccess` 映射。
- `/mcp` 必须带 `Authorization: Bearer <key>`，并按 key 允许的 server ID
  过滤 tools、resources、prompts 和调用。

相关文件：

- `apps/desktop/src-tauri/src/access_keys.rs`
- `apps/desktop/src-tauri/src/mcp/server.rs`
- `apps/web/src/renderer/components/keys/KeyManager.tsx`
- `packages/shared/src/types/platform-api/domains/access-key-api.ts`

## 本地 MCP HTTP 入口

- 桌面 MCP endpoint 默认绑定 `127.0.0.1`。
- 没有明确外部访问设计前，不应改为 `0.0.0.0`。
- health 输出不能包含密钥或 token。

## SQLite 状态

- 桌面版和 Server 版状态都保存在程序可执行文件同目录的 `mcp.db`。
- `store_state` 保存结构化应用状态。
- `access_keys` 保存 access key 元数据和 hash。
- 旧 `state.json` 和独立 `access-keys.sqlite` 只作为迁移输入。

相关文件：

- `apps/desktop/src-tauri/src/state.rs`
- `apps/desktop/src-tauri/src/access_keys.rs`

## 远端 URL 和代理请求

任何会请求用户输入 URL 的功能，都必须验证 scheme、host 和 redirect 行为。
除非功能明确要求，否则不要允许访问本机网络、内网地址或云 metadata 地址。

## Workflow 和 Hook

Workflow/Hook 会处理 MCP 输入输出。扩展这块时必须保留校验、环检测、敏感字段
脱敏和执行限制。

相关文件：

- `apps/desktop/src-tauri/src/workflow/`
- `apps/desktop/src-tauri/src/hook/`

## 日志

请求日志不能保存 raw access key、bearer token、API key 或带 secret 的完整 payload。
持久化前需要脱敏。
