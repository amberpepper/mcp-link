# 数据库 Schema 管理

## 目标

保证 SQLite schema 可以随版本演进，同时保留用户本地数据。

## 基本规则

- `CREATE TABLE IF NOT EXISTS` 用于新表。
- `ALTER TABLE` 前先检测列是否存在。
- 迁移必须幂等。
- 迁移失败要返回错误并停止后续写入。

## 当前迁移入口

- `apps/desktop/src-tauri/src/state.rs`
- `apps/desktop/src-tauri/src/access_keys.rs`

## 旧数据来源

- `state.json`：旧桌面状态文件，只读迁移。
- `access-keys.sqlite`：旧 access key 独立库，只读迁移。

## 新增表流程

1. 在拥有该数据的 Rust 模块里添加 `open_connection` 或 schema 初始化逻辑。
2. 写幂等 schema 创建语句。
3. 写旧数据迁移函数。
4. 添加单元测试覆盖空库、旧库和重复迁移。
5. 更新文档和 Platform API。

## 不推荐

- 在 Web 组件里直接读写存储。
- 用字符串拼接 SQL 参数。
- 新增独立 JSON 状态文件。
