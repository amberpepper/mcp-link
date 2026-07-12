<h1 align="center">MCP Link</h1>
<h3 align="center">统一的 MCP 服务器管理应用</h3>

<div align="center">

[[English](README.md) | [日本語](README_ja.md) | 中文]

</div>

## 项目简介

MCP Link 用一个界面统一管理本地和远程 Model Context Protocol（MCP）服务器，既可以作为桌面应用运行，也可以作为带 Web 管理界面的无头 Server 运行。

### 主要功能

- 连接本地 stdio 和远程 HTTP MCP 服务器
- 导入 DXT、JSON 配置，或手动添加服务器
- 启用或停用服务器及单个工具
- 创建访问 Key，并控制每个 Key 能访问哪些服务器
- 查看请求日志、同步 Agent Skills
- 支持 Windows、macOS、Linux 和 Docker
- 从 GitHub Releases 检查并安装签名更新

## 安装

从 [GitHub Releases](https://github.com/amberpepper/mcp-link/releases) 下载桌面安装包或 Server 二进制文件。

## 开发运行

### 环境要求

- Node.js 22 或更高版本
- pnpm 10
- Rust stable
- 当前系统对应的 [Tauri 环境依赖](https://v2.tauri.app/start/prerequisites/)

### 克隆与安装依赖

```bash
git clone https://github.com/amberpepper/mcp-link.git
cd mcp-link
pnpm install
```

### 启动桌面应用

```bash
pnpm dev:desktop
```

Rust 编译和桌面打包应在对应系统的原生环境运行。Windows 请使用 PowerShell 或 CMD，不要在 WSL 中执行。

### 启动 Server 模式

```bash
pnpm dev:server
```

打开 <http://127.0.0.1:3284>。默认管理密码为 `admin`，首次登录后请在设置页面修改。

修改监听地址：

```bash
MCP_LINK_HTTP_ADDR=0.0.0.0:3284 pnpm dev:server
```

PowerShell：

```powershell
$env:MCP_LINK_HTTP_ADDR = "0.0.0.0:3284"
pnpm dev:server
```

### 仅启动 Web 前端

```bash
pnpm dev:web
```

### 生产构建

```bash
pnpm build:desktop
pnpm build:server
```

GitHub Actions 会构建 Windows、macOS、Linux 的 Desktop 和 Server 产物。推送 `v1.0.0` 这样的版本标签会自动创建 GitHub Release。

## Docker

构建并运行无头 Server 版本：

```bash
docker build -t mcp-link:latest .
docker volume create mcp-link-data
docker run --rm \
  -p 3284:3284 \
  -v mcp-link-data:/app \
  --name mcp-link \
  mcp-link:latest
```

打开 <http://localhost:3284>，使用默认密码 `admin` 登录。更多说明见 [Docker 部署文档](docs/DOCKER.md)。

## 隐私与安全

配置、凭据、日志和服务器数据均保存在本地。将 Server 模式开放到局域网或公网时，请修改默认密码，并根据需要限制网络访问。

## 项目地址

<https://github.com/amberpepper/mcp-link>

## 许可证

本项目采用 Sustainable Use License，详情见 [LICENSE.md](LICENSE.md)。
