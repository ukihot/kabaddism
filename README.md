# カバディズム / kabaddism

**国民の暮らしを整えてカバディを強くし、年に一度の世界大会で領地を獲得する、国家育成シミュレーション。**

プレイヤーは国家カバディ育成機構の責任者として、代表選手の練習だけでなく、教育・医療・住環境・交通・経済・競技施設を行政から整備する。生活が整えば練習に出られる人が増え、練習が増えれば代表が強くなり、大会に勝てば領地が増える — この因果を追いかけるゲーム。

Rust + [Bevy 0.19](https://bevy.org/) 製。シングルプレイ、オンライン要素なし。

> **状態: 開発中（MVP フェーズ7/9）。** 画面から1年分を通して遊べるところまで動く。地図・詳細画面・バランス調整が未了で、セーブ形式は予告なく変わる。

---

## 動かす

必要なもの: [Rust](https://rustup.rs/) 1.96 以降（edition 2024）。Windows 10/11 x86_64 が主対象、Linux x86_64 が副。

Linux では、bevy が使う -sys クレートの開発ヘッダが要ります（Debian / Ubuntu の場合）。

```sh
sudo apt-get install libudev-dev libwayland-dev libxkbcommon-dev libfontconfig-dev
```

`libfontconfig-dev` が無いと `yeslogic-fontconfig-sys` のビルドが
`Package fontconfig was not found in the pkg-config search path` で止まります。
日本語フォントをシステムから探すために必要です。

```sh
git clone https://github.com/ukihot/kabaddism.git
cd kabaddism
cargo run --release
```

初回は Bevy のビルドに10分以上かかる。`--release` を推奨（デバッグビルドは日次計算が遅く、依存ライブラリの診断ログも出る）。

### ヘッドレスで回す

画面を出さずに1年分のシミュレーションを回せる。バランスを見るときはこちらが速い。

```sh
cargo run --release --bin harness -- --seed 42 --years 1
```

| 引数 | 既定 | 意味 |
|---|---|---|
| `--seed N` | 42 | 乱数シード。同じシードなら結果は完全に一致する |
| `--years N` | 1 | 何年分回すか |
| `--quiet` | off | 日々のログを抑え、年末の集計だけ出す |

`cargo run --release -- --seed 123` で、ゲーム本体のシードも指定できる。

---

## 構造

この設計の中心は、**シミュレーションを ECS に載せないこと**。

```
app （bevy）   描画 / UI / 入力 / 画面遷移
   │  呼ぶのは step_day() と読み取りだけ
sim （純 Rust）世界状態 + 日次パイプライン + 乱数 + 会計
```

`sim` は `serde` / `rand` / `ron` にしか依存せず、bevy を import しない。おかげで `App` を構築せずに `cargo test` と `harness` で全部検証でき、バランス調整のイテレーションが数秒で済む。この規律は `tools/check_layering.sh` で確認する。

理由と代償は [doc/design.md §1.1](doc/design.md) に書いてある。

## ドキュメント

| 文書 | 中身 |
|---|---|
| [doc/concept.md](doc/concept.md) | 企画書。何を面白いと考えているか |
| [doc/requirements.md](doc/requirements.md) | 要件定義。何を作り、何を作らないか（FR / NFR / 受入条件） |
| [doc/design.md](doc/design.md) | 設計書。モジュール構成、日次パイプライン、経済モデル、実装フェーズ |

数値バランスは `assets/data/*.ron`、表示文字列は `assets/text/ja.ron` にあり、再ビルドせず書き換えられる。

## 貢献

[CONTRIBUTING.md](CONTRIBUTING.md) を参照。脆弱性の報告は [SECURITY.md](SECURITY.md) の手順で。

## ライセンス

以下のいずれかを選択して利用できる。

- Apache License 2.0（[LICENSE-APACHE](LICENSE-APACHE)）
- MIT License（[LICENSE-MIT](LICENSE-MIT)）

`src/`、`assets/`、`doc/` を含むリポジトリ全体に同じ条件が適用される。

貢献として提出された成果物は、Apache-2.0 の定義に従い、追加の条件なしに上記の二重ライセンスで受け入れられる。
