# 本番運用

本書は個人用の単一ライブラリを1つのサーバーで運用する構成を扱います。トークンを渡した端末は同じライブラリを管理できます。一般向けの複数ユーザーサービスをこの構成で公開しないでください。

## 初期構成

LinuxホストにDocker EngineとComposeを用意し、ドメインのA/AAAAレコードをホストに向けます。外部へ公開するポートは80/443と管理用SSHだけです。APIの8080番はComposeネットワーク内だけに置きます。Caddyが証明書取得・更新とHTTPS終端を担当します。

```bash
cp deploy/.env.example deploy/.env
# deploy/.env に生成トークン、実ドメイン、ACMEメールを設定
chmod 600 deploy/.env
docker compose --env-file deploy/.env -f deploy/compose.yml up -d --build --wait
```

`KANSO_TOKEN` は `kanso-server token` の256ビット乱数を使い、秘密管理システムに保管します。設定ファイル、CIのログ、コマンドライン引数、Gitに載せないでください。更新するには新しいトークンに設定してサーバーを再起動し、各端末を再接続します。旧トークンは使えなくなります。

サーバーコンテナはUID/GID 10001、読み取り専用ルート、権限削減、メモリ512 MiB、CPU 1、プロセス128の制限を持ちます。SQLiteのデータは `kanso_data` ボリューム、TLS証明書はCaddyの永続ボリュームに保存します。OSのディスク暗号化とバックアップの暗号化はホスト側で行います。

フィード取得は検証したIPへの直接接続を使います。企業プロキシや環境変数のHTTPプロキシを介して取得する構成には対応しません。サーバーから公開DNSと公開HTTP(S)への通信を許可してください。ホスト側でも私設ネットワーク・メタデータ宛のegress拒否を設定すると検証境界を強化できます。

## 監視

`/healthz` はHTTPの生存確認です。認証付き `/api/v1/stats` でDB読取りも確認します。外部監視でTLS証明書期限、HTTP失敗率、CPU・メモリ、ディスク空き、フィードの `last_fetched` と `last_error`、更新ジョブの進捗、バックアップの作成時刻を確認します。トークン付きURLは使わず、監視システムの秘密ヘッダー設定を使ってください。

```bash
docker compose --env-file deploy/.env -f deploy/compose.yml ps
docker compose --env-file deploy/.env -f deploy/compose.yml logs --tail=100 reader
```

ログはローテーションされます。APIトークンやフィードURLの秘密クエリをログに含めません。500フィード、100,000記事の上限と本文100,000文字の切り詰めがあります。大量の長い記事がある場合はディスク使用量が大きくなるので、実データ量に応じた容量監視が必要です。負荷試験と容量計画は配布先のフィード数・記事長・同時接続に合わせて実施します。

## バックアップと復元

稼働中の `.sqlite` だけをコピーするとWAL内の更新を落とすため、SQLite backup APIを使います。毎日バックアップし、7日の日次・4週の週次など必要な保持期間をホスト側ジョブで設定します。別ホストまたは暗号化オブジェクトストレージにも複製し、定期的に復元を検証してください。

```bash
docker compose --env-file deploy/.env -f deploy/compose.yml exec -T reader \
  kanso-server backup --output /app/data/backups/daily-2026-10-07.sqlite
```

出力先は新しいファイルに限定し、整合性を検査してから成功を返します。同じファイル名で再実行すると失敗します。ネイティブのDBはOSのapp data directoryにあります。サーバーのDBは環境変数 `KANSO_DATABASE` で指定できます。

ホスト上のSQLiteファイルを扱う場合:

```bash
python3 scripts/backup.py backup data/kanso.sqlite backups/daily.sqlite
python3 scripts/backup.py verify backups/daily.sqlite
# サーバーを停止してから、新しいパスへ復元する
python3 scripts/backup.py restore backups/daily.sqlite data/restored.sqlite
# KANSO_DATABASE を restored.sqlite に変更して起動・API検査
```

コンテナの復元はサービスを停止し、バックアップを検査したうえで新しいボリュームへ配置し、UID/GID 10001とファイル0600を設定します。復元先に旧WAL/SHMを持ち込まないでください。元のボリュームは検証終了まで保持します。

## CIからのデプロイ

`Deploy server` は `production` Environmentを使用します。GitHub側で承認者と許可するブランチを設定してください。Secretsは `DEPLOY_HOST`、`DEPLOY_USER`、`DEPLOY_SSH_KEY`、`DEPLOY_KNOWN_HOSTS`。ホスト鍵は管理者が確認した値を登録し、実行中の `ssh-keyscan` で置き換えません。

ホストの `/opt/kanso/repo` にリポジトリ、`/opt/kanso/.env` に秘密設定、`/opt/kanso/deploy.sh` に配布スクリプトを配置します。配布ユーザーにはこの構成を実行する権限を与えます。Dockerへのアクセスはホスト上の強い権限なので、配布専用のアカウントと限定したSSH鍵で管理してください。

デプロイはGitHubの実行リビジョンを正確に取得・ビルドし、既存DBをバックアップし、コンテナを更新して健康状態を待ちます。失敗時は旧イメージへ戻します。現行スキーマは1です。将来の不可逆DBマイグレーションではイメージだけのロールバックでは足りないため、互換性を確認し、必要なら停止してDBをバックアップから復元します。
