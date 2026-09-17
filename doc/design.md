# カバディズム 設計書

v1.0 / 対象: MVP
上位文書: [requirements.md](requirements.md) / [concept.md](concept.md)

対象バージョン: **Bevy 0.19.1** / Rust edition 2024

---

## 1. アーキテクチャ方針

### 1.1 中心となる判断: シミュレーションは ECS に載せない

```
┌─────────────────────────────────────────────┐
│  bevy (app 層)                               │
│   描画 / UI / 入力 / 音 / 画面遷移 / ファイルIO │
│                                             │
│   Resource<GameRes> ───┐                    │
└────────────────────────┼────────────────────┘
                         │ 呼び出しは step_day() と read-only 参照のみ
┌────────────────────────▼────────────────────┐
│  sim (純 Rust / bevy 非依存)                 │
│   世界状態 + 日次パイプライン + 乱数 + 会計     │
└─────────────────────────────────────────────┘
```

本作のシミュレーションは **逐次・決定論・少数エンティティ（数千）** である。ECS の利点（並列化・動的合成・大規模イテレーション）はどれも効かず、逆に以下のコストを払うことになる：

- 決定論の保証が難しい（システム並列実行・Entity ID の割当順）
- セーブが困難（World のシリアライズは重く壊れやすい）
- テストに `App` の構築が必要になる
- 会計の恒等式（生産 = 受取 + 在庫差分）を「1箇所で閉じる」ことができない

したがって:

| 層 | 責務 | 依存 |
|---|---|---|
| `sim` | 世界の状態と、それを1日進める計算のすべて | `serde`, `rand`, `ron` のみ。**bevy を使わない** |
| `app` | 表示・入力・画面遷移・ファイル | bevy 0.19.1 |

`app` は `sim` を**読むだけ**。`sim` を書き換えるのは `Game::step_day()` と、プレイヤー操作を表す少数のコマンド関数のみ。

> ponytail: 単一クレート内のモジュール分割。`sim` が bevy を import しない規律を CI（`cargo deny`不要、grep で十分）で担保する。分離が破れ始めたら workspace 2クレートに割る。

### 1.2 ECS を使う範囲

`app` 側の描画・UI エンティティのみ。地図のタイル、施設スプライト、UI ノード、ニュースの行。これらは毎フレーム `sim` の状態から同期される**派生データ**であり、セーブ対象ではない。

---

## 2. モジュール構成

```
src/
  main.rs              App 構築、プラグイン登録
  bin/
    harness.rs         ヘッドレスのプレイハーネス（§15）。bevy を構築せずに1年回す
  sim/
    mod.rs             Game（最上位状態）、step_day()
    world.rs           District / Facility / Cohort / Person / Team のデータ
    ids.rs             型付き ID（PersonId, TeamId, DistrictId, FacilityId ...）
    calendar.rs        日付、年次イベント日、週末判定
    time_budget.rs     1日24時間の配分計算
    economy/
      mod.rs           日次の生産・決済・収支
      genkaba.rs       現カバ決済
      card.rs          カード利用枠・負担・週末清算
      accounts.rs      K建て会計と恒等式チェック
    training.rs        練習参加 → 成長 → 疲労
    policy.rs          政策カードの適用、進行中事業 (Project)
    budget.rs          年次予算編成
    cup.rs             世界大会、領地移動、他国モデル
    events.rs          ネガティブイベント判定・適用
    news.rs            記事生成、スレッド、履歴
    stats.rs           国家ステータス集計
    rng.rs             決定論的乱数（ドメイン別ストリーム）
    save.rs            セーブ/ロード
    defs.rs            外部データ定義（カード/施設/イベント/文面）の型
  app/
    mod.rs             KabaddismPlugin、Resource<GameRes>
    state.rs           GameState / Phase
    title.rs           タイトル画面
    tick_driver.rs     pending_days → step_day() のフレーム分割実行
    ui/
      mod.rs           画面骨格、配色、共通ウィジェット
      top_bar.rs       カレンダー + 国家ステータス
      news_feed.rs     下部ニュース
      policy_panel.rs  政策カード選択
      map.rs           中央の町/国地図
      inspector.rs     人物/施設/地区の詳細（情報3段階の②③）
      budget_screen.rs 年次予算編成
      cup_screen.rs    世界大会
assets/
  data/
    policies.ron
    facilities.ron
    events.ron
    balance.ron
    news/*.ron
  text/ja.ron
  sprites/
tools/
  check_layering.sh    sim が bevy を import していないことの確認（§1.1）
```

