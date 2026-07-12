# ADR：数据库架构

## 状态

已采纳。

## 背景

桌面端已经迁移到 Rust/Tauri。旧桌面实现中的 TypeScript repository/database
结构不再适用，本地持久化统一使用 SQLite。

## 决策

桌面本地状态存放在一个 SQLite 文件中：

```text
mcp.db
├── store_state
└── access_keys
```

Desktop 和 Server 都把 `mcp.db` 放在当前可执行文件所在目录。

当前入口：

- 状态加载与迁移：`apps/desktop/src-tauri/src/state.rs`
- Access key 表：`apps/desktop/src-tauri/src/access_keys.rs`
- Platform API 写入：`apps/desktop/src-tauri/src/platform/mod.rs`

## `store_state`

`store_state` 以 top-level key/value JSON 的方式保存应用状态：

- `servers`
- `settings`
- `workflows`
- `hooks`
- `skills`

这种结构保留了现有 Web 类型的灵活性，同时避免继续写 `state.json`。

## `access_keys`

`access_keys` 保存服务端调用 `/mcp` 所需的 key 管理数据：

- `id`
- `name`
- `key_prefix`
- `token_hash`
- `server_access`
- `created_at`
- `last_used_at`

明文 key 只在创建时返回一次。

## 迁移

- 如果 `mcp.db` 为空，会尝试从旧 `state.json` 迁移。
- 如果旧 `access-keys.sqlite` 存在，会迁移到同一个 `mcp.db`。
- 旧文件只作为输入，不再继续写入。

## 后续方向

当某类数据需要复杂查询或高频写入时，可以从 `store_state` 中拆成独立表。
拆表必须带迁移和回滚策略。
