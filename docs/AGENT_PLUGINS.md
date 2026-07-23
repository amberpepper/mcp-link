# MCP Link Agent plugins

Agent plugins add support for AI CLIs without changing MCP Link itself. A plugin is a ZIP archive with the `.mclagent` extension and a `manifest.json` at its root.

## Manifest

```json
{
  "schemaVersion": 2,
  "id": "example-agent",
  "name": "Example Agent",
  "version": "1.0.0",
  "description": "Example AI CLI adapter",
  "capabilities": [
    "sessions.list",
    "sessions.read",
    "sessions.stats",
    "sessions.resume",
    "sessions.duplicate",
    "sessions.branch",
    "sessions.delete",
    "sessions.export-native",
    "sessions.import",
    "config.read",
    "config.write",
    "management.read",
    "management.write"
  ],
  "instanceConfig": {
    "sessionPathKind": "directory",
    "homeLevelsUp": 1,
    "sessionPathTemplate": "${ROOT}/sessions",
    "skillPathTemplate": "${ROOT}/skills",
    "command": "example-agent",
    "resumeArguments": ["resume", "{sessionId}"]
  },
  "configFiles": [
    {
      "id": "settings",
      "label": "settings.json",
      "pathTemplate": "${ROOT}/settings.json",
      "language": "json",
      "defaultContent": "{}\n"
    }
  ],
  "skillTargets": [
    {
      "id": "example-global",
      "label": "Example global Skills",
      "scope": "global",
      "pathTemplate": "${HOME}/.example/skills",
      "mode": "copy",
      "format": "agents-skill"
    }
  ],
  "files": [
    {
      "id": "data",
      "pathTemplate": "${ROOT}",
      "access": "read-only"
    }
  ],
  "databases": [
    {
      "id": "sessions",
      "pathTemplate": "${SESSION_ROOT}",
      "access": "read-only"
    }
  ],
  "runtime": {
    "kind": "wasm",
    "entry": "plugin.wasm"
  }
}
```

WASM entries must be safe relative `.wasm` paths inside the package. Plugins do not require Node or another system runtime.

## Instances

An Agent plugin describes one CLI adapter. The user adds an environment by selecting that CLI's configuration directory, such as `.codex` or `.claude`. The same plugin can be added for Windows and multiple WSL distributions by selecting a different configuration directory for each environment. The plugin fixes the session path, Skill path, CLI command, and resume arguments; these values are not user-editable.

`instanceConfig.sessionPathKind` is `directory` or `file`. `homeLevelsUp` tells MCP Link how many parent directories separate the selected configuration root (`${ROOT}`) from `${HOME}`. MCP Link checks only the exact directory selected by the user. It does not enumerate WSL distributions, search PATH, locate executables, or scan drives. A `\\wsl.localhost\\<distribution>\\...` or `\\wsl$\\<distribution>\\...` configuration path identifies the WSL environment.

Adding an instance and reading its sessions never executes the CLI. MCP Link uses the plugin's fixed `command` and `resumeArguments` only when the user explicitly resumes a session.

`configFiles` exposes CLI configuration files through the instance editor. Each entry has a stable `id`, display `label`, `pathTemplate`, and editor `language` (`json`, `jsonc`, `toml`, `yaml`, or `text`). Paths must use `${ROOT}`, `${HOME}`, or `${LOCALAPPDATA}` and are resolved from the directory selected for that instance. `defaultContent` is shown when the file does not exist; the file and its parent directory are created only when the user saves. Use `config.read` and `config.write` to advertise the corresponding operations. Configuration files are read and written by MCP Link itself and do not require a plugin runtime method.

Schema 2 plugins use `runtime.kind: "wasm"` with an `entry` ending in `.wasm`. The WASM SDK exports `mcp_link_call`, `mcp_link_alloc`, and `mcp_link_dealloc`. Plugins do not open arbitrary paths themselves; they call the Host SDK for `file.read`, `file.write`, `file.list`, `file.remove`, `sqlite.query`, and `sqlite.transaction`. The plugin still owns the SQL and session conversion logic. `files` and `databases` limit each resource to the selected instance, and `read-write` must be explicitly declared before a write is allowed. Node/process plugins and Schema 1 manifests are not supported.