ファイル数は多いが、いずれも単一責務で 200〜400 行を想定。1ファイルに混ぜるとバランス調整時に衝突する。

`app/assets.rs`（RON のロード）と `app/save_io.rs`（ファイル入出力）は**作らない**。
`sim::defs::Defs::load_default()` と `sim::save::{write_to, read_from, slot_path}` が
すでに探索・読み書き・保存先の決定まで持っており、app 側で包み直す理由がないため。
bevy の `AssetServer` を経由しないのは、`Defs` がフレームをまたがない同期ロードで足り、
ロード完了待ちの状態を1つ増やさずに済むから。

---

## 3. Bevy 構成

### 3.1 Cargo.toml

```toml
[package]
name = "kbism"
version = "0.1.0"
edition = "2024"

[dependencies]
bevy = { version = "0.19.1", default-features = false, features = [
  "ui",                      # bevy_ui + winit + state + asset + picking 一式
  "2d",                      # Camera2d とスプライト（地図: フェーズ8）
  "system_font_discovery",   # 日本語を出す。埋め込み FiraMono には和文グリフがない
] }
serde = { version = "1", features = ["derive"] }
ron = { version = "0.12", features = ["integer128"] }  # rand_chacha の word_pos は u128
rand = "0.10"
rand_chacha = { version = "0.10", features = ["serde"] }

[dev-dependencies]
# ベンチは criterion を入れず、まずは #[test] + Instant で足りる

[profile.dev]
opt-level = 1          # sim の日次計算が debug だと遅すぎる
[profile.dev.package."*"]
opt-level = 3
```

音声・GLTF・3D・アニメーションの機能は落とす。bevy 0.19 の feature は
`ui` / `2d` / `3d` / `audio` という上位のまとまりに整理されており、
個別の `bevy_*` を列挙するより `ui` + `2d`（= `3d` と `audio` を外す）のほうが
同じ範囲を短く、かつ将来の再編に強く表せる。

`bevy_feathers` / `bevy_ui_widgets` は v0.19 時点で実験的なため v1 では使わず、必要なウィジェットは `bevy_ui` の `Node` + `Interaction` で自前実装する（種類は5つ程度で足りる）。`ui` feature が `bevy_ui_widgets` を連れてくるが、使わなければよい。

**フォント**: 表示文字列はすべて日本語なので、埋め込みの `FiraMono-subset.ttf` では
1文字も出ない。`system_font_discovery` を有効にし、`TextFont.font` に
`FontSource::SansSerif` を渡してシステムのフォントを引く。和文フォントを
`assets/` に同梱する方針へ切り替えるときは、この feature を外して
`FontSource::Handle` に変えるだけで済む。

> 既知のログ: 起動時に `ICU4X data error: No segmentation model for complex script: Chinese/Japanese`
> が出る。行分割は UAX #14 が CJK を処理するため影響はなく（`icu_segmenter` 自身が
> 「LineSegmenter は CJ 辞書を必要としない」と明記している）、出所は parley が
> `WordSegmenter::new_for_non_complex_scripts` を固定で使っていること。
> 影響するのは日本語の**単語境界**（ダブルクリック選択・単語単位のキャリブレーション）だけ。
> このログは `icu_provider` が `debug_assertions` 時のみ `eprintln!` するもので、
> リリースビルドでは消える。辞書を効かせたい場合は parley 側の変更が要る。

### 3.2 状態遷移

```rust
#[derive(States, Default, Clone, PartialEq, Eq, Hash, Debug)]
enum GameState { #[default] Boot, Title, InGame }

#[derive(SubStates, Default, Clone, PartialEq, Eq, Hash, Debug)]
#[source(GameState = GameState::InGame)]
enum Phase {
    #[default] Planning,   // 政策カード選択。時間は止まっている
    Advancing,             // pending_days を消化中
    Budget,                // 年次予算編成（モーダル）
    Cup,                   // 世界大会（モーダル）
}
```

遷移:

```
Planning --[カード実行]--> Advancing
Advancing --[pending_days == 0]--> Planning
Advancing --[予算編成日に到達]--> Budget --[確定]--> Advancing (残日数>0) or Planning
Advancing --[世界大会日に到達]--> Cup   --[終了]--> Advancing or Planning
```

年次イベントで停止しても `pending_days` は保持する（FR-TIME-06）。

### 3.3 システム配置

