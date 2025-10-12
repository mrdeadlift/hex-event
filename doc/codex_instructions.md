# 開発タスク（Codex系コーディングエージェント向け）

## 目的
LoLクライアントの Live Client Data API + LCU WS を統合し、**高速・軽量（Rust単一バイナリ）かつ扱いやすいSDK（TS/Pythonバインディング）**を提供する。

## アウトカム
- levents（Rustコア）
- @levents/sdk（TS）
- levents-py（Py）

## 0. プロジェクト初期化
- モノレポ作成: `levents/`（Rust workspace）+ `bindings/ts/` + `bindings/py/`
- CI: GitHub Actions（Rust / Node / Py のビルド・テスト・lint）
- ライセンス & コード規約: MIT, Conventional Commits, rustfmt, clippy, eslint, black

**Agent Prompt（例）**

Create a new monorepo with a Rust workspace levents/ and two bindings: bindings/ts (TypeScript, N-API) and bindings/py (Python, gRPC client). Add GitHub Actions to build/test all packages on push.

## 1. Rust コア（高速・軽量デーモン）
### 1.1 クレート構成
- `levents-core`: Live Client/LCU クライアント、イベント正規化
- `levents-daemon`: CLI/常駐、IPC（gRPC または N-API/FFI）
- `levents-model`: 共通型（Serdeモデル、Event型）

**Acceptance**
- `cargo build` で全crateビルド通過
- `cargo test` でユニットテスト通過

**Agent Prompt**

Scaffold Rust workspace with crates levents-core, levents-daemon, levents-model. Set up tokio, reqwest/hyper, serde, tracing.

### 1.2 Live Client Data API（Port 2999）
- `/liveclientdata/activeplayer`, `/players`, `/eventdata` を取得
- 差分検知: xxhash で shallow-hash、必要時のみ deep diff
- 適応ポーリング: 戦闘中 100–200ms / 通常 500–1000ms / idle 1500ms
- デバウンス/集約: 近接イベントのまとめ発火

**Acceptance**
- `levents watch` 実行時に CPU < 2–3%、常時メモリ < 30MB（目安）
- `onKill`/`onDeath`/`onLevelUp`/`onItemAdded` が 1試合で正しく発火ログ

**Agent Prompt**

Implement a Live Client poller with adaptive intervals and hashing-based diff. Expose a stream of normalized events.

### 1.3 LCU（Lockfile検出 + HTTPS + WebSocket）
- Lockfile 読み取り（ポート/トークン）
- HTTPS クライアント（自己署名許可、127.0.0.1限定）
- WS購読：ドラフト/ロビー/フェーズ変更（例：`/lol-champ-select/v1/session`）
- 自動再接続・バックオフ

**Acceptance**
- LCU接続断→自動復帰
- `phaseChange` イベントを受け取り stream へ流せる

**Agent Prompt**

Implement LCU connector (lockfile discovery, HTTPS/WS auth) with auto-reconnect and expose phase change events.

### 1.4 イベント正規化レイヤ
- 高レベルイベント：Kill, Death, Assist, LevelUp, ItemAdded/Removed, GoldDelta, Respawn, PhaseChange
- 型：`Event { kind, ts, payload }` + payload 専用型（Ability/Item/PlayerRef 等）
- 重複抑制・順序保証（timestamp / monotonic id）

**Acceptance**
- 同一イベントの二重発火が ≦ 1%（ベンチ中）
- 型が TS/Py バインディングと一致（JSON schema 生成）

**Agent Prompt**

Implement event normalization and deduplication. Generate JSON schema from Rust types.

## 2. IPC / API 仕様
### 2.1 API 方式決定
- gRPC（推奨）: 言語間共通／ストリームに強い
- `proto/events.proto`: `Subscribe(SubscribeRequest)` returns `(stream Event)`
- Control RPC（設定変更：interval、filters）
- （代替）N-API 直結（TSのみ強いがPy拡張が重くなる）