```json
{
  "input": {
    "agentId": "example-agent",
    "configRoot": "\\\\wsl.localhost\\Ubuntu\\home\\me\\.example"
  },
  "derivedInstance": {
    "id": "user-generated-id",
    "agentId": "example-agent",
    "label": "Example Agent · Ubuntu",
    "cliRoot": "\\\\wsl.localhost\\Ubuntu\\home\\me\\.example",
    "sessionRoot": "\\\\wsl.localhost\\Ubuntu\\home\\me\\.example\\sessions",
    "skillRoot": "\\\\wsl.localhost\\Ubuntu\\home\\me\\.example\\skills",
    "resumeCommand": "wsl.exe -d Ubuntu -- example-agent resume {sessionId}"
  }
}
```

The complete instance object is included alongside method-specific fields such as `nativeId`, `session`, `title`, and `cwd`. A plugin runtime must use the supplied instance paths instead of discovering or assuming a global path.

## Runtime protocol

MCP Link loads `plugin.wasm` with Wasmtime for each call and invokes the exported `mcp_link_call` function with one JSON-RPC 2.0 request. The plugin returns a packed pointer and length for one JSON-RPC response. Execution is fuel-limited, response size is limited, and the module has no WASI filesystem or process access.

When the plugin needs an authorized file or SQLite resource, it calls the imported `mcp_link::host_call` function. The Host validates the resource ID, access mode, selected instance path, relative path, SQL parameters, and size limits before performing the operation. Plugins therefore do not require Node or any other separately installed runtime.

### CLI management protocol

Plugins advertising `management.read` provide a normalized management center for one CLI instance. `management.write` additionally enables direct configuration mutations. MCP Link owns the page layout and standard domain components; plugins return data, not arbitrary UI schemas.

The runtime methods are:

- `describeManagement`: returns the supported sections for the selected instance.
- `loadManagementSection`: parses native files into one normalized section.
- `mutateManagementSection`: validates and applies one normalized mutation.

Section IDs are opaque plugin-owned identifiers. A descriptor selects a
reusable renderer such as `overview`, `form`, `mcp`, `skills`, `providers`,
`models`, `permissions`, `environment`, or `raw-config`; scalar CLI settings
should use `form` so a new section does not require host or Web changes.
Unsupported sections must be omitted from the descriptor. Every loaded
configuration section includes a content revision. Every mutation includes
`expectedRevision`; stale writes must fail with `CONFIG_CONFLICT`.

Plugins must preserve unknown native fields when changing a supported field. API keys, tokens, passwords, and other secrets are returned only as a masked state such as `{ "configured": true, "masked": "••••••••" }`. A blank secret input keeps the existing value; plugins must never send the existing raw secret to the renderer.

Configuration resources used by management adapters must be declared `read-write`. The Host methods `file.read` with `includeRevision: true` and `file.writeAtomic` implement revision checks, temporary-file replacement, and `.mcp-link-backups` backups. JSON helpers are available in the WASM SDK; format-sensitive adapters such as TOML should use a lossless editor and call the same atomic Host method.

Supported methods are:

- `probe`
- `listSessions`
- `loadSession`
- `loadSessionStats`
- `resumeCommand`
- `duplicateSession`
- `deleteSession`
- `renameSession`
- `importSession`
- `exportNative`
- `installSkill`
- `removeSkill`
- `describeManagement`
- `loadManagementSection`
- `mutateManagementSection`

Only advertise methods implemented by the runtime in `capabilities`. A Skill target with `mode: "native"` must implement `installSkill` and `removeSkill`. `installSkill` receives the Skill plus the previous installation record when updating, and may return `installedPath` and `nativeReference`; MCP Link persists both and supplies the complete installation record to `removeSkill`.

`loadSessionStats` is an optional operation advertised with `sessions.stats`. It receives `instance` and `nativeId`, and returns reported usage without loading conversation messages. Return `null` when the source session has no reliable usage data. Omit unavailable fields instead of estimating them:

```json
{
  "inputTokens": 12000,
  "outputTokens": 1800,
  "cachedInputTokens": 8000,
  "cacheWriteTokens": 0,
  "reasoningTokens": 420,
  "totalTokens": 13800,
  "cost": 0.042,
  "contextWindow": 200000,
  "source": "reported"
}
```

## Installation safety

Importing a package is an explicit user action. Plugin security relies on the WASM sandbox, declared Host resources, and package validation.

Installation is staged and validated before the existing plugin is replaced. Unsafe paths, more than 4,096 entries, and expanded packages larger than 128 MB are rejected.
