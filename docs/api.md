# APIとCLI

APIのベースパスは `/api/v1`。`Authorization: Bearer <KANSO_TOKEN>` が全エンドポイントに必要です。トークンは単一ライブラリ全体の管理権限を持ちます。エラーは `{ "error": "..." }`。クエリ解析やJSON解析のエラーはAxum標準の応答となる場合があります。

| Method | Path | 処理 |
|---|---|---|
| GET | `/stats` | フィード・記事・未読・スター件数 |
| GET / POST | `/feeds` | 購読一覧 / 追加 |
| PATCH / DELETE | `/feeds/{id}` | 表示名・フォルダ更新 / 記事を含む削除 |
| POST | `/feeds/{id}/refresh` | 1フィードを取得。完了まで最大25秒 |
| GET | `/articles` | プレビュー一覧 |
| GET / PATCH | `/articles/{id}` | 全文 / 既読・スター更新 |
| POST | `/mark-read` | フィード・フォルダ範囲を既読化 |
| POST / GET | `/refresh` | 全件更新を開始 (202) / 進捗 |
| POST / GET | `/opml` | XMLをインポート / エクスポート |

購読追加:

```json
{"url":"https://blog.rust-lang.org/feed.xml","title":"Rust Blog","folder":"Tech"}
```

PATCH `/feeds/{id}` は `title` と `folder` を両方指定します。
PATCH `/articles/{id}` は `{"read":true}`、`{"starred":true}` または両方を指定します。
POST `/mark-read` は `{"feed_id":null,"folder":null}` で全体を、指定がある場合はその積集合を対象にします。検索・スター状態による絞り込みは扱いません。

GET `/articles` のクエリ: `feed_id`、`folder`、`unread=true|false`、`starred=true|false`、`q`、`limit` (1–200、既定50)、`offset`。新しい公開日順、同一公開日はID順です。更新中のページングはスナップショットではありません。`content` は一覧では400文字のプレビュー、個別取得では全文です。HTMLを含む信頼できない入力なので、API利用側でも無加工のHTML挿入を行わないでください。

OPML POSTは `Content-Type: application/xml` とUTF-8本文を渡します。重複購読は追加せず、全件を一つのトランザクションで処理します。フィードURLと記事リンクは別のものです。購読登録だけでは記事取得を行わないため、`/refresh` または定期更新で取得してください。

更新進捗:

```json
{"running":false,"processed":3,"total":3,"added":12,"errors":0}
```

主なステータス: 201追加、202ジョブ開始、204更新・削除成功、400入力・操作失敗、401未認証、404未存在、409更新競合、413本文超過、429同時実行超過、502取得失敗。CLIは失敗時に非ゼロ終了し、成功結果をJSONまたはOPMLとして標準出力します。

`/healthz` は認証不要の生存確認で、DBの整合性や外部フィードへの到達性を保証しません。外部から検査する場合はHTTPSの `/healthz` と認証付き `/api/v1/stats` を組み合わせてください。
