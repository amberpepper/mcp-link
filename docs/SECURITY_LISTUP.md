# 安全后续清单

当前 Rust/Tauri 和 Web 实现的后续安全事项：

- 为 `/mcp` bearer-token 认证失败增加限流或节流。
- 如果要防离线数据库泄露，考虑用带安装级 secret 的 HMAC 或 per-install salt
  保存 access key hash。
- 高频 MCP 请求场景下，不要每次请求都写 `last_used_at`，可以改成节流更新。
- 对 `proxyFetch` 和 `proxyFetchText` 的调用点做 SSRF 审计。
- 远端 MCP URL 校验必须覆盖 scheme、host、redirect 和内网地址范围。
- Workflow context 和请求日志需要持续保持敏感字段脱敏。
- UI 后续补充 key 轮换和审计能力。
