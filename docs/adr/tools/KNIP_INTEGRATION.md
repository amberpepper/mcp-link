# ADR：Knip 死代码检测

## 状态

已采纳。

## 背景

MCP Link 是 pnpm monorepo。随着 `apps/desktop`、`apps/web`、`apps/cli`
和多个 package 增长，未使用文件、依赖和导出会逐渐堆积。

## 决策

使用 [Knip](https://knip.dev/) 检测未使用代码、依赖和导出。

配置入口：

- `knip.json`
- `package.json` 的 `pnpm knip`

## 价值

- 发现未使用依赖。
- 发现未使用导出。
- 帮助清理迁移后的旧代码。
- 降低构建和维护成本。

## 使用方式

```bash
pnpm knip
pnpm knip --debug
pnpm knip --include-config-hints
```

## 维护规则

- 删除一个 app 或 package 后，同步更新 `knip.json`。
- 对确认为 false positive 的项，优先写精确 ignore。
- 新增 workspace 时补充对应入口和 project globs。
- 不要用宽泛 ignore 掩盖真实未使用代码。

## 备选方案

- ESLint `no-unused-vars`：只能发现变量级问题，不能覆盖依赖和文件。
- `ts-prune`：覆盖范围比 Knip 窄。
- 手工检查：不可持续，容易漏。
