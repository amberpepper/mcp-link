# Platform and Agent Architecture

## Platform targets

The desktop and server targets are mutually exclusive:

```text
desktop  = Tauri shell, native dialogs, autostart, desktop MCP endpoint
server   = HTTP server, embedded web assets, no Tauri APIs
```

Build the server with `--no-default-features --features server`. The crate now
rejects a build that enables both targets.

`getPlatformCapabilities` is the runtime capability contract. Clients should
hide native actions when `desktopDialogs`, `autostart`, or
`mcpHttpEndpoint` is false instead of calling a Desktop-only method and
waiting for a runtime error.

## Agent layers

```text
plugins/agents-wasm/  Rust/WASM adapter source
plugins/agents/       packaged manifests, icons, and plugin.wasm artifacts
```

All CLI-specific adapters are WASM plugins. The default desktop bundle does
not preinstall CLI plugins; users import the packages they need. A distributor
may opt into preinstallation through `plugins/agents/bundled.txt`, using the
same package/runtime boundary as user-installed plugins. Build `plugin.wasm`
from `plugins/agents-wasm/`, keep the package manifest and assets under
`plugins/agents/<id>/`, and regenerate `dist/` packages with the packaging
script.

Each plugin manifest is the source of truth for session paths, config files,
skills, resume arguments, and platform path hints.

## Agent management boundary

AI CLI configuration is plugin-owned. The dependency direction is fixed:

```text
Web generic renderer <- shared renderer contract <- WASM plugin adapter
                                               ^
Desktop host (capability checks, invocation, generic envelopes, atomic I/O)
```

The desktop host may validate only protocol-level properties: plugin and
instance identity, identifier syntax and size limits, revisions, mutation
envelopes, capabilities, and atomic resource access. Section identifiers and
section `data` are opaque to it.

The desktop host must not:

- enumerate management section names;
- parse or write a CLI's configuration keys;
- branch on a section such as models, providers, or permissions;
- validate a plugin's section-specific payload shape.

Every management section descriptor contains a plugin-owned `id`, a stable
`renderer`, and a `source` (`plugin` or `host`). The Web app selects a reusable
renderer from `renderer`, uses `source` to decide where data comes from, and
sends every plugin load and mutation back with the original plugin-owned
`id`. A new CLI section therefore requires no change in the desktop host.

CLI-specific scalar settings use the reusable `form` renderer. The plugin
returns localized groups, field controls, select options, defaults, and an
opaque values map. The Web renderer knows only generic controls (`text`,
`select`, `switch`, and `textarea`); native keys such as a CLI's sandbox or
reasoning settings must not appear in Web or host source code.

Before merging an Agent management feature, verify that:

1. CLI discovery, parsing, validation, preview, and mutation live under the
   matching `plugins/agents-wasm/<agent>/` crate.
2. Reusable plugin protocol helpers live in `plugins/agents-wasm/sdk/`.
3. `apps/desktop/.../platform/agents/management.rs` remains an opaque
   transport boundary.
4. A custom section ID passes the host boundary tests without adding an
   allowlist entry.
