# ADR：MCP Workflow 与 Hook 系统

## 状态

已采纳。

## 背景

MCP Link 聚合多个 MCP server。用户需要在 MCP 请求前后插入自定义逻辑，
例如修改参数、阻断调用、记录审计信息或对响应做转换。

## 决策

Hook 不作为独立系统存在，而是作为 Workflow 的节点运行。

核心入口：

- Web 编辑器：`apps/web/src/renderer/components/workflow/`
- Web store：`apps/web/src/renderer/stores/workflow-store.ts`
- Rust 执行器：`apps/desktop/src-tauri/src/workflow/`
- Hook runtime：`apps/desktop/src-tauri/src/hook/`

## Workflow 模型

Workflow 由 nodes 和 edges 组成：

- `start`
- `end`
- `mcp-call`
- `hook`

`hook` 节点可以同步执行，也可以 fire-and-forget。

## 执行规则

- 保存或启用前必须校验图结构。
- 不允许循环图导致无限执行。
- 同步 hook 会阻塞主流程并返回结果。
- 异步 hook 不阻塞主流程，错误只记录日志。
- MCP call 节点仍然需要经过 access key、server access 和 tool permission。

## Hook 上下文

Hook 能拿到当前请求上下文：

```typescript
interface HookContext {
  request: {
    method: string;
    params: unknown;
  };
  response?: unknown;
  metadata: {
    clientId: string;
    serverId?: string;
    serverName?: string;
    shared?: Record<string, unknown>;
  };
}
```

## 安全约束

- Hook 不能直接访问文件系统。
- Hook 不能直接读取 access key 明文。
- 上下文和日志必须脱敏 token、password、API key 等字段。
- 执行时间和输出大小需要受控。

## 后续方向

- 更细的资源限制。
- 更明确的 hook 权限模型。
- Workflow 运行历史和失败排查 UI。
