# Kanso RSS

あなたの情報を、あなたのペースで。Rustで実装したRSS/Atomリーダーです。
Windows、macOS、iOSのアプリとWeb画面、JSON API、CLIを提供します。

## 機能

- フィード購読、フォルダ分類、表示名の編集、購読削除
- RSS/Atom取得、15分ごとの更新、更新状況と取得エラーの表示
- 未読・既読、スター、検索、ページング、キーボード操作（J / K / S / `/`）
- OPMLインポート・エクスポート
- ネイティブアプリの端末内SQLite保存、HTTPSサーバーへの接続
- Webのレスポンシブ表示とPWAシェル、ライト・ダークテーマ
- 認証付きAPIと、JSONを出力するRust CLI

APIサーバーは**個人用の単一ライブラリ**です。トークンを持つクライアントは同じデータへの全権限を持ちます。複数アカウントのデータ分離や一般公開の新規登録機能は実装していません。端末内データとサーバーデータの自動マージは行わず、接続先のライブラリを表示します。移行にはOPMLを使えます。記事は安全なテキストとして表示します。

## 起動

Rust 1.90.0、Node.js 24、Python 3.11以降を使用します。Rustのコア・API・CLIはNode.jsなしでビルドできます。

```bash
cargo build --locked -p kanso-server -p kanso-cli
export KANSO_TOKEN="$(./target/debug/kanso-server token)"
./target/debug/kanso-server
```

`http://127.0.0.1:8080` を開き、生成したトークンを入力します。トークンは画面を開いている間だけ保持します。サーバーは `data/kanso.sqlite` に保存します。トークンの表示・保管は使用する秘密管理ツールで行ってください。

PowerShell:

```powershell
cargo build --locked -p kanso-server -p kanso-cli
$env:KANSO_TOKEN = & .\target\debug\kanso-server.exe token
.\target\debug\kanso-server.exe
```

環境変数: `KANSO_TOKEN`、`KANSO_DATABASE`、`KANSO_LISTEN`。
既定はループバックのみです。外部HTTP待受は明示指定とTLSリバースプロキシが必要です。

## CLI

```bash
./target/debug/kanso add https://blog.rust-lang.org/feed.xml --title "Rust Blog" --folder "Tech"
./target/debug/kanso refresh --wait
./target/debug/kanso feeds
./target/debug/kanso articles --unread --search Rust --limit 50
./target/debug/kanso article ARTICLE_ID
./target/debug/kanso read ARTICLE_ID
./target/debug/kanso star ARTICLE_ID
./target/debug/kanso export --output subscriptions.opml
./target/debug/kanso import subscriptions.opml
```

別サーバーへは `KANSO_API_URL=https://rss.example.com` を指定します。
リモート接続はHTTPS必須です。トークンはプロセス一覧へ露出する引数にせず環境変数で渡します。
[API仕様](docs/api.md)を参照してください。

## ネイティブアプリ

```bash
npm ci --ignore-scripts
cd apps/native
npx tauri dev
npx tauri build --ci -- --locked
```

WindowsではMSVC Build ToolsとWebView2、macOSではXcode Command Line Toolsが必要です。macOSのIntel/Apple Siliconを対象にしています。iOSではmacOSとXcode、Apple Developerの署名設定が必要です。

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
cd apps/native
npx tauri ios init --ci
npx tauri ios dev
```

アプリは初回起動から端末内ライブラリを利用できます。設定でHTTPSサーバーとトークンを指定すると同じサーバーのライブラリを使えます。iOSのバックグラウンド中はOSによって処理が停止されるため、定期取得は常時稼働するサーバーで行ってください。ネイティブのOPML出力はコピーに対応し、Webではファイル保存にも対応します。

## 本番運用・リリース

[運用手順](docs/operations.md)、[リリースと署名](docs/releasing.md)、[設計](docs/architecture.md)、[セキュリティ検証](docs/security-review.md)を参照してください。

CIはRustのテスト・Clippy、実サーバーとCLIの接続検証、ブラウザ操作とXSSの検証、バックアップ復元、依存脆弱性・ライセンス検査を行います。Windows/macOSのネイティブビルド、iOSのRustライブラリビルドも対象です。バージョンタグからチェックサム付きのドラフトリリースを作成し、Apple/Windowsの署名ワークフローを別途用意しています。

署名用の証明書、Appleのプロビジョニング、配布先のアカウント、GitHub Environmentsとデプロイ先を設定し、各プラットフォームで初回ビルドと実機検証を完了してから公開してください。環境固有の未検証項目は[検証記録](docs/security-review.md)に記載しています。

## ライセンス

本体は[MIT](LICENSE)。直接・間接依存のバージョン、SPDXライセンス、全文は[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)と[licenses/](licenses/)に含まれます。画面にもライセンス案内を用意しています。取得した記事の権利は各権利者に帰属します。

```bash
cargo fetch --locked
npm ci --ignore-scripts
python3 scripts/licenses.py
```

依存更新で上流のライセンスファイルが不足した場合は、`scripts/fetch-license-fallbacks.py` で正確な上流リビジョンから補完し、ソースURLとハッシュをレビューしてから保存します。不明なライセンスや全文の欠落はリリースを失敗させます。
