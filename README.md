<h1 align="center">MCP Link</h1>
<h3 align="center">A unified MCP server management app</h3>

<div align="center">

[English | [日本語](README_ja.md) | [中文](README_zh.md)]

</div>

## Overview

MCP Link manages local and remote Model Context Protocol (MCP) servers from one interface. It is available as a desktop application and as a headless server with a web UI.

### Features

- Connect to local stdio and remote HTTP MCP servers
- Import DXT and JSON configurations or configure servers manually
- Enable or disable servers and individual tools
- Create access keys and control which servers each key can access
- View request logs and synchronize Agent Skills
- Run on Windows, macOS, Linux, or Docker
- Check for and install signed updates from GitHub Releases

## Installation

Download desktop installers and server binaries from [GitHub Releases](https://github.com/amberpepper/mcp-link/releases).

## Development

### Requirements

- Node.js 22 or later
- pnpm 10
- Rust stable
- The [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your operating system

### Clone and install

```bash
git clone https://github.com/amberpepper/mcp-link.git
cd mcp-link
pnpm install
```

### Start the desktop app

```bash
pnpm dev:desktop
```

Rust and desktop packaging should be run from the native operating-system environment. On Windows, use PowerShell or Command Prompt rather than WSL.

### Start server mode

```bash
pnpm dev:server
```

Open <http://127.0.0.1:3284>. The default server password is `admin`; change it from Settings after the first login.

To listen on another address:

```bash
MCP_LINK_HTTP_ADDR=0.0.0.0:3284 pnpm dev:server
```

In PowerShell:

```powershell
$env:MCP_LINK_HTTP_ADDR = "0.0.0.0:3284"
pnpm dev:server
```

### Start only the web frontend

```bash
pnpm dev:web
```

### Production builds

```bash
pnpm build:desktop
pnpm build:server
```

GitHub Actions builds Desktop and Server artifacts for Windows, macOS, and Linux. Pushing a version tag such as `v1.0.0` creates a GitHub Release.

## Docker

Build and run the headless Server edition:

```bash
docker build -t mcp-link:latest .
docker volume create mcp-link-data
docker run --rm \
  -p 3284:3284 \
  -v mcp-link-data:/app \
  --name mcp-link \
  mcp-link:latest
```

Open <http://localhost:3284> and log in with the default password `admin`. See [Docker deployment](docs/DOCKER.md) for details.

## Privacy and security

Configurations, credentials, logs, and server data are stored locally. When exposing Server mode to a network, change the default password and restrict network access as appropriate.

## Repository

<https://github.com/amberpepper/mcp-link>

## License

This project is licensed under the Sustainable Use License. See [LICENSE.md](LICENSE.md).
