# AI CLI native configuration capability matrix

This matrix records what each Agent plugin may expose in MCP Link's management
center. A section is present only when the CLI has a verified native file and
schema for that capability. Plugins mutate the CLI's own configuration, keep
unrelated and unknown fields, and never replace the user's existing Provider or
MCP list.

`Yes` means MCP Link provides structured read/write management. `Read` means the
native value is shown but is not safely editable through the current normalized
contract. `No native support` is used only when the upstream project explicitly
does not implement that capability. `Not exposed` means no sufficiently stable
native write shape has been verified; it does not claim the CLI can never gain
the feature.

| CLI | Native configuration managed | MCP | Global instructions | Skills | Provider / model | Permissions |
| --- | --- | --- | --- | --- | --- | --- |
| Claude Code | `~/.claude/settings.json`, `~/.claude.json` | Yes, native `mcpServers` | `~/.claude/CLAUDE.md` | Global and project | Anthropic environment settings plus existing values; MCP Link is only an addable preset | Yes |
| Codex | `~/.codex/config.toml` | Yes, native `mcp_servers` | `~/.codex/AGENTS.md` | `$HOME/.agents/skills` and project `.agents/skills` | Yes, native `model_providers`, model and reasoning settings | Yes |
| Crush | platform Crush `crush.json` | Yes, native `mcp` | `~/.config/crush/CRUSH.md` | Global and project | Yes, native providers and large/small model slots | Not exposed |
| Gemini CLI | `~/.gemini/settings.json` | Yes, native `mcpServers` | `~/.gemini/GEMINI.md` | Gemini global/project and shared project Skills | Model selection; authentication remains in Gemini's native environment settings | Not exposed |
| Grok CLI | `~/.grok/user-settings.json` | Yes, native `mcp.servers` | `~/.grok/AGENTS.md` | Shared global/project `.agents/skills` | Native API key, default model and reasoning effort; base URL remains `GROK_BASE_URL` | Not exposed |
| Kimi Code | `~/.kimi-code/config.toml`, `mcp.json` | Yes, native `mcpServers` | `~/.kimi-code/AGENTS.md` | Global and project | Yes, native Provider/model tables; MCP Link is only an addable Provider preset | Not exposed |
| Oh My Pi | `~/.omp/agent/config.yml`, `models.yml`, `mcp.json` | Yes, native `mcpServers` | `~/.omp/agent/AGENTS.md` | Global and project | Yes, native providers and default/smol/slow roles | Not exposed |
| OpenCode | global `opencode.json` | Yes, native `mcp` | `~/.config/opencode/AGENTS.md` | Global and project | Yes, native providers and model | Yes |
| Pi | `~/.pi/agent/settings.json`, `models.json` | **No native support** | `~/.pi/agent/AGENTS.md` | Pi global/project and shared project Skills | Yes, native providers/models | Not exposed |
| Qwen Code | `~/.qwen/settings.json` | Yes, native `mcpServers` | `~/.qwen/QWEN.md` | Qwen global/project and shared project Skills | Yes, native `modelProviders` entries and selected model | Not exposed |
| Reasonix | `config.toml`, `.env` | Yes, native `[[plugins]]` | `REASONIX.md` | Global and project | Yes, native `[[providers]]`; secrets remain in `.env` | Yes |
| ZCode | `~/.zcode/v2/config.json`, `setting.json`, `~/.zcode/cli/config.json` | Yes for user scope, native `mcp.servers`; workspace scope is not written without an explicit project context | Not exposed: no verified standalone global instruction file | Global and project | Existing Desktop providers are managed in `v2/config.json`; models are read-only | Not exposed |

## Upstream evidence

- Claude Code: [settings](https://code.claude.com/docs/en/settings),
  [MCP](https://code.claude.com/docs/en/mcp),
  [memory/instructions](https://code.claude.com/docs/en/memory), and
  [Skills](https://code.claude.com/docs/en/skills).
- Codex: the current [Codex manual](https://developers.openai.com/codex/codex-manual.md),
  including `config.toml`, MCP, `AGENTS.md`, and the user/project Skill
  locations.
- Crush: the official [charmbracelet/crush](https://github.com/charmbracelet/crush)
  repository and its configuration schema.
- Gemini CLI: the official
  [settings schema](https://raw.githubusercontent.com/google-gemini/gemini-cli/main/schemas/settings.schema.json)
  and [google-gemini/gemini-cli](https://github.com/google-gemini/gemini-cli)
  documentation.
- Grok CLI: the official
  [superagent-ai/grok-cli](https://github.com/superagent-ai/grok-cli) source,
  especially `src/utils/settings.ts` and `src/utils/instructions.ts`.
- Kimi Code: the official
  [MoonshotAI/kimi-cli](https://github.com/MoonshotAI/kimi-cli) documentation
  and the native sample configuration supplied for this adapter.
- Oh My Pi: [model configuration](https://github.com/can1357/oh-my-pi/blob/main/docs/models.md),
  [agent configuration](https://github.com/can1357/oh-my-pi/blob/main/docs/config-usage.md),
  and [MCP configuration](https://github.com/can1357/oh-my-pi/blob/main/docs/mcp-config.md).
- OpenCode: official [configuration](https://opencode.ai/docs/config/),
  [MCP](https://opencode.ai/docs/mcp-servers/),
  [rules](https://opencode.ai/docs/rules/), and
  [Skills](https://opencode.ai/docs/skills/) pages.
- Pi: the official [badlogic/pi-mono](https://github.com/badlogic/pi-mono)
  `packages/coding-agent/docs/models.md` and `docs/settings.md`; the upstream
  coding agent deliberately has no built-in MCP client.
- Qwen Code: the official
  [QwenLM/qwen-code](https://github.com/QwenLM/qwen-code) source and settings
  documentation.
- Reasonix: the
  [DeepSeek-Reasonix `main-v2` source](https://github.com/esengine/DeepSeek-Reasonix/tree/main-v2),
  including `docs/CONFIG_PATHS.md`, `internal/config/config.go`, and
  `internal/config/mcpjson.go`.
- ZCode: official [MCP Servers](https://zcode.z.ai/en/docs/mcp-services),
  [Skills](https://zcode.z.ai/en/docs/skill), and
  [model connection](https://zcode.z.ai/en/docs/configuration) pages. The MCP
  page explicitly defines user scope as `~/.zcode/cli/config.json` with
  `mcp.servers`, workspace scope as `<project>/.zcode/config.json`, and the
  native disable flag as `enable: false`.

## Mutation rules

- Saving applies immediately after validation; there is no redacted preview or
  second confirmation dialog.
- Existing Provider and MCP entries are never cleared when the MCP Link preset
  is added.
- Blank secret inputs retain the existing native secret. Raw saved secrets are
  never returned to the Web renderer.
- Unknown native fields are preserved by structured mutations. Format-sensitive
  TOML adapters use `toml_edit`; JSON/YAML adapters preserve values but may
  normalize whitespace when serialized.
- A project-scoped file is written only after the user has selected an explicit
  project path. An Agent instance alone is not treated as project authority.
