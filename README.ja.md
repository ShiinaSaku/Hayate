<div align="center">

<img src="./assets/logo.svg" alt="Hayate" width="120" height="120" />

# Hayate（疾風）

**暗号化・圧縮対応の LAN ファイル転送 — バイナリひとつ、クラウドもアカウントも SSH も不要。**

QUIC トランスポート · X25519 + Argon2id + AEAD · 4 語ペアリングフレーズ · ローカル Web ダッシュボード

[![Website](https://img.shields.io/badge/website-hayate.shiina.xyz-6ea8fe?style=flat-square)](https://hayate.shiina.xyz)
[![CI](https://img.shields.io/github/actions/workflow/status/ShiinaSaku/Hayate/ci.yml?style=flat-square&label=CI)](https://github.com/ShiinaSaku/Hayate/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/hayate?style=flat-square&color=e37602&label=crates.io)](https://crates.io/crates/hayate)
[![npm](https://npmx.dev/api/registry/badge/version/@shiinasaku/hayate)](https://npmx.dev/package/@shiinasaku/hayate)
[![docs.rs](https://img.shields.io/docsrs/hayate?style=flat-square&color=3fb950&label=docs.rs)](https://docs.rs/hayate)
[![Rust 1.96+](https://img.shields.io/badge/rust-1.96%2B-dea584?style=flat-square&logo=rust)](Cargo.toml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](LICENSE)

[English](./README.md) · **日本語**

</div>

```console
$ hayate send ./photos.zip

   ●  Pairing code
   forest-river-mango-silver-orbit

   Receiver runs:  hayate receive --code "forest-river-mango-silver-orbit"
```

両方のマシンでコマンドは 1 回ずつ。ファイルは暗号化・圧縮され、整合性を検証しながら
ローカルネットワークを直接流れます。

<div align="center">
<img src="./assets/dashboard.png" alt="Hayate Web ダッシュボード" width="100%" />
<br />
<sub><code>hayate web</code> — トークン認証付き・ループバック限定のダッシュボード。ピアと履歴をリアルタイム表示。</sub>
</div>

## 目次

- [Hayate を選ぶ理由](#hayate-を選ぶ理由)
- [機能](#機能)
- [インストール](#インストール)
- [クイックスタート](#クイックスタート)
- [コマンドリファレンス](#コマンドリファレンス)
- [Web ダッシュボード](#web-ダッシュボード)
- [スクリプトと自動化](#スクリプトと自動化)
- [仕組み](#仕組み)
- [セキュリティモデル](#セキュリティモデル)
- [パフォーマンス](#パフォーマンス)
- [比較](#比較)
- [ライブラリとして使う](#ライブラリとして使う)
- [開発](#開発)
- [リリース](#リリース)

---

## Hayate を選ぶ理由

- **インフラ不要。** リレーもランデブーサーバーも SSH デーモンもありません。同じネットワーク上の
  2 台だけで十分です。
- **フレーズで認証。** 4 語のコード（約 41 ビット）をメモリハードな Argon2id で伸長し、
  鍵交換に混ぜ込みます。フレーズを知るピアだけがセッション鍵を導出できます。
- **必要なところで速い。** QUIC、4 MiB フレーム、非同期先読み、そして zstd と AEAD を
  専用ワーカースレッドで実行します。
- **再開できて、検証できる。** 中断したファイルは最後のフレーム境界から再開します。
  チェックサムは受信済みの先頭部分を含むファイル全体を対象にします。
- **中身を確認できる。** ローカルダッシュボードがピアと履歴を表示します。データはどこにも送信しません。

## 機能

| | |
| --- | --- |
| **トランスポート** | QUIC（compio: io_uring / IOCP / kqueue）、4 MiB フレーム、8 段先読み、48 MB フローウィンドウ |
| **暗号化** | 一時 X25519 ECDH、Argon2id によるフレーズ伸長、HKDF-SHA256、フレーム単位の AES-256-GCM または ChaCha20-Poly1305 |
| **ペアリング** | 4 語フレーズ、mDNS + UDP 探索、`ip:port` 直接指定、任意の `--passphrase` |
| **整合性** | 平文に対する BLAKE3（デフォルト）または SHA-256。サイズとフレームを検証 |
| **再開** | 単一ファイルは最後の 4 MiB 境界から再開。完了済みファイルは再検証 |
| **圧縮** | zstd レベル 1。圧縮済み形式はスキップし、非圧縮性データでは自動的に無効化 |
| **制御** | `--bandwidth-limit`、保存済みピア、対話ピッカー（`send --pick`）、上書きポリシー |
| **ダッシュボード** | `hayate web`: ドラッグ＆ドロップ送信、受信リスナー、進捗、履歴、トークン認証 |
| **スクリプト対応** | NDJSON イベント（`schema: hayate/1`）、安定した終了コード、`--quiet` / `--verbose` |
| **プラットフォーム** | macOS・Linux・Windows 向け静的バイナリ（Android/Termux はソースからビルド） |

内部では、完了ベースの非同期ランタイム [compio](https://github.com/compio-rs/compio)
（io_uring / IOCP / kqueue）上で動作します。現代の高スループットサーバーと同じクラスの
カーネルプリミティブです。

---

## インストール

### シェルインストーラ

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/ShiinaSaku/Hayate/releases/latest/download/hayate-cli-installer.sh | sh
```

Windows では同じリリースの `hayate-cli-installer.ps1` を実行してください。

### npm

```bash
npm install -g @shiinasaku/hayate
# インストールせずに実行する場合:
npx @shiinasaku/hayate --help
```

インストール時に macOS・Linux・Windows（x64 / arm64）向けのビルド済みバイナリを取得します。

### GitHub Releases

ビルド済みアーカイブ（`.tar.xz` / `.zip`）、インストーラ、`sha256.sum`、ビルド provenance
attestation を公開しています: [最新リリース](https://github.com/ShiinaSaku/Hayate/releases)。

### ソースからビルド

```bash
git clone https://github.com/ShiinaSaku/Hayate.git
cd Hayate
mise run build          # または: cargo build --release -p hayate-cli
./target/release/hayate --help
```

**Rust 1.96**（edition 2024）が必要です。ダッシュボードのバンドルはコミット済みのため、
CLI のビルドに JavaScript ツールチェーンは不要です。

### ライブラリ

```bash
cargo add hayate
```

転送エンジンは[ライブラリクレート](https://docs.rs/hayate)としても提供しています。
独自のインターフェースをその上に構築できます。

---

## クイックスタート

### ペアリングモード — IP アドレス不要

```bash
# 送信側: 一度きりのフレーズを表示して待機
hayate send ./photos.zip

# 受信側: 同じフレーズで参加
hayate receive --code "forest-river-mango-silver-orbit"
```

フレーズが違えば AEAD 認証に失敗し、転送は中止されます。

### 直接モード — アドレスが分かっている場合

```bash
hayate receive --port 50001 --output ./downloads
hayate send ./archive.tar.gz 192.168.1.50:50001
```

直接転送も認証したい場合は、**両側**で `--passphrase <secret>` を指定します。
シークレットが一致しなければ、ペイロードが流れる前のハンドシェイクで中止されます。

```bash
hayate receive --port 50001 --output ./downloads --passphrase "correct horse battery staple"
hayate send ./archive.tar.gz 192.168.1.50:50001 --passphrase "correct horse battery staple"
```

### ディレクトリ

ディレクトリは tar アーカイブとしてストリームされ、その場で圧縮・暗号化されます。
展開時には絶対パス、`..` 成分、シンボリックリンクによる脱出、ハードリンクのトラバーサルを
拒否します。

```bash
hayate send ./my-project 192.168.1.50:50001
```

### 受信側を探す

```bash
hayate discover                       # mDNS + UDP クエリ（通常 ~3 秒）
hayate discover --scan                # サブネット全ホストもスキャン
hayate discover --cidr 10.0.0.0/16    # 明示的なネットワークをスキャン
```

受信側は mDNS で広告し、UDP クエリにも応答するため、通常の LAN では即座に見つかります。
サブネットスキャンは 4096 ホストまでに制限され、インターフェースのネットマスク
（`/22`、`/16` など）を尊重します。

---

## コマンドリファレンス

### `hayate send [PATH] [TARGET]`

| フラグ | 説明 | デフォルト |
| --- | --- | --- |
| `PATH` | 送信するファイルまたはディレクトリ | 対話ピッカー |
| `TARGET` | 受信側の `ip:port` または `hostname:port` | ペアリングモード |
| `--to <NAME>` | 保存済みピアへ送信 | — |
| `--pick` | LAN をスキャンして受信側を選択 | オフ |
| `--code <phrase>` | ペアリングフレーズ | 未ペアリング時は自動生成 |
| `--passphrase <secret>` | 直接転送用の帯域外シークレット | なし |
| `-z, --compress[=<bool>]` | zstd 圧縮 | オン |
| `--no-compress` | 圧縮を無効化 | オフ |
| `--hash <algo>` | `blake3` または `sha256` | `blake3` |
| `--bandwidth-limit <RATE>` | 帯域制限（`10MiB`、`500KiB`、`2M`） | 無制限 |
| `--no-progress` | プログレスバーを非表示 | オフ |

### `hayate receive`

| フラグ | 説明 | デフォルト |
| --- | --- | --- |
| `-b, --bind <addr>` | バインドアドレス | `0.0.0.0`（`HAYATE_BIND`） |
| `-p, --port <port>` | 待受ポート | `50001`（`HAYATE_PORT`） |
| `-o, --output <dir>` | 保存先ディレクトリ（なければ作成） | `.` |
| `--code <phrase>` | ペアリングセッションに参加 | なし |
| `--passphrase <secret>` | 直接転送の送信者を認証 | なし |
| `--auto-accept` | 承認プロンプトを省略 | オフ |
| `--resume` | 中断した単一ファイル転送を再開 | オフ |
| `--overwrite` | 既存の保存先ファイルを置き換え | プロンプト / 拒否 |
| `--no-overwrite` | 保存先が存在する転送を拒否 | オフ |
| `--once` | 最初の転送（または拒否）で終了 | オフ |
| `--no-progress` | プログレスバーを非表示 | オフ |

### `hayate discover`

| フラグ | 説明 | デフォルト |
| --- | --- | --- |
| `-t, --timeout <secs>` | 探索 / スキャンのタイムアウト | `15` |
| `--scan` | サブネット全ホストもスキャン | オフ |
| `--cidr <cidr>` | 明示的なネットワークをスキャン | 自動 |

### `hayate web`

| フラグ | 説明 | デフォルト |
| --- | --- | --- |
| `--bind <ip>` | バインドするインターフェース | `127.0.0.1` |
| `--port <port>` | ダッシュボードのポート | `8787` |
| `--allow-lan` | ループバック外に公開（非推奨） | オフ |

### ピア、履歴、ドキュメント

```bash
hayate peers add laptop 192.168.1.20:50001
hayate send ./movie.mkv --to laptop
hayate peers list
hayate history -n 10 --format json
hayate docs            # ターミナル内ハンドブック
hayate man             # roff man ページ（隠しコマンド）
```

### グローバルフラグ

| フラグ | 説明 |
| --- | --- |
| `--color <auto\|always\|never>` | カラー出力のポリシー |
| `--format <pretty\|plain\|json>` | 人間向け UI、プレーンテキスト、NDJSON イベント |
| `-q, --quiet` | 出力を減らす（繰り返し可） |
| `-v, --verbose` | 詳細出力（繰り返し可） |

`hayate docs` は `send`、`receive`、`discover`、`web`、`security`、終了コードなどの
完全なリファレンスをターミナルに表示します。

---

## Web ダッシュボード

```bash
hayate web                 # http://127.0.0.1:8787/?t=<token> を表示
hayate web --port 9000
```

ダッシュボードは意図的に小さく、ローカル限定です。

- ライブ状態は **Server-Sent Events**（ピア、転送、受信状態）で届きます。
- すべての処理はこのマシン内で完結します。ダッシュボードが話すのはローカルの Hayate エンジンだけです。

UI は SvelteKit SPA（Tailwind + Tabler アイコン、独自デザインシステム）として `web/build` に
ビルドされ、バイナリに埋め込まれます。`hayate web` に実行時のアセット依存はありません。
転送は CLI と同じエンジンを通るため、ブラウザはもうひとつのフロントエンドにすぎません。

---

## スクリプトと自動化

`--format json` は stdout に 1 行 1 イベントの NDJSON を出力し、`schema: hayate/1` で
タグ付けします:

```bash
hayate send ./build.tar.zst --format json \
  | jq -r 'select(.event == "transfer_complete") | .speed_bps'
```

終了コードは安定しており、文書化されています（`hayate docs exit`）:

| コード | 意味 |
| ---: | --- |
| 0 | 成功 |
| 1 | 一般的なランタイム / 転送エラー |
| 2 | 使い方 / 引数のエラー |
| 3 | 受信側が転送を拒否 |
| 4 | プロトコルバージョン不一致 |
| 5 | ペアリングパスフレーズが無効 |
| 6 | タイムアウト |
| 7 | ユーザーによるキャンセル |
| 130 | 中断（Ctrl+C / Esc） |

---

## 仕組み

```text
Disk → [8 async reads × 4 MiB] → [hasher thread] → [workers: zstd + AEAD] → [QUIC window] → Wire
                                                                                  ↓
                                    receiver: decrypt → reorder → hash → sequential write
```

1. **接続。** 送信側は一時的な自己署名証明書で QUIC 接続を張ります（trust-on-first-use）。
   実際の認証はアプリケーション層のハンドシェイクです。
2. **交換。** 双方が一時 X25519 鍵を提供し、受信側が暗号スイートを選択
   （ハードウェア AES があれば AES-256-GCM、なければ ChaCha20-Poly1305）。送信側が暗号化
   メタデータを送ります。
3. **同意。** 受信側が承認または拒否し、ファイル転送ならフレーム境界に揃えた再開オフセットを返します。
4. **ストリーム。** ファイルは先読みされ、専用スレッドでハッシュされ、適応的に圧縮され、
   フレームごとの一意なノンス（ホットループでの CSPRNG 呼び出しではなく、ワーカーごとの
   カウンタ）で AEAD 封印されて、順番に書き込まれます。ディレクトリは tar として流れます。
5. **検証。** 双方が再開済みの先頭部分を含む平文全体をハッシュし、`blake3$…` または
   `sha256$…` チェックサムを報告します。

---

## セキュリティモデル

| 層 | プリミティブ |
| --- | --- |
| 鍵合意 | 一時 X25519 ECDH |
| 鍵導出 | Argon2id で伸長したパスフレーズ + トランスクリプト束縛 HKDF-SHA256 |
| フレーム暗号化 | AES-256-GCM または ChaCha20-Poly1305、フレームごとに一意な 96 ビットノンス |
| 整合性 | 平文に対する BLAKE3 または SHA-256 + フレーム単位の AEAD |
| メタデータ | ファイル名とサイズは使用前に暗号化 |
| ディレクトリ安全性 | `..`、絶対パス、シンボリックリンク脱出、ハードリンクのトラバーサルを拒否 |
| ダッシュボード | ループバックバインド、起動ごとの 128 ビットトークン、`Host` 検証、厳格な CSP |

Hayate が **しない** こと:

- **探索は認証されません。** mDNS 広告と UDP 応答はヒントにすぎません。転送を認証するのは
  ハンドシェイクです。
- **`--passphrase` なしの直接モードは未認証です。** 経路上で QUIC を傍受できる相手は
  受信側になりすませます。信頼できないネットワークではペアリングか共有 `--passphrase` を
  使ってください。
- **インターネット越えの NAT 突破はしません。** Hayate は LAN ツールであり、リレーや
  ホールパンチングは使いません。

AEAD、HKDF、ハッシュは [ring](https://github.com/briansmith/ring) と
[BLAKE3](https://github.com/BLAKE3-team/BLAKE3)、パスフレーズ伸長は RustCrypto の
`argon2` クレートを使用しています。独自実装のプリミティブはありません。

---

## パフォーマンス

- **先読み** — 並行 `read_at` により、暗号処理の間もディスクを休ませません。
- **専用ハッシャー** — BLAKE3/SHA-256 は専用スレッドで実行され、数 MiB のハッシュが
  compio の完了ループを止めません。
- **有界パイプライン** — ペイロード段階のキューはすべて有界です。遅いディスクやネットワークは
  4 MiB バッファをメモリに溜めるのではなく、バックプレッシャーとして働きます。
- **ワーカースレッド** — 圧縮と AEAD はチャネルで接続された専用スレッドで実行され、
  非同期イベントループをブロックしません。
- **暗号スイート交渉** — ハードウェア AES があれば AES-256-GCM、なければ
  ChaCha20-Poly1305（`HAYATE_FORCE_CHACHA20` で強制可能）。
- **適応圧縮** — zstd レベル 1。圧縮済み拡張子はスキップし、非圧縮性データでは 16 MiB 後に
  自動的に無効化します。
- **順序付き書き込み** — 受信側はフレームを並べ替えてから書き込むため、ディスク I/O は
  常にシーケンシャルです。

自分のマシンで測る:

```bash
cargo run --release --example throughput -- ./large.bin 3
```

スループットは OS の UDP GSO/GRO 対応に大きく左右されます。Linux は両方に対応し、
macOS は現状未対応のため、同じパイプラインでも 1 秒あたりのパケット数が大きく異なります。

### Linux のソケットバッファ

負荷時に転送が停滞する場合は、カーネルの UDP バッファ上限を引き上げてください:

```bash
sudo sysctl -w net.core.rmem_max=134217728
sudo sysctl -w net.core.wmem_max=134217728
```

macOS と Windows は UDP バッファを自動で調整します。

---

## 比較

| ツール | トランスポート | 探索 | 暗号化 | サーバー要否 | Web UI |
| --- | --- | --- | --- | --- | --- |
| scp / rsync | TCP / SSH | 手動 IP | SSH | sshd | なし |
| Magic Wormhole | TCP / TLS | ランデブーサーバー | PAKE | 必要（公開リレー） | なし |
| LocalSend | HTTP / HTTPS | mDNS | TLS | 不要 | なし |
| croc | TCP リレー | コードフレーズ | PAKE | リレー任意 | なし |
| sendme | iroh QUIC | チケット / iroh DNS | TLS | iroh リレーにフォールバック | なし |
| **Hayate** | **QUIC（compio）** | **mDNS + UDP クエリ** | **X25519 + Argon2id + AEAD** | **不要** | **ローカルダッシュボード** |

`sendme` が最も近い比較対象です。iroh ベースのためインターネット越えの NAT 突破に強い一方、
Hayate はあえてインフラを持たず LAN に特化しています。リレー / DNS の往復がなく、圧縮でき、
長いチケットではなく短いコードでペアリングできます。

---

## ライブラリとして使う

```toml
[dependencies]
hayate = "7"
compio = { version = "0.19", features = ["macros", "runtime", "fs", "net", "time"] }
```

```rust
use hayate::HayateSender;

#[compio::main]
async fn main() -> Result<(), hayate::EngineError> {
    let checksum = HayateSender::new()
        .code("forest-river-mango-silver-orbit".to_owned())
        .compress(true)
        .send("./file.txt", |bytes| {
            println!("sent {bytes} bytes");
            Ok(())
        })
        .await?;

    println!("done: {checksum}");
    Ok(())
}
```

公開 API が compio の型を公開することはありませんが、返される future は **compio** ランタイム上で
実行する必要があります。独自インターフェース向けには、ライフサイクルの全ステップを報告する
ステージド API があります: `HayateSender::send_with`、`HayateReceiver::receive_with`、
`ListeningReceiver`、`TransferStage`。ドキュメント: [docs.rs/hayate](https://docs.rs/hayate)。

---

## 開発

```bash
mise run check      # fmt（nightly rustfmt）+ clippy + tests + ダッシュボード型チェック
mise run test       # cargo test --workspace --all-targets
mise run web        # ダッシュボード変更後に web/build を再生成
mise run build      # このホスト向けリリースバイナリ
mise run android    # Termux 用バイナリ（aarch64-linux-android）
```

リポジトリ構成:

| パス | 内容 |
| --- | --- |
| `hayate/` | 転送エンジンライブラリ（crates.io で公開） |
| `hayate-cli/` | `hayate` バイナリ: clap、進捗 UI、ダッシュボードホスト |
| `web/` | SvelteKit + Tailwind のダッシュボード（独自デザインシステム、Bun + Vite） |
| `hayate-cli/src/web/` | 埋め込み HTTP/SSE サーバーと静的アセット配信 |

ダッシュボードは `web/build` にビルドされ、`include_dir!` でバイナリに埋め込まれます。
`web/build` はコミット済みなので、`cargo build` に JavaScript ツールチェーンは不要です。
UI を変更したら `mise run web` を実行して結果をコミットしてください。CI がバンドルの
最新性を検証します。

---

## リリース

リリースは 2 つのコミュニティツールで自動化されています:

- [**release-plz**](https://release-plz.dev) がバージョン / CHANGELOG の PR を作成し、
  OIDC trusted publishing で `hayate` を crates.io に公開します（保存されたレジストリ
  トークンは不要）。
- [**cargo-dist**](https://axodotdev.github.io/cargo-dist) がアーカイブとインストーラを
  ビルドし、npm パッケージを公開し、ビルド provenance attestation 付きの GitHub Release を
  作成します。

初回セットアップと正確なフローは [`RELEASE.md`](./RELEASE.md) を参照してください。

---

## 謝辞

Hayate は [compio](https://github.com/compio-rs/compio)、
[quinn-proto](https://github.com/quinn-rs/quinn)、
[rustls](https://github.com/rustls/rustls)、
[ring](https://github.com/briansmith/ring)、
[RustCrypto argon2](https://github.com/RustCrypto/password-hashes)、
[BLAKE3](https://github.com/BLAKE3-team/BLAKE3)、
[zstd](https://github.com/facebook/zstd) によって構築されています。

---

<div align="center">

**MIT ライセンス** · [Issues](https://github.com/ShiinaSaku/Hayate/issues) · [Releases](https://github.com/ShiinaSaku/Hayate/releases) · [Website](https://hayate.shiina.xyz)

</div>