すべて `Update` に置く。`FixedUpdate` は使わない — ゲーム内時間は実時間と無関係で、プレイヤー操作でのみ進むため。

```rust
app.add_systems(Update, (
    tick_driver::run_pending_days,      // sim を進める唯一の場所
    ui::sync_from_sim,                  // sim → 表示の一方向同期
).chain().run_if(in_state(GameState::InGame)));
```

`sim` を触るシステムは `run_pending_days` と、プレイヤー操作を表すボタンハンドラ
（政策実行・予算確定・大会実行）だけ。これらは互いに排他な `Phase` でしか動かない。
他はすべて `Res<GameRes>` の読み取りで、書き込み競合が原理的に起きない。

`Game` は bevy を知らないので（§1.1）、`Resource` を実装するのは app 側の
包み紙 `GameRes(pub Game)` の仕事。`Deref` / `DerefMut` を導出するので
呼び出し側は `Game` をそのまま触っているように書ける。

### 3.4 フレームを止めない進行（NFR-03）

60日進行を1フレームで回すとウィンドウが固まる。フレーム予算で分割する。

```rust
fn run_pending_days(mut game: ResMut<GameRes>, mut next: ResMut<NextState<Phase>>) {
    let budget = Instant::now() + Duration::from_millis(6); // 1フレーム6ms上限
    while game.pending_days > 0 && Instant::now() < budget {
        match game.advance_one() {           // ← sim 呼び出しはここだけ
            Some(StopReason::Budget) => { next.set(Phase::Budget); return }
            Some(StopReason::Cup)    => { next.set(Phase::Cup);    return }
            None => {}
        }
    }
    if game.pending_days == 0 { next.set(Phase::Planning); }
}
```

進行速度（1日あたりの演出時間）はプレイヤー設定で変えられるようにし、「早送り」時はこの予算いっぱいまで回す。ニュースは日付順にキューへ積まれ、UI 側が独自のペースで流す。

この「日付順のキュー」を作るときに、1日進むごとの `DayAdvanced` メッセージを足す。
それまでは読む側がいないので**作らない**（フェーズ9）。フェーズ7〜8 の news_feed は
記事本数の変化を見て組み直すだけで足りる。

> ponytail: `Instant` による時間予算は素朴だが正しい。日数が巨大化しない（最大でも数十日）ため、ワーカースレッドへ逃がす必要はない。必要になったら `AsyncComputeTaskPool` へ移す。

---

## 4. sim のデータモデル

### 4.1 最上位

```rust
#[derive(Serialize, Deserialize)]
pub struct Game {
    pub version: u32,
    pub date: Date,
    pub pending_days: u16,
    pub rng: RngSet,
    pub world: World,          // sim::world::World（bevy の World ではない）
    pub treasury: Treasury,
    pub projects: Vec<Project>,
    pub news: NewsLog,
    pub stats: StatsHistory,
    pub nations: Vec<Nation>,  // 他国（簡易モデル）
    #[serde(skip)]
    pub defs: Arc<Defs>,       // 外部データ。セーブに含めない
}
```

`defs`（カード定義・バランス係数）はセーブに含めない。ロード時に現在のデータファイルを再適用する — 調整のたびにセーブが壊れるのを避けるため。定義の削除に対してはロード時に検証し、欠損があれば警告して該当事業を中止扱いにする。

### 4.2 世界

```rust
pub struct World {
    pub districts: Vec<District>,        // 添字 = DistrictId
    pub facilities: Vec<Facility>,
    pub teams: Vec<Team>,
    pub people: Vec<Person>,             // 追跡人物のみ（30〜80）
    pub environment: Environment,        // 天候・供給などの外部条件
}

pub struct District {
    pub name: String,
    pub owner: NationId,                 // 領地の所属（大会で変わる）
    pub cohorts: Vec<Cohort>,            // 8〜16 程度
    pub infra: Infra,                    // 交通整備度・住環境・医療アクセス
    pub distance_to: Vec<u8>,            // 地区間の移動コスト（分）
    pub history: Vec<HistoryEntry>,
}

pub struct Cohort {
    pub age_band: AgeBand,
    pub occupation: Occupation,
    pub headcount: u32,
    pub time: TimeBudget,                // 集団平均の1日配分
    pub condition: Condition,            // 健康・栄養・疲労
    pub ability: Ability,                // 平均能力と分散
    pub obligation: f32,                 // 世帯あたり平均の決済負担 (K)
    pub credit_limit: f32,
    pub team: Option<TeamId>,            // 共同チーム等の所属
}

pub struct Person {                       // 追跡人物
    pub name: String,
    pub home: DistrictId,
    pub age: u8,
    pub occupation: Occupation,
    pub household: Household,
    pub time: TimeBudget,
    pub condition: Condition,
    pub ability: Ability,
    pub talent: f32,                      // 不変。成長速度の係数
    pub motivation: f32,
    pub experience: f32,
    pub team: Option<TeamId>,
    pub injury: Option<Injury>,
    pub status: PersonStatus,             // Active / Paused / Retired / Emigrated
    pub history: Vec<HistoryEntry>,       // ニュースの続報の素
}
```

