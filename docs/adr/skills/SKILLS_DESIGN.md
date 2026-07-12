# Skills 管理设计

## 目标

- 在 MCP Link 中集中管理 Agent Skills。
- 将 skill 同步到多个 agent 的个人目录。
- 支持启用、禁用、编辑和导入。

## 当前入口

- Web UI：`apps/web/src/renderer/components/skills/`
- Store：`apps/web/src/renderer/stores/`
- Platform API：`packages/shared/src/types/platform-api/domains/skills-api.ts`
- Rust 后端：`apps/desktop/src-tauri/src/platform/mod.rs`

## 类型

核心类型在：

```text
packages/shared/src/types/skill-types.ts
packages/shared/src/types/platform-api/domains/skills-api.ts
```

Skill 字段：

- `id`
- `name`
- `enabled`
- `createdAt`
- `updatedAt`

## 文件系统

Skill 内容以目录和 `SKILL.md` 表示。数据库只保存元数据，不保存可由路径推导的绝对路径。

启用 skill 时，对已配置 agent path 创建链接或同步文件；禁用时移除链接。

## Agent Path

用户可以配置多个 agent path。默认候选包括：

- Claude Code
- OpenAI Codex
- GitHub Copilot
- Cline
- OpenCode

## 安全

- 导入目录时必须限制路径穿越。
- 写文件前确认目标路径在允许目录下。
- 不在日志中输出用户目录下的敏感内容。
