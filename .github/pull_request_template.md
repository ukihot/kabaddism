## 何を

<!-- 変更の内容を1〜2行で -->

## なぜ

<!-- 解決する問題、関連する Issue（Fixes #123）。
     design.md / requirements.md の該当箇所があれば番号を書いてください -->

## どう確かめたか

<!-- 実際に走らせた確認を書いてください。該当するものにチェック -->

- [ ] `cargo test`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo fmt --check`
- [ ] `cargo run --release --bin harness -- --years 1`（バランスに触る変更の場合、前後の出力を貼ってください）
- [ ] 画面で確認した（UI の変更の場合、スクリーンショットを貼ってください）

## 影響

- [ ] セーブ形式が変わる（`FORMAT_VERSION` の更新が要る）
- [ ] 同じシードでの結果が変わる（決定論は保たれるが、既存のセーブや期待値がずれる）
- [ ] 依存クレートが増減する
- [ ] design.md / requirements.md の改訂を伴う

<!-- どれにも当てはまらなければ、この節は消して構いません -->