**コホートと追跡人物は同じ計算関数を通す。** 時間配分・疲労・成長のロジックを2重に書かない（`headcount` を重みとして扱うだけ）。これを破ると「集団は改善したのに選手は改善しない」という説明不能な挙動が出る。

### 4.3 型付き ID

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PersonId(pub u16);
```

Vec の添字。エンティティは削除せず `status` で無効化するため、ID は安定する（セーブ互換とニュース参照の両方に効く）。

---

## 5. 日次パイプライン

`Game::step_day()` は FR-SIM-01 の順序をそのまま関数列にする。

```rust
pub fn step_day(&mut self) -> Option<StopReason> {
    self.date = self.date.next();

    // ① 進行中のものを進める
    policy::advance_projects(self);

    // ② 生活と生産
    let env = environment::update(self);          // 外部環境（乱数ストリーム C）
    time_budget::allocate(self);                  // 24h の配分を決める
    let production = economy::produce(self, env); // 労働 → 財・サービス
    economy::settle_daily(self, &production);     // 現カバ決済・カード利用
    training::run(self);                          // 練習参加・成長
    condition::recover(self);                     // 睡眠・栄養・医療による回復

    if self.date.is_weekend() {
        economy::weekly_settlement(self);         // 町内大会でのカード清算
    }

    // ③④ イベント
    let fired = events::roll(self);               // 日次抽選（乱数ストリーム B）
    events::apply(self, &fired);

    // ⑤ 集計
    let today = stats::aggregate(self);
    self.stats.push(today);

    // ⑥ ニュース
    news::generate(self, &fired, &today);

    debug_assert!(accounts::identities_hold(self));

    self.calendar_stop_reason()   // 予算編成日 / 世界大会日 なら Some
}
```

各関数は `&mut Game` を取る単純な自由関数。呼び出し順が仕様そのものなので、この20行が最も重要な設計物である。トレイトや動的ディスパッチを挟まない。

---

## 6. 決定論と乱数

```rust
#[derive(Serialize, Deserialize)]
pub struct RngSet {
    pub daily: ChaCha8Rng,    // A: 日常の小さな変動（売上・参加人数・練習成果）
    pub events: ChaCha8Rng,   // B: 条件付きイベント
    pub env: ChaCha8Rng,      // C: 外部環境（天候・供給・他国動向）
    pub cup: ChaCha8Rng,      // D: 大会の試合結果
}
```

- ストリームを分けるのは、片方の消費回数の変化がもう片方の結果を変えないようにするため。イベント数が変わっても天候が変わらない。
- `ChaCha8Rng` は `serde` でそのまま状態を保存でき、ロード後も列が継続する（FR-SAVE-02 / AC-10）。
- 反復順序は常に `Vec` の添字順。`HashMap` のイテレーションを計算に使わない。
- 浮動小数は `f32`。並列縮約を行わない（そもそも `sim` は単スレッド）。

決定論テスト:

```rust
#[test]
fn deterministic() {
    let a = play(seed=42, &script);
    let b = play(seed=42, &script);
    assert_eq!(a.state_hash(), b.state_hash());
}
```

`state_hash()` は全フィールドを走査する FNV ハッシュ。セーブ往復テストにも同じ関数を使う。

---

## 7. 時間予算モデル

1日 = 1440分。配分は優先度順に確定し、**残余が練習に回る**（FR-POP-03）。

```
sleep      = 480 - 住環境ペナルティ(0〜90)
work       = 職業ごとの所定（0 / 360 / 480）
commute    = f(住居地区, 勤務地区, 交通整備度)
care       = 育児・介護・家事（世帯構成、託児所の利用可否で減る）
shopping   = 決済に要する時間（現カバ or カード、決済会場の混雑で増える）
─────────────────────────────────────────
leisure    = 1440 - 上記
practice   = min(leisure × 意欲 × 参加可能性, 道場の受入枠)
```

`参加可能性` は「通える道場があるか」「その時間に開いているか」「利用権があるか」の積。終バス延長カードは `commute` と `参加可能性`（夜間枠）を変え、練習時間には直接触らない — これが concept §21-6 の実装形である。

政策から能力値への直接加算は、コード上に**存在させない**。`Ability` を書き換える関数は `training::run` と `events::apply`（負傷・離脱）だけに限定する。

---

## 8. 経済モデル（concept §8 の未確定4項目の確定）

### 8.1 単位と会計

K は評価単位であり通貨ではない。商品ごとに `standard_value: f32`（K）を `defs` で固定し、実質 GDP を次で算出する（FR-ECO-06）:

```
GDP_day = Σ(生産量_財 × standard_value_財) − Σ(中間投入量 × standard_value)
```

勝敗による所有移転、カードの負担累積・清算は **GDP に一切入らない**。これは移転取引であり生産ではない。`accounts::identities_hold()` が毎日次を検証する:

```
生産 = 消費 + 在庫変化                    （実物）
Σ負担の増加 = Σ負担の清算 + Σ未清算残高の増加   （決済）
```

このアサートは debug ビルドで常時有効。経済が発散する前に落とす（リスク表の最上位項目への対策）。

### 8.2 現カバ決済（FR-ECO-01）

```
p_win = S_c^2 / (S_c^2 + S_s^2)         // S = チーム実効戦力（疲労・出場人数を反映）
負担倍率 = if 勝利 { 0.6 } else { 1.4 } // 期待値が 1.0 近傍になるよう defs で調整
支払負担 = 商品の standard_value × 負担倍率
```

- 商品は**必ず**引き渡される。決済が成立しないという状態を作らない（生活必需品の供給が勝敗で止まると、ゲームが成立しない）。
- 消費者・店舗の双方が時間（`shopping`）と疲労を消費する。強い相手のいる高級店は所要時間が長い。
- 負担は「労務・物資の提供」として店舗側の受取に加算される。生産は増えない。

### 8.3 カード決済（FR-ECO-02, 03）

利用時:

```
obligation += standard_value × 1.0        // 現カバと違い倍率なし。ただし後で清算義務
credit_limit = base
             × 契約チーム戦力係数 (0.5〜2.0)
             × 出場余力係数 (0〜1.0)
             × 履行実績係数 (過去4週の清算率, 0.6〜1.2)