**Acceptance**
- gRPCサーバが `levents-daemon` で起動
- CLI `levents watch --grpc` でイベント表示

**Agent Prompt**

Add a gRPC server to levents-daemon that exposes Subscribe/Control RPCs, streaming normalized Event messages.

## 3. TypeScript バインディング（@levents/sdk）
### 3.1 パッケージ骨子
- `createClient({ live?: {enabled, intervalBaseMs}, lcu?: {enabled} })`
- `on('kill'|'death'|..., handler)` / `off()`
- 型定義：`types.ts`（proto 由来）

**Acceptance**
- `npm pack` / `pnpm build` 成功
- Node 20+ でサンプルが動作（イベント受信）

**Agent Prompt**

Implement a TS client for the gRPC API with on(event, handler) subscription API and TypeScript types.

### 3.2 サンプル & DX
- `examples/node/basic.ts`（ログ出し）
- `examples/node/overlay.ts`（TUI/簡易HUD）
- エラーメッセージ改善（LCU未起動、2999未開放）

**Acceptance**
- `pnpm example:basic` でイベント表示
- 失敗時に actionable なエラーメッセージ

## 4. Python バインディング（levents-py）
- 依存：grpcio, pydantic, typer
- API：`subscribe(event:str, handler:Callable)`
- CLI：`python -m levents watch`

**Acceptance**
- `pip install -e .` → サンプルが動作
- 型検証（pydantic）パス

**Agent Prompt**

Implement a Python client wrapping the gRPC API, provide subscribe() and a small CLI using typer.

## 5. 観測性・最適化
- `--metrics`（CPU/メモリ/loop-lag、Prometheus `/metrics`）
- `--trace`（イベント頻度/遅延の計測）
- ベンチ：リアル試合30分でプロファイル収集

**Acceptance**
- Prometheus 互換の `/metrics` を expose
- ベンチ結果を `docs/perf.md` に記録

**Agent Prompt**

Add a Prometheus metrics endpoint to the daemon and instrument key paths (poller, diff, WS). Provide a perf doc.

## 6. セキュリティ / 耐障害
- 127.0.0.1 のみバインド、認証トークン（ローカルIPC用）
- LCU/2999 へのリトライ（指数バックオフ）
- 例外時のソフトデグレード（ライブ非対応なら LCU のみ等）

**Acceptance**
- 依存先ダウン時もプロセスが落ちない
- 脅威モデルと制約を `SECURITY.md` に記載

## 7. ドキュメント & 配布
- `README.md`: 5分で動く（Quick Start）
- APIリファレンス（TS/Py）
- 配布：GitHub Releases（Win/macOS/Linux 向け単一バイナリ）
- 変更履歴：`CHANGELOG.md`

**Acceptance**
- 新規開発者が README の手順だけで実行可能
- リリースアーティファクト生成（Actions）

**Agent Prompt**

Write comprehensive README and generate release artifacts automatically on tags.

## 8. テスト計画
- ゴールデンテスト：RiotサンプルJSONからイベント期待値照合
- 耐久テスト：長時間ストリームでリーク無し
- 統合テスト：ダミーLive/LCUサーバでE2E

**Acceptance**
- `cargo test`, `pnpm test`, `pytest` すべて緑
- E2Eで `onKill`/`LevelUp` 系の期待イベント合致

## 9. ロードマップ（短期→中期）
- v0.1.0：Live/LCU最小イベント + TS SDK
- v0.2.0：Py SDK + metrics + CLI
- v0.3.0：プロファイル最適化・HUDサンプル
- v0.4.0：フィルタ/購読条件、録画（ローカルJSONログ）

## 10. 参考・比較（同様アプリ）
- lcu-driver (Python)：LCU接続に強い（WS/管理込み）
- hexgate (TypeScript)：型安全な LCU ラッパ
- LiveEvents（観戦専用API）：無効化報告あり→非依存方針
- Live Client Data API：本作の対戦中データ源（ポーリング）
