# 数据库设计模式

## 当前原则

- Rust/Tauri 桌面端使用 SQLite。
- 不再新增 JSON 状态文件。
- 密钥类数据只保存 hash 或密文，不保存可直接使用的明文。
- 所有 schema 变更需要考虑旧数据迁移。

## 表设计

优先使用清晰的小表。当前已有：

- `store_state`：保存应用状态的 top-level JSON 值。
- `access_keys`：保存 access key 元数据、hash 和 server 权限。

示例：

```sql
CREATE TABLE IF NOT EXISTS access_keys (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  key_prefix TEXT NOT NULL,
  token_hash TEXT NOT NULL UNIQUE,
  server_access TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  last_used_at TEXT
);
```

## JSON 字段

可以使用 JSON 字段保存变化快、查询少的结构，例如 `server_access`。

如果需要按字段过滤、排序或聚合，应拆成独立列或独立表。

## 事务

多 key 写入必须放在事务内。`store_state` 保存时先删除旧 key，再批量写入新值，
并在一个事务内提交。

## 迁移

迁移代码要满足：

- 可以重复执行。
- 不丢失旧数据。
- 旧表不存在时安全跳过。
- 出错时返回明确错误，不吞掉异常。

## Secret 处理

- Access key 明文不入库。
- Bearer token、API key、password 等敏感字段进入日志或同步前必须脱敏。
- 如果未来保存可逆 secret，应使用操作系统安全存储或明确的加密方案。
