# 模型网关

MCP Link 使用一个当前选中的上游 API，同时提供 OpenAI Chat Completions、
OpenAI Responses 和 Anthropic Messages 接口。

```text
OpenAI Chat Completions：http://127.0.0.1:3285/openai/v1/chat/completions
OpenAI Responses：       http://127.0.0.1:3285/openai/v1/responses
Anthropic Messages：     http://127.0.0.1:3285/anthropic/v1/messages
```

每个 API 提供商选择一种上游格式：OpenAI Compatible（Chat
Completions）、OpenAI Responses 或 Anthropic Messages，并分别配置 Base
URL、API Key 和模型列表。模型映射属于具体提供商，不是全局配置。例如
Codex 请求 `gpt-5` 时，可以转发到上游 `claude-opus-4-6`：

```text
客户端模型：gpt-5
上游模型：  claude-opus-4-6
```

当客户端格式与上游格式不同时，网关会转换请求、普通 JSON 响应和 SSE
流，覆盖文本、图片、instructions/system、函数工具、工具调用/结果、结束原因和
Token usage。Responses 独有的托管工具及 `previous_response_id` 在 Responses
上游中原样透传；转换到无状态 Compatible 或 Anthropic 上游时会明确报错，不会
静默丢弃。没有映射的模型名会原样透传。

客户端使用 Gateway Key 作为 Bearer Token 连接。
