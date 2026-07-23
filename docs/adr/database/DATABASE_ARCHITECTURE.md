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
├── access_keys
├── gateway_providers
├── gateway_routes
├── agent_instances
└── skill_installations
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

Gateway、Agent 实例和 Skill 安装关系不允许写入 `store_state`，必须使用字段化表、主键、索引和数据库约束。

## 领域表

- `gateway_providers`：协议、地址、密钥、模型和启用状态。
- `gateway_routes`：提供商内唯一的模型别名映射，并通过外键级联删除。
- `agent_instances`：CLI 类型、配置根目录、会话目录、Skill 目录和恢复命令。
- `skill_installations`：Skill、Agent、目标和安装状态关系。

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

## 新领域规则

新增业务领域不能继续写入 `store_state`。只有低频、无关系查询需求的应用配置可以保留为 JSON；实体和实体关系必须建立正式表。
