<div align="center">

<img src="./assets/logo.svg" alt="Hayate" width="128" height="128" />

# Hayate（疾風）

**暗号化・圧縮対応の LAN ファイル転送。バイナリひとつ、コマンドひとつ。クラウドもアカウントも SSH も不要です。**

QUIC トランスポート · X25519 + AEAD · 4 語ペアリングフレーズ · macOS / Linux / Windows / Android

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

両方のマシンで 1 コマンドずつ。ファイルは暗号化・圧縮され、エンドツーエンドで整合性が検証されて届きます。

<div align="center">
<img src="./assets/dashboard.png" alt="Hayate web ダッシュボード" width="100%" />
<br />
<sub><code>hayate web</code> — トークン認証付き・ループバック限定のダッシュボード。</sub>
</div>

---

## 概要

同じ部屋にある 2 台のマシン間でファイルを移すのに、クラウド経由の往復も、SSH デーモンも、チャットアプリの添付容量制限も、本来いらないはずです。Hayate は単一の静的バイナリで、ローカルネットワークを手元でいちばん速い転送経路に変えます。人間が読めるコードフレーズでデバイスをペアリングし、mDNS と UDP ブロードキャストでピアを発見し、アプリケーション層の暗号化を施した QUIC でデータを送ります。サーバー不要、設定不要、信頼する第三者も不要です。

## 特徴