```

`obligation > credit_limit` になると、その世帯はカードを使えず現カバへ回る（＝時間と疲労を余分に払う）。これが「決済能力不足 → 生活調達が難しくなる」の実装。

週末清算:

```
清算力 = Σ(出場チームの戦力 × 出場枠 × コンディション) × 会場係数
清算量 = min(清算力 × 換算レート, obligation)
obligation -= 清算量
if 清算量 < obligation_before × 要求率 {
    未清算として繰越し、credit_limit を縮小、履行実績係数を下げる
}
```

回復手段は2系統（FR-ECO-03）:
1. **労務による直接清算** — 余暇時間を労働に振り替える。練習時間が削られる（育成への跳ね返り）。
2. **公的・共同体による肩代わり** — 公共チームの稼働枠、共同チームへの加入、公的支援。予算を消費する。

代理決済（FR-ECO-08）は、本人が戦えない（高齢・育児・負傷・疾病）場合に 1 の代わりに使える経路として、代理業チームと公的支援の2種を実装する。

### 8.4 事業者の持続（FR-ECO-04）

店舗・生産者・カード組合は日次収支を持つ:

```
店舗:  受取負担 − 仕入(中間投入) − 常駐チーム維持費 = 日次収支
       累積収支 < 閾値 が N日続く → 縮小 → 閉店（ニュース化）
組合:  加盟店への給付保証 ≤ 準備（公共チーム稼働 + 物資 + 公的支援）
       不足 → 全世帯の credit_limit を一律縮小（「カード利用枠の縮小」イベント）
