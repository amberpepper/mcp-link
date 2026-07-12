<h1 align="center">MCP Link</h1>
<h3 align="center">統合 MCP サーバー管理アプリ</h3>

<div align="center">

[[English](README.md) | 日本語 | [中文](README_zh.md)]

</div>

## 概要

MCP Link は、ローカルおよびリモートの Model Context Protocol（MCP）サーバーを一つの画面で管理します。デスクトップアプリとしても、Web 管理画面を備えたヘッドレス Server としても実行できます。

### 主な機能

- ローカル stdio およびリモート HTTP MCP サーバーへの接続
- DXT、JSON 設定のインポートと手動設定
- サーバーおよび個別ツールの有効化・無効化
- アクセスキーの作成とサーバー単位のアクセス制御
- リクエストログの確認と Agent Skills の同期
- Windows、macOS、Linux、Docker に対応
- GitHub Releases から署名済みアップデートを確認・インストール

## インストール

[GitHub Releases](https://github.com/amberpepper/mcp-link/releases) からデスクトップインストーラーまたは Server バイナリをダウンロードしてください。

## 開発

### 必要環境

- Node.js 22 以降
- pnpm 10
- Rust stable
- 使用する OS の [Tauri 前提条件](https://v2.tauri.app/start/prerequisites/)

### クローンと依存関係のインストール

```bash
git clone https://github.com/amberpepper/mcp-link.git
cd mcp-link
pnpm install
```

### デスクトップアプリの起動

```bash
pnpm dev:desktop
```

Rust のコンパイルとデスクトップパッケージングは、各 OS のネイティブ環境で実行してください。Windows では WSL ではなく PowerShell またはコマンドプロンプトを使用します。

### Server モードの起動

```bash
pnpm dev:server
```

<http://127.0.0.1:3284> を開きます。既定の管理パスワードは `admin` です。初回ログイン後に設定画面から変更してください。

待受アドレスを変更する場合：

```bash
MCP_LINK_HTTP_ADDR=0.0.0.0:3284 pnpm dev:server
```

PowerShell：

```powershell
$env:MCP_LINK_HTTP_ADDR = "0.0.0.0:3284"
pnpm dev:server
```

### Web フロントエンドのみ起動

```bash
pnpm dev:web
```

### 本番ビルド

```bash
pnpm build:desktop
pnpm build:server
```

GitHub Actions は Windows、macOS、Linux 向けの Desktop と Server をビルドします。`v1.0.0` のようなバージョンタグをプッシュすると GitHub Release が自動作成されます。

## Docker

ヘッドレス Server 版をビルドして実行します：

```bash
docker build -t mcp-link:latest .
docker volume create mcp-link-data
docker run --rm \
  -p 3284:3284 \
  -v mcp-link-data:/app \
  --name mcp-link \
  mcp-link:latest
```

<http://localhost:3284> を開き、既定のパスワード `admin` でログインしてください。詳細は [Docker デプロイ](docs/DOCKER.md) を参照してください。

## プライバシーとセキュリティ

設定、認証情報、ログ、サーバーデータはローカルに保存されます。Server モードをネットワークへ公開する場合は、既定のパスワードを変更し、必要に応じてネットワークアクセスを制限してください。

## リポジトリ

<https://github.com/amberpepper/mcp-link>

## ライセンス

このプロジェクトは Sustainable Use License の下でライセンスされています。詳細は [LICENSE.md](LICENSE.md) を参照してください。
