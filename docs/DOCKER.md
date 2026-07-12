# Docker 部署

Docker 镜像运行的是 Server 版 `mcp-link-server`，不包含桌面 WebView。

## 构建镜像

```bash
docker build -t mcp-link:latest .
```

## 运行

```bash
docker run --rm -p 3284:3284 --name mcp-link mcp-link:latest
```

默认监听地址：

```text
0.0.0.0:3284
```

容器内数据库路径：

```text
/app/mcp.db
```

如需持久化，使用命名 volume 挂载 `/app`：

```bash
docker volume create mcp-link-data
docker run --rm \
  -p 3284:3284 \
  -v mcp-link-data:/app \
  --name mcp-link \
  mcp-link:latest
```

打开 `http://localhost:3284`，使用默认密码 `admin` 登录。首次登录后请在设置页面修改密码。

源码与发布地址：<https://github.com/amberpepper/mcp-link>