```

「カード組合の運営監査」カードは、この準備率と過剰契約の内訳を可視化する（数値を改善するのではなく、見えるようにする）。

---

## 9. 育成モデル

```
参加判定    : practice_minutes > 0 かつ 受入枠に空きがある
指導の質 q  : 道場の指導者数・技能 / 在籍者数（不足すると受入停止イベント）
コンディション c : f(疲労, 健康, 栄養, 住環境)  ∈ [0, 1.2]
成長        : ability += practice_h × q × talent × c × k − decay(ability, age)
疲労        : fatigue += practice_h × 強度 × (2 − c)
```

- `decay` により能力は放置で下がる。高齢では `decay` が成長を上回り、自然に引退へ向かう。
- `c` が低い状態での練習は成長が小さく疲労だけ増える → 「疲労蓄積 → 負傷」イベントの確率が上がる。無理押しが機能しない設計。
- 日常変動（乱数ストリーム A）は成長量に ±10% 程度の乗算ノイズとして入れる。イベントの発生には使わない。

---

## 10. 政策と事業

```rust
pub struct PolicyDef {         // defs（policies.ron）
    pub id: String,
    pub name: String,           // 俗称
    pub official_name: String,  // 大仰な公文書名
    pub days: u8,
    pub initial_cost: f32,      // K
    pub upkeep: f32,            // K / 年
    pub requires: Vec<Requirement>,   // 人員・施設・前提事業・対象
    pub target: TargetSpec,           // 地区 / コホート / 人物
    pub effects: Vec<Effect>,         // 環境変数への作用のみ
    pub lead_time: RangeInclusive<u8>,// 効果発現まで
    pub uncertainty: String,          // 表示用
}

pub struct Project {            // 実行中の事業
    pub policy: String,
    pub target: Target,
    pub days_remaining: u16,
    pub resources_held: Resources,
    pub state: ProjectState,    // Planning / Construction / Operating / Stalled
}
```

`Effect` が触れる先は **`Infra` / `Facility` / 利用権 / 時間コスト / 受入枠 / 支援額** のみ。`Ability` や `Stats` を直接書けないよう、`Effect` の enum にそもそも項目を作らない。型で規律を担保する。

実行フロー:

```
カード選択 → requires 検証（不足を列挙して表示）
          → 予算枠から initial_cost を引当（枠と実資源を別判定: FR-BUD-05）
          → Project を projects に push
          → pending_days = days
