# リリースとプラットフォーム検証

## 検査と成果物

`Quality and security` はRust単体・APIテスト、Clippy、実サーバー/CLI、ブラウザ操作、バックアップ/復元、Cargoの脆弱性・ライセンス・取得元検査、npm監査を実行します。CI依存アクションはコミットSHAで固定し、Dependabotで更新します。Windows/macOSはTauriのネイティブビルドとコアテスト、iOSはデバイス向けRust staticlibとシミュレーター向けコンパイルを行います。iOSのRustビルド成功だけでは完成したIPAや実機動作を保証しません。

バージョンはCargo workspace、package.json、tauri.conf.jsonを揃えます。`scripts/check-version.py` で不一致とタグ不一致を拒否します。アプリの識別子 `app.kanso.reader` は配布前に自分が管理するBundle IDへ変更し、Apple DeveloperのApp IDと一致させてください。

`v0.1.0` などのタグから、品質ゲートが通ったサーバー/CLI (Windows、macOS ARM/Intel、Linux)、Webシェル、未署名デスクトップアプリをZIPにまとめます。依存ライセンス全文、説明書、SHA256SUMSを添付し、GitHubのドラフトリリースを作ります。未署名アプリのZIPには `UNSIGNED.txt` を含め、署名済みIPAは証明書のある別ワークフローで作成します。Webシェルは同じオリジンでAPIへアクセスするため、静的ファイルだけのホスティングではRSS取得・保存は動きません。

## Windows署名

`Signed Windows release` の `windows-release` EnvironmentへPFXのbase64を `WINDOWS_CERTIFICATE`、パスワードを `WINDOWS_CERTIFICATE_PASSWORD` として登録します。SigntoolでインストーラをSHA-256署名・タイムスタンプし、署名検証後に成果物をアップロードします。EV証明書やハードウェア/クラウド鍵を使う場合はPFX方式に代えて組織の署名サービスを接続してください。CIの署名設定は実際の証明書による初回検証が必要です。

## macOS / iOS署名

`Signed Apple release` の `macos-release` と `ios-release` Environmentを設定します。macOSとiOSの証明書をそれぞれのEnvironmentに分けて登録します。

| Secret | 用途 |
|---|---|
| APPLE_CERTIFICATE | 署名証明書P12のbase64 |
| APPLE_CERTIFICATE_PASSWORD | P12パスワード |
| APPLE_SIGNING_IDENTITY | 対象プラットフォームの署名ID |
| APPLE_ID / APPLE_PASSWORD | 公証用Apple ID・アプリ用パスワード |
| APPLE_TEAM_ID | 開発者チームID |
| APPLE_PROVISIONING_PROFILE | iOS配布プロファイルのbase64 |

macOSはUniversalアプリの署名・公証をTauriへ渡します。iOSはApple Distribution証明書と登録Bundle IDに合うプロファイルを用意し、XcodeのSigning設定を初回に検証します。macOSのDeveloper ID証明書とiOSのDistribution証明書は用途が異なるので、配布構成に合わせてEnvironmentまたはワークフローを分けて管理してください。

署名・IPA生成・App Store審査はApple側の資格情報と規約が必要です。ワークフローは成果物を作成するところまでを扱い、TestFlight / App Store Connectへのアップロードは配布管理者が実行します。Windows Storeへの登録も配布管理者のアカウントで行います。

## 公開前の確認

品質ゲートに加え、Windowsのインストール/削除とWebView2、macOS ARM/IntelのGatekeeper・公証、iOS実機の初回起動・バックグラウンド復帰・ファイル選択、HTTPSサーバー接続、トークン変更、OPML往復、保存データのアップグレードを確認します。長時間運転と実際のフィード集合での負荷・ディスク容量試験を行い、運用監視とバックアップ復元を確認してからドラフトを公開します。

この作業環境はLinuxです。作成済みのGitHubワークフローはリポジトリへ配置して初回実行する必要があります。プラットフォームビルド・証明書署名・実機検証の結果が未取得の状態を、公開準備完了として扱わないでください。