|                    |                                                                                                                                                             |
| ------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **パフォーマンス** | QUIC、4 MiB フレーム、8 段の非同期先読み。圧縮と AEAD は専用ワーカースレッドで実行します。                                                                  |
| **暗号化**         | 一時的な X25519 鍵合意、HKDF-SHA256、フレーム単位の AEAD（AES-256-GCM または ChaCha20-Poly1305）。すべて [ring](https://github.com/briansmith/ring) 経由。 |
| **ゼロ設定**       | 4 語のペアリングフレーズが mDNS + UDP ブロードキャストでピアを探索・認証します。`ip:port` の直接指定も可能です。                                            |
| **探索**           | 受信側が mDNS 広告と UDP クエリに応答するため `hayate discover` は即座。`--scan` はネットマスク対応のサブネットスイープ。                                   |
| **ダッシュボード** | `hayate web` がトークン認証付きのループバック限定ダッシュボードを提供。ピア、デバイス情報、転送履歴を表示。                                                   |
| **ポータビリティ** | macOS・Linux・Windows 向けの自己完結バイナリ 1 つ。x64 / arm64 の両対応（Android/Termux はソースからビルド）。                                               |
| **スクリプト対応** | NDJSON イベントストリーム、安定した文書化済み終了コード、`--quiet` / `--verbose`、機械可読な `--format json`。                                              |

内部では、完了ベースの非同期ランタイム [compio](https://github.com/compio-rs/compio)（io_uring / IOCP / kqueue）上で動作しています。現代の高スループットサーバーと同じクラスのカーネルプリミティブです。

---

## インストール

### シェルインストーラ

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/ShiinaSaku/Hayate/releases/latest/download/hayate-cli-installer.sh | sh
```

Windows は同じリリースの `hayate-cli-installer.ps1` を実行してください。

### npm

```bash
npm install -g @shiinasaku/hayate
hayate --help
```

インストール時に macOS・Linux・Windows（x64 / arm64）向けのビルド済みバイナリを取得します。

### GitHub Releases

ビルド済みアーカイブ（`.tar.xz` / `.zip`）、インストーラ、`sha256.sum`、ビルド provenance attestation を公開しています:
[最新リリース](https://github.com/ShiinaSaku/Hayate/releases)。

### Cargo（ライブラリ）

```bash
cargo add hayate
```

転送エンジンは[ライブラリクレート](https://docs.rs/hayate)としても提供しています。独自のインターフェースをその上に構築できます。

### ソースからビルド

```bash
git clone https://github.com/ShiinaSaku/Hayate.git
cd Hayate
cargo build --release -p hayate-cli
./target/release/hayate --help
```

**Rust 1.96**（edition 2024）が必要です。

---

## 使い方

### ペアリングモード — IP アドレス不要

```bash
# 送信側 — 一度きりのコードフレーズを表示して待機
hayate send ./photos.zip

# 受信側 — 同じフレーズで参加
hayate receive --code "forest-river-mango-silver-orbit"
```

フレーズはセッションの認証に使われます。鍵導出の種になるため、フレーズが違えば復号に失敗し、転送は中止されます。

### 直接モード — アドレスが分かっている場合

```bash
hayate receive --port 50001 --output ./downloads
hayate send ./archive.tar.gz 192.168.1.50:50001
```

直接転送も認証したい場合は、**両側**で `--code <phrase>` を指定してください。フレーズが帯域外パスフレーズとして機能します。

### ディレクトリ

ディレクトリは tar アーカイブとしてストリームされ、オンザフライで圧縮・暗号化されます。展開時には絶対パス、`..` 成分、シンボリックリンクによる脱出を拒否します。

```bash
hayate send ./my-project
hayate receive --code "harbor-lantern-cedar-quartz-drift" --output ./downloads/
```

### ピア探索

```bash
hayate discover                       # mDNS + UDP クエリ（通常 ~3 秒）
hayate discover --scan                # サブネット全ホストもスキャン
hayate discover --cidr 10.0.0.0/16    # 明示的なネットワークをスキャン
```

受信側は mDNS で広告し、UDP クエリにも応答するため、通常の LAN では即座に見つかります。
サブネットスキャンは 4096 ホストまでに制限され、インターフェースのネットマスク
（`/22`、`/16` など）を尊重します。

### Web ダッシュボード

```bash
hayate web                 # http://127.0.0.1:8787/?t=<token> を表示
hayate web --port 9000
```

ループバック限定のダッシュボードを起動します。ファイルをドロップして検出済みピアや入力した
`ip:port` へ送信でき、**Start receiving** で `~/Downloads/Hayate` への受信を開始できます。
進捗は SSE で更新され、完了した転送は履歴に残ります。起動ごとに変わるトークンが必要で、
ループバック外に公開するには `--allow-lan` を明示してください。

---

## コマンドリファレンス

### `hayate send <PATH> [TARGET]`

| フラグ                    | 説明                                                 | デフォルト               |
| ------------------------- | ---------------------------------------------------- | ------------------------ |
| `PATH`                    | 送信するファイルまたはディレクトリ                   | 対話ピッカー             |
| `TARGET`                  | 受信側の `ip:port`（省略時はペアリング）             | —                        |
| `--to <NAME>`             | 保存済みピアへ送信                                   | —                        |
| `--pick`                  | LAN をスキャンして受信側を選択                       | オフ                     |
| `--code <phrase>`         | ペアリングフレーズ（ターゲット指定時はパスフレーズ） | 非ペアリング時は自動生成 |
| `-z, --compress[=<bool>]` | Zstd 圧縮                                            | オン                     |
| `--no-compress`           | 圧縮を無効化（`-z` と競合）                          | オフ                     |
| `--hash <algo>`           | 整合性アルゴリズム: `blake3` または `sha256`         | `blake3`                 |
| `--bandwidth-limit <RATE>` | 帯域制限（`10MiB`、`500KiB`、`2M`）                | 無制限                   |
| `--no-progress`           | プログレスバーを非表示                               | オフ                     |

### `hayate receive`

| フラグ               | 説明                       | デフォルト                 |
| -------------------- | -------------------------- | -------------------------- |
| `-b, --bind <addr>`  | バインドアドレス           | `0.0.0.0`（`HAYATE_BIND`） |
| `-p, --port <port>`  | 待受ポート                 | `50001`（`HAYATE_PORT`）   |
| `-o, --output <dir>` | 保存先ディレクトリ         | `.`                        |
| `--code <phrase>`    | ペアリングセッションに参加 | なし                       |
| `--auto-accept`      | 承認プロンプトを省略       | オフ                       |
| `--no-progress`      | プログレスバーを非表示     | オフ                       |

### `hayate discover`

| フラグ                 | 説明                               | デフォルト |
| ---------------------- | ---------------------------------- | ---------- |
| `-t, --timeout <secs>` | スキャンのタイムアウト             | `15`       |
| `--cidr <cidr>`        | サブネット（例: `192.168.1.0/24`） | 自動       |

### グローバルフラグ

| フラグ                           | 説明                                           |
| -------------------------------- | ---------------------------------------------- |
| `--color <auto\|always\|never>`  | カラー出力のポリシー                           |
| `--format <pretty\|plain\|json>` | 人間向け UI、プレーンテキスト、NDJSON イベント |
| `-q, --quiet`                    | 出力を減らす（繰り返し可）                     |
| `-v, --verbose`                  | 詳細出力（繰り返し可）                         |

完全なリファレンスはバイナリ内にあります: `hayate docs`。

---

## スクリプトと自動化

`--format json` は stdout に 1 行 1 イベントの NDJSON を出力します。ステージ、進捗、ピア、サマリーなど、どの言語からでも安全に消費できます。

```bash
hayate send ./build.tar.zst --format json | jq -r 'select(.type=="summary") | .speed_bps'
```

終了コードは安定しており、文書化されています（`hayate docs exit`）:

| コード | 意味                            |
| -----: | ------------------------------- |
|      0 | 成功                            |
|      1 | 一般的なランタイム / 転送エラー |
|      2 | 使い方 / 引数のエラー           |
|      3 | 受信側が転送を拒否              |
|      4 | プロトコルバージョン不一致      |
|      5 | ペアリングパスフレーズが無効    |
|      6 | タイムアウト                    |
|      7 | ユーザーによるキャンセル        |
|    130 | 中断（Ctrl+C / Esc）            |

---

## シェル補完

```bash
hayate completions bash --install   # ~/.bash_completion.d/hayate
hayate completions zsh --install    # ~/.zsh/completions/_hayate
hayate completions fish --install   # ~/.config/fish/completions/hayate.fish
hayate completions powershell       # stdout に出力
```

`--install` 後、Hayate はシェルの rc ファイルに追加すべき行をそのまま表示します。Fish は次のセッションから自動で補完を読み込みます。

---

## パフォーマンス

```text
Disk → [8 async reads × 4 MiB] → [worker threads: zstd + AEAD] → [QUIC window] → Wire
```

- **先読み** — 並行 `read_at` により、暗号処理の間もディスクを休ませません。
- **ワーカースレッド** — 圧縮と AEAD はチャネルで接続された専用スレッドで実行し、非同期イベントループを一切ブロックしません。
- **暗号スイート交渉** — ハードウェア AES があれば AES-256-GCM、なければ ChaCha20-Poly1305（`HAYATE_FORCE_CHACHA20` で強制可能）。
- **選択的圧縮** — zstd レベル 1。すでに圧縮済みの拡張子（`.zip`、`.mp4` など）はスキップ。
- **順序付きディスク書き込み** — 受信側はフレームを並べ替えてから書き込むため、ディスク I/O は常にシーケンシャルです。

### Linux のソケットバッファ

負荷時に転送が停滞する場合は、カーネルの UDP バッファ上限を引き上げてください:

```bash
sudo sysctl -w net.core.rmem_max=134217728
sudo sysctl -w net.core.wmem_max=134217728
```

macOS と Windows は UDP バッファを自動で調整します。

---

## セキュリティモデル

| 層                 | プリミティブ                                                     |
| ------------------ | ---------------------------------------------------------------- |
| 鍵合意             | 一時的な X25519 ECDH                                             |
| 鍵導出             | HKDF-SHA256（トランスクリプト束縛、パスフレーズは IKM に混合）   |
| フレーム暗号       | AES-256-GCM または ChaCha20-Poly1305（フレームごとに新規ノンス） |
| 整合性             | 平文に対する BLAKE3 または SHA-256                               |
| メタデータ         | ファイル名とサイズは使用前に暗号化                               |
| ディレクトリ安全性 | `..`、絶対パス、シンボリックリンク脱出を拒否                     |

ペアリングモードでは、フレーズを知るピアだけがセッション鍵を導出できます。フレーズが間違っていればメタデータの AEAD に失敗し、転送は中止されます。探索自体は設計上認証されません。**転送を認証するのはハンドシェイクです**。直接モードはネットワークの局所性に依存します。両側で `--code` をパスフレーズとして渡す場合を除きます。

すべての暗号処理は [ring](https://github.com/briansmith/ring) によって提供されます。独自実装のプリミティブはありません。

---

## 比較

| ツール         | トランスポート | 探索               | 暗号化            | サーバー要否       | Android        |
| -------------- | -------------- | ------------------ | ----------------- | ------------------ | -------------- |
| scp / rsync    | TCP / SSH      | 手動 IP            | SSH               | sshd               | 限定的         |
| Magic Wormhole | TCP / TLS      | ランデブーサーバー | PAKE              | 必要（公開リレー） | Python 経由    |
| LocalSend      | HTTP / HTTPS   | mDNS               | TLS               | 不要               | 対応           |
| croc           | TCP リレー     | コードフレーズ     | PAKE              | リレー任意         | Go 経由        |
| **Hayate**     | **QUIC**       | **mDNS + UDP**     | **X25519 + AEAD** | **不要**           | **ネイティブ** |

---

## ライブラリとして使う

```toml
[dependencies]
hayate = "6"
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

公開 API が compio の型を公開することはありませんが、返される future は **compio** ランタイム上で実行する必要があります。独自インターフェース向けには、ライフサイクルの全ステップを報告するステージド API があります: `HayateSender::send_with`、`HayateReceiver::receive_with`、`ListeningReceiver`、`TransferStage`。ドキュメント: [docs.rs/hayate](https://docs.rs/hayate)。

---

## 開発

```bash
mise run check      # fmt (nightly rustfmt) + clippy + tests + ダッシュボード型チェック
mise run test       # cargo test --workspace --all-targets
mise run web        # ダッシュボード変更後に web/build を再生成
mise run build      # このホスト向けリリースバイナリ
mise run android    # Termux 用バイナリ (aarch64-linux-android)
```

ダッシュボードは `web/`（SvelteKit + Tailwind + shadcn-svelte、Bun + Vite）にあります。ビルド成果物は CLI バイナリに埋め込まれるため、UI を変更したら `mise run web` を実行して `web/build` をコミットしてください。

リリースは [release-plz](https://release-plz.dev)（バージョン管理・crates.io への OIDC 公開）と [cargo-dist](https://axodotdev.github.io/cargo-dist)（アーカイブ、shell/PowerShell/npm インストーラ、GitHub attestation）で行います。詳細は `RELEASE.md`。

---

## 謝辞

Hayate は [compio](https://github.com/compio-rs/compio)、
[quinn-proto](https://github.com/quinn-rs/quinn)、
[rustls](https://github.com/rustls/rustls)、
[ring](https://github.com/briansmith/ring)、
[BLAKE3](https://github.com/BLAKE3-team/BLAKE3)、
[zstd](https://github.com/facebook/zstd) によって構築されています。

---

<div align="center">

**MIT ライセンス** · [Issues](https://github.com/ShiinaSaku/Hayate/issues) · [Releases](https://github.com/ShiinaSaku/Hayate/releases) · [Website](https://hayate.shiina.xyz)

</div>
