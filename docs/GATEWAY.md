# Model Gateway

MCP Link exposes OpenAI Chat Completions, OpenAI Responses, and Anthropic
Messages endpoints from one selected upstream provider.

```text
OpenAI Chat Completions: http://127.0.0.1:3285/openai/v1/chat/completions
OpenAI Responses:        http://127.0.0.1:3285/openai/v1/responses
Anthropic Messages:      http://127.0.0.1:3285/anthropic/v1/messages
```

Choose one upstream format for each provider: OpenAI Compatible (Chat
Completions), OpenAI Responses, or Anthropic Messages. Configure its base URL,
API key, and models. Model mappings belong to that provider, not to the gateway
globally. For example, a Codex request for `gpt-5` can be forwarded to
`claude-opus-4-6`:

```text
client model:   gpt-5
upstream model: claude-opus-4-6
```

When the client format differs from the upstream format, the gateway converts
requests, JSON responses, and SSE streams. Text, images, instructions/system
messages, function tools, tool calls/results, finish reasons, and token usage
are converted. Responses-only hosted tools and `previous_response_id` pass
through to Responses upstreams; conversion to a stateless Compatible or
Anthropic upstream fails explicitly rather than silently dropping them.
Unmapped model names are forwarded unchanged.

Clients authenticate with the Gateway Key using a bearer token.
