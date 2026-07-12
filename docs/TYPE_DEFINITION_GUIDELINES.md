# TypeScript 类型定义规范

MCP Link 通过集中管理类型，保证 Rust core、Tauri 桌面端、Rust server
和 Web 前端之间的接口稳定。

## 允许的位置

- `packages/shared/src/types/`：共享领域类型和 Platform API 类型。
- `apps/web/src/renderer/platform-api/`：前端运行时适配器相关类型。
- `*.d.ts`：全局声明。
- 仅被单个组件使用的 Props 可以留在 `.tsx` 组件文件里。

## 不允许的位置

不要在组件、store、service、utility 或生产代码测试外的位置散落新的领域类型或
API 类型。测试专用类型可以留在测试文件内。

## 导入方式

优先从 shared 包导入：

```typescript
import type { MCPServer, PlatformAPI } from "@mcp_link/shared";
```

不要在局部重复定义共享领域类型。字段属于共享契约时，先改
`packages/shared/src/types/`，再更新调用方。

## Platform API 类型

新增 API domain 放在：

```text
packages/shared/src/types/platform-api/domains/
```

并从这些文件导出：

```text
packages/shared/src/types/platform-api/index.ts
packages/shared/src/types/platform-api/platform-api.ts
packages/shared/src/types/index.ts
```

## ESLint 约束

自定义规则 `custom/no-scattered-types` 会检查大范围类型定义是否放在允许位置。
较大的 TypeScript 改动提交前应执行 `pnpm lint:fix`。