```

事業の完成は `days` とは独立。工事は `advance_projects` で日々進み、必要資源（建設人員・資材）が確保できない日は `Stalled` となり進まない（「費用増による工事停止」イベント）。

---

## 11. イベントとニュース

### 11.1 イベント

```rust
pub struct EventDef {
    pub id: String,
    pub scope: Scope,              // District / Facility / Person / Cohort
    pub trigger: Vec<Condition>,   // 対象の状態への条件（全国平均は使わない）
    pub base_chance: f32,          // 日次確率
    pub stage: u8,                 // 0 = 予兆, 1 = 本番, 2 = 深刻
    pub prerequisite: Option<String>,  // 前段イベントの id
    pub cooldown_days: u16,
    pub effects: Vec<Effect>,
    pub news: NewsTemplate,
}
```

- 日次確率で抽選するため、政策日数による歪みが原理的に生じない（FR-SIM-04）。
- 同一 (event_id, target) の進行中フラグと cooldown で重複を防ぐ（FR-SIM-06）。
- `prerequisite` により予兆 → 本番の連鎖を強制する（FR-SIM-07）。
- 発生時に `Issue` を開き、解決時に閉じる。閉じた `Issue` は `history` に残る（FR-SIM-10）。

### 11.2 ニュース

```rust
pub struct Article {
    pub date: Date,
    pub kind: ArticleKind,        // Policy / Project / Person / Economy / Cup / Local / Trivial
    pub headline: String,
    pub body: String,
    pub subjects: Vec<Subject>,   // 遷移先（人物/施設/地区）
    pub thread: Option<ThreadId>, // 続報のスレッド
    pub weight: u8,               // 重要度。政策終了時に残す記事の選別に使う
    pub pinned: bool,
}
```

- 記事は `stats` と**同じ内部状態から**生成する（FR-STAT-05）。ニュース専用の隠し変数を作らない。
- `ThreadId` は `Issue` または `Project` または `PersonId` に紐づく。これだけで「人手不足の道場が休止し、政策で再開し、数年後に代表を送り出す」連鎖が自動的に1本の線になる。
- 文面は `assets/text/ja.ron` のテンプレート + プレースホルダ。コードに日本語を埋め込まない（NFR-10）。
- `Trivial` 記事は毎日一定確率で混ぜる。重大性と大仰さの不一致（FR-NEWS-09）は、`weight` が低いのに `official_name` を使う記事として表現する。

---

## 12. 予算編成と世界大会

### 12.1 予算編成

```rust
pub struct Treasury {
    pub allocations: [f32; 8],      // 分野別の年度枠（K）
    pub spent: [f32; 8],
    pub committed_upkeep: f32,      // 既存事業の継続費用
    pub revenue_estimate: f32,
    pub real_resources: Resources,  // 人員・物資・公共チーム稼働（枠とは別）
}
```

- 画面は「次年度収入見込み / 継続費用 / 残り配分可能額」を先に提示し、その上でスライダで8分野へ配分する。
- スライダの初期値は `BudgetBriefing::default_plan()`（配分可能額の固定重み按分）。
  ヘッドレスのハーネスも同じ関数を使う — 自動プレイと UI の既定値が食い違うと
  バランス調整の実測が UI の挙動を予測しなくなるため、1箇所に置く。
  重みを `balance.ron` へ出すのはフェーズ9。
- 収入を超える配分を**許可する**。結果は翌年度の事業停止として現れる（FR-BUD-06）。
- 政策カードの `initial_cost` は対応する分野の枠から引く。枠が空でも実資源がなければ `Stalled`（FR-BUD-05）。

### 12.2 世界大会

```rust
pub struct Nation {              // 他国（簡易モデル）
    pub name: String,
    pub strength: f32,
    pub growth: f32,
    pub districts: Vec<DistrictId>,
}
```

- 代表選考は追跡人物から `ability × condition` 上位を自動提示し、プレイヤーが差し替えられる。
- 試合結果は戦力比のロジスティック + 乱数ストリーム D。連携・疲労・負傷が戦力に反映される。
- **賭け金の提示**: 大会前に「賭ける領地」と「最大損失」を確定表示する。本拠地は対象外（FR-CUP-04）。この2つは大会処理より前に計算し、UI に渡した値をそのまま使う（後から変わらないことをテストで保証: AC-11）。
- 領地の移動は `District.owner` の変更のみ。住民・施設・チーム・履歴はそのまま残る。所属が変わった地区の追跡人物は、翌年から他国代表の候補になる（FR-CUP-08）。

---

## 13. UI 設計

### 13.1 レイアウト

```
┌────────────────────────────────────────────────────┐
│ 年/月/日  大会まで N日 │ 人口 GDP 予算 平均能力 代表 │ ← top_bar
├──────────────┬─────────────────────────────────────┤
│              │                                     │
│  政策パネル   │        町 / 国 の地図                │
│  （推奨3件   │   施設・人の動き・事業の進捗          │
│   + 全一覧） │   地図切替: 生活/育成/経済/領土       │
│              │                                     │
├──────────────┴─────────────────────────────────────┤
│ ニュース（日付順に流れる。クリックで対象へ）          │ ← news_feed
└────────────────────────────────────────────────────┘
```

### 13.2 情報の3段階（FR-UI-02）

| 段階 | 場所 | 内容 |
|---|---|---|
| ① 何が | ニュース見出し / ステータスの色 | 「北部道場、入門受け入れ停止」 |
| ② 誰・どこ | 見出しクリック → inspector | その道場の在籍者数・指導者数・地区・影響を受けている人物一覧 |
| ③ なぜ・何が効く | inspector の内訳パネル | 指導枠 = 指導者3名 × 定員12 = 36 < 在籍44。関連政策カードへの直リンク |

③の「なぜ」は、`stats::aggregate` が集計時に残す**内訳レコード**から生成する。UI が独自に再計算しない（NFR-09）。

### 13.3 sim → UI の同期

```rust
fn sync_from_sim(game: Res<Game>, mut q: Query<...>) {
    if !game.is_changed() { return; }
    // 変更のあった日だけ UI ノードを更新
}
```

UI は毎フレーム全再構築しない。`Res<Game>` の変更検知 + `DayAdvanced` メッセージで差分更新する。ニュース行は上限（表示100件）を持つリングで、それ以上は履歴画面から検索する。

---

## 14. セーブ・ロード

```rust
#[derive(Serialize, Deserialize)]
struct SaveFile {
    format_version: u32,      // 非互換時にインクリメント
    game: Game,               // defs は skip されている
}
```

- 形式: RON（可読性を優先。デバッグでの差分確認が効く）。1年分のセーブで数 MB 以内。
- 保存先: `dirs` は使わず `std::env::var("APPDATA")` / `XDG_DATA_HOME` を直接読む。
- `format_version` 不一致は**拒否**する。マイグレーションは v1 では書かない（FR-SAVE-03）。上書きせず、別名で保持する。
- オートセーブは年次イベント直前と政策実行直前。スロットはローテーション3枠。
- 往復テスト: `hash(game) == hash(load(save(game)))`（AC-10）。

> ponytail: RON で始める。セーブが 10MB を超えるか 1秒を超えたら `postcard` + zstd に差し替える（`Serialize` 実装は変えずに済む）。

---

## 15. テスト戦略

`sim` が bevy に依存しないので、すべて素の `cargo test` で回る。

| 種別 | 内容 |
|---|---|
| 決定論 | 同一シード2回 → 状態ハッシュ一致（AC-06） |
| セーブ往復 | save → load → ハッシュ一致（AC-10） |
| 会計恒等式 | 360日走らせて `identities_hold()` が毎日成立 |
| 政策回帰 | 全20カードについて、実行あり/なしを同一シードで比較し、期待指標が期待方向へ動く（AC-03）。期待値は各カードの定義に `expect:` として併記する |
| 完走 | ランダム政策選択で360日 × 10回、パニックなし（AC-01） |
| 経路発生 | 「生活問題 → 政策 → 参加 → 成長 → 代表」の事例が1件以上（AC-05） |
| 性能 | 1ティック ≤ 2ms（AC-09）。`#[test]` + `Instant` で十分 |

