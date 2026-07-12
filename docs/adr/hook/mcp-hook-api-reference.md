# MCP Hook API 参考

## 概览

Hook 脚本用于在 MCP 请求处理前后执行自定义逻辑。当前 Hook 由 Workflow
系统调度。

## 执行时机

- Pre-hook：MCP 请求发送到目标 server 前执行，可修改或阻断请求。
- Post-hook：MCP 响应返回后执行，可记录、转换或校验响应。

## 上下文

脚本可以读取 `context`：

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
    error?: string;
    shared?: Record<string, unknown>;
  };
}
```

## 返回值

脚本应返回：

```typescript
interface HookResult {
  continue: boolean;
  context?: HookContext;
  error?: {
    code: string;
    message: string;
  };
}
```

- `continue: true`：继续执行后续节点。
- `continue: false`：停止当前流程。
- `context`：传给后续节点的更新后上下文。
- `error`：停止或失败原因。

## 示例：修改请求参数

```javascript
if (context.request.method === "tools/call") {
  return {
    continue: true,
    context: {
      ...context,
      request: {
        ...context.request,
        params: {
          ...context.request.params,
          maxResults: 10,
          language: "zh",
        },
      },
    },
  };
}

return { continue: true };
```

## 示例：简单限流

```javascript
const shared = context.metadata.shared ?? {};
const key = `${context.metadata.clientId}:${context.request.method}`;
const now = Date.now();
const last = shared[key];

if (last && now - last < 1000) {
  return {
    continue: false,
    error: {
      code: "RATE_LIMITED",
      message: "请求过于频繁，请稍后重试",
    },
  };
}

shared[key] = now;

return {
  continue: true,
  context: {
    ...context,
    metadata: {
      ...context.metadata,
      shared,
    },
  },
};
```

## 安全规则

- 不要在日志里输出 token、password、API key。
- 不要依赖 Hook 绕过 access key 或 server 权限。
- 重任务应放到后端受控实现里，不要放在 Hook 脚本中。
- Hook 失败时应返回明确错误，而不是吞掉异常。
