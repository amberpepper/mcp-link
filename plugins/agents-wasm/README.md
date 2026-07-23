# Agent WASM plugins

These crates target `wasm32-unknown-unknown` and use `mcp-link-agent-wasm-sdk` for Host file and SQLite calls.

Build on the native Windows Rust environment:

```powershell
rustup target add wasm32-unknown-unknown
cd G:\code\mcp-router\plugins\agents-wasm
cargo build --release --target wasm32-unknown-unknown
```

Copy each resulting `.wasm` to `plugins/agents/<id>/plugin.wasm` before
creating the `.mclagent` package. The matching
`plugins/agents/<id>/manifest.json` and its icon assets are the package source
of truth; do not keep second copies beside the Rust crate because the packager
does not read them.

## Management adapters

Management capabilities belong to the plugin adapter. A plugin declares each
section with an opaque `id` and a reusable Web `renderer`, then owns the
section's normalized data, validation, and config mutation. Do not
add CLI names, section IDs, or CLI config keys to the desktop host.

Use `management_section_descriptor` and `management_section` from the SDK for
the protocol envelopes. Preserve unknown native config fields during writes
and cover that behavior with plugin-local tests.

For scalar configuration pages, declare `renderer: "form"` and return the
dynamic form contract (`schemaVersion`, localized `groups`, generic `fields`,
and opaque `values`). Field keys, enum options, and validation remain inside
the plugin; the Web app must not import CLI-specific configuration knowledge.