UI はテストしない。手動チェックリスト（AC-04）で見る。

ヘッドレスのプレイハーネス:

```rust
// tests/playthrough.rs
let mut game = Game::new(seed, &defs);
while game.date.year == 1 {
    let card = strategy.pick(&game);
    game.execute_policy(card);
    while game.pending_days > 0 { game.step_day(); }
}
```

`app` を一切構築せずに1年が回る。これがこの設計の最大の利点で、バランス調整のイテレーションが数秒で済む。

---

## 16. 実装フェーズ

各フェーズの終わりに、上のハーネスで1年回ることを確認する。

| # | 内容 | 完了条件 |
|---|---|---|
| 1 | `sim` の骨格: Date / World / step_day の空実装 / RngSet / セーブ | 360日が空回しで回り、決定論テストが通る |
| 2 | 時間予算 + 育成 + 疲労 | 練習時間が住環境・通勤で変わり、能力が動く |
| 3 | 経済（§8 全体）+ 会計恒等式 | 360日で恒等式が崩れない。店舗が黒字/赤字で分かれる |
| 4 | 政策カード20枚 + 事業 + 予算編成 | 政策回帰テストが全カードで通る |
| 5 | イベント15種 + ニュース + スレッド | 続報が1本の線になる |
| 6 | 世界大会 + 領地 + 他国 | 1年サイクルがハーネスで完結 |
| 7 | bevy 側: 状態遷移 / tick_driver / top_bar / news_feed / policy_panel / 予算・大会の最小モーダル | 画面から1年遊べる |
| 8 | 地図 / inspector（情報3段階）/ 予算・大会画面の作り込み / オートセーブ | AC-04 の手動チェックリストが通る |
| 9 | バランス調整、演出、AC 全項目の確認 | v1 |

フェーズ1〜6 は描画がなくても検証できる。**UI は最後**でよく、それによって最も不確実な経済モデルに開発時間を集中できる。

予算編成と世界大会の画面は 7 と 8 に割れる。「画面から1年遊べる」には
年次イベントで止まったあと**先へ進める手段**が要るので、提示と確定だけの
モーダルをフェーズ7に置く（`budget_screen.rs` / `cup_screen.rs`）。
分野別スライダと代表の差し替えはフェーズ8 で同じファイルに足す。

オートセーブ（§14）はフェーズ8。`sim::save` 側は完成しているが、
呼び出しにはファイル書き込みの失敗をプレイヤーへ見せる経路が要り、
それは inspector と同じ通知の仕組みに乗る。

---

## 17. 未確定事項（実装時に決める）

| 項目 | いつ決めるか |
|---|---|
| バランス係数の具体値（`balance.ron` の全数値） | フェーズ3〜6 のハーネス実測で |
| 領地の評価額・供出枠・順位ごとの配分方法（concept §16） | フェーズ6 |
| 地区数・コホート分割の粒度（4地区×8コホートで足りるか） | フェーズ2の性能実測で |
| 初期シナリオ（どの地区にどんな問題を仕込むか） | フェーズ9。AC-05 の経路が確実に発生する配置にする |
| 地図の表現（タイル / 抽象ノード） | フェーズ8。まず抽象ノードで作り、不足したらタイルへ |
