Kanso RSS v0.1.0 は、個人用のRSS/Atomリーダーの初回リリースです。

- フィード購読・フォルダ、定期取得、未読・既読、スター、検索、OPML入出力
- Rustの認証付きJSON APIとCLI、レスポンシブWeb画面、端末内SQLite保存
- Windows、macOS Apple Silicon / Intel のデスクトップアプリ
- 依存ライセンス全文、SHA256SUMS、バックアップ・復元と運用手順

配布ZIPにはAPIサーバー/CLI、Web画面、**未署名**のデスクトップアプリを含みます。Windows署名・macOS公証は証明書を設定した署名ワークフローで行ってください。iOSの実装とコンパイル検査は含まれますが、署名済みIPAはこのリリースに含みません。

サーバーは単一ライブラリで、トークンを持つクライアントが全データを共有します。HTTPSの背後へ配置し、実際のフィードでの長時間運転、バックアップ復元、対象OSでのインストール・実機動作を確認してから本番公開してください。正式なCodex Securityスキャンの結果は未取得です。詳細は `docs/security-review.md` と `docs/operations.md` を参照してください。
