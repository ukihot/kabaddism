//! ニュース（design.md §11.2 / FR-NEWS-*）
//!
//! 記事は `stats` と**同じ内部状態から**生成する。ニュース専用の隠し変数を作らない（FR-STAT-05）。
//! 文面は assets/text/ja.ron のテンプレート + プレースホルダ。コードに日本語を埋め込まない（NFR-10）。

use serde::{Deserialize, Serialize};

use super::calendar::Date;
use super::ids::{Subject, ThreadId};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ArticleKind {
    /// 政策の効果
    Policy,
    /// 事業の進捗
    Project,
    /// 選手の成長・移籍・引退
    Person,
    /// 経済や生活の問題
    Economy,
    /// 大会の結果
    Cup,
    /// 地域の話題
    Local,
    /// くだらない日常記事
    Trivial,
    /// 予算編成
    Budget,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Article {
    pub date: Date,
    pub kind: ArticleKind,
    pub headline: String,
    pub body: String,
    /// 見出しからの遷移先（FR-NEWS-03）
    pub subjects: Vec<Subject>,
    /// 続報のスレッド（FR-NEWS-07）
    pub thread: Option<ThreadId>,
    /// 重要度。政策終了時に残す記事の選別に使う（FR-NEWS-06）
    pub weight: u8,
    pub pinned: bool,
    /// ③「なぜ」に対応する内訳（FR-UI-02）。stats の内訳レコードから引き写す。
    pub because: Vec<String>,
    /// 関連政策カードへの直リンク（FR-NEG-02 / FR-UI-02 ③）
    pub remedies: Vec<String>,
}

/// スレッドの見出し。続報を1本の線として辿るために持つ。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Thread {
    pub id: ThreadId,
    pub topic: Subject,
    pub label: String,
    pub opened: Date,
    pub closed: Option<Date>,
    /// 記事の添字
    pub articles: Vec<u32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NewsLog {
    pub articles: Vec<Article>,
    pub threads: Vec<Thread>,
}

impl NewsLog {
    pub fn push(&mut self, article: Article) -> u32 {
        let idx = self.articles.len() as u32;
        if let Some(tid) = article.thread
            && let Some(t) = self.threads.iter_mut().find(|t| t.id == tid)
        {
            t.articles.push(idx);
        }
        self.articles.push(article);
        idx
    }

    /// スレッドを開く（同じ話題が既にあればそれを返す）。
    pub fn open_thread(&mut self, topic: Subject, label: String, date: Date) -> ThreadId {
        if let Some(t) = self.threads.iter().find(|t| t.topic == topic && t.closed.is_none()) {
            return t.id;
        }
        let id = ThreadId(self.threads.len() as u16);
        self.threads.push(Thread { id, topic, label, opened: date, closed: None, articles: Vec::new() });
        id
    }

    pub fn close_thread(&mut self, id: ThreadId, date: Date) {
        if let Some(t) = self.threads.iter_mut().find(|t| t.id == id) {
            t.closed = Some(date);
        }
    }

    /// 日付・対象・種別での検索（FR-NEWS-04）。
    pub fn search(
        &self,
        from: Option<Date>,
        to: Option<Date>,
        kind: Option<ArticleKind>,
        subject: Option<Subject>,
    ) -> Vec<&Article> {
        self.articles
            .iter()
            .filter(|a| from.is_none_or(|d| a.date >= d))
            .filter(|a| to.is_none_or(|d| a.date <= d))
            .filter(|a| kind.is_none_or(|k| a.kind == k))
            .filter(|a| subject.is_none_or(|s| a.subjects.contains(&s)))
            .collect()
    }

    /// ある期間の主要記事（FR-NEWS-06: 政策の進行終了時に残す）。
    pub fn highlights(&self, from: Date, to: Date, min_weight: u8) -> Vec<&Article> {
        let mut v: Vec<&Article> = self
            .articles
            .iter()
            .filter(|a| a.date >= from && a.date <= to && (a.weight >= min_weight || a.pinned))
            .collect();
        v.sort_by(|a, b| b.weight.cmp(&a.weight).then(a.date.cmp(&b.date)));
        v
    }

    pub fn thread_of(&self, id: ThreadId) -> Option<&Thread> {
        self.threads.iter().find(|t| t.id == id)
    }

    /// ピン留め（FR-NEWS-05）。
    pub fn set_pinned(&mut self, index: u32, pinned: bool) {
        if let Some(a) = self.articles.get_mut(index as usize) {
            a.pinned = pinned;
        }
    }
}

/// 記事を組み立てるヘルパ。`text` はテンプレート表。
pub struct Builder<'a> {
    pub text: &'a super::defs::TextTable,
}

impl<'a> Builder<'a> {
    pub fn article(
        &self,
        date: Date,
        kind: ArticleKind,
        key: &str,
        args: &[(&str, &str)],
        weight: u8,
    ) -> Article {
        Article {
            date,
            kind,
            headline: self.text.format(&format!("{key}.headline"), args),
            body: self.text.format(&format!("{key}.body"), args),
            subjects: Vec::new(),
            thread: None,
            weight,
            pinned: false,
            because: Vec::new(),
            remedies: Vec::new(),
        }
    }
}

// ───────────────────────────── ⑥ 日次の記事生成 ─────────────────────────────

use super::defs::Defs;
use super::ids::{DistrictId, PersonId};
use super::stats::DailyStats;
use super::world::PersonStatus;
use super::{Game, events, ids::Target, rng};
use std::sync::Arc;

/// その日のニュースを生成する。
///
/// イベントの記事は `events::apply` が出すので、ここが扱うのは
/// 事業の進捗・人物の節目・地域の話題・どうでもいい日常記事（FR-NEWS-02）。
pub fn generate(game: &mut Game, _fired: &[events::Fired], today: &DailyStats) {
    person_milestones(game);
    local_topics(game, today);
    trivia(game);
}

fn person_milestones(game: &mut Game) {
    let defs: Arc<Defs> = Arc::clone(&game.defs);
    let mut items: Vec<(PersonId, &'static str, u8)> = Vec::new();

    for (i, p) in game.world.people.iter().enumerate() {
        let id = PersonId::from_index(i);
        if p.status != p.reported_status {
            let key = match p.status {
                PersonStatus::Retired => "person.retired",
                PersonStatus::Paused => "person.paused",
                PersonStatus::Emigrated => "person.emigrated",
                PersonStatus::Active => "person.returned",
            };
            items.push((id, key, 7));
            continue;
        }
        let a = p.life.ability.value;
        let level = if a >= 80.0 {
            3
        } else if a >= 60.0 {
            2
        } else if a >= 40.0 {
            1
        } else {
            0
        };
        if level > p.milestone {
            items.push((id, "person.milestone", 6));
        }
    }

    for (id, key, weight) in items {
        let (name, place, level, status) = {
            let p = &game.world.people[id.index()];
            let a = p.life.ability.value;
            let level = if a >= 80.0 {
                3
            } else if a >= 60.0 {
                2
            } else if a >= 40.0 {
                1
            } else {
                0
            };
            (
                p.name.clone(),
                game.world.district(p.home).name.clone(),
                level,
                p.status,
            )
        };
        let b = Builder { text: &defs.text };
        let ability = format!("{:.0}", game.world.people[id.index()].life.ability.value);
        let mut a = b.article(
            game.date,
            ArticleKind::Person,
            key,
            &[("name", &name), ("place", &place), ("ability", &ability)],
            weight,
        );
        a.subjects.push(Subject::Person(id));
        // その人物のスレッドへ束ねる（続報: FR-NEWS-07）
        let thread = game.news.open_thread(Subject::Person(id), name.clone(), game.date);
        a.thread = Some(thread);
        game.news.push(a);

        let p = &mut game.world.people[id.index()];
        p.milestone = level;
        p.reported_status = status;
        game.history_note(Target::Person(id), game.date, "history.milestone", &ability);
    }
}

fn local_topics(game: &mut Game, today: &DailyStats) {
    // 5日に一度、もっとも参加率が伸びた地区を取り上げる
    if game.date.doy % 5 != 0 {
        return;
    }
    let Some(prev) = game.stats.days.len().checked_sub(6).and_then(|i| game.stats.days.get(i))
    else {
        return;
    };
    let mut best: Option<(u16, f32)> = None;
    for d in &today.by_district {
        let before = prev
            .by_district
            .iter()
            .find(|x| x.district == d.district)
            .map(|x| x.participation)
            .unwrap_or(d.participation);
        let delta = d.participation - before;
        if best.is_none_or(|(_, v)| delta > v) {
            best = Some((d.district, delta));
        }
    }
    let Some((did, delta)) = best else { return };
    if delta.abs() < 0.01 {
        return;
    }
    let defs: Arc<Defs> = Arc::clone(&game.defs);
    let id = DistrictId(did);
    let place = game.world.district(id).name.clone();
    let key = if delta > 0.0 { "local.participation_up" } else { "local.participation_down" };
    let b = Builder { text: &defs.text };
    let pct = format!("{:.0}", delta.abs() * 100.0);
    let mut a = b.article(game.date, ArticleKind::Local, key, &[("place", &place), ("pct", &pct)], 4);
    a.subjects.push(Subject::District(id));
    game.news.push(a);
}

/// くだらない日常記事（FR-NEWS-02）。
/// 重大性と報道の大仰さが一致しない記事を混ぜる（FR-NEWS-09）: weight は低いが公文書名を使う。
fn trivia(game: &mut Game) {
    if !rng::chance(&mut game.rng.daily, 0.22) {
        return;
    }
    let defs: Arc<Defs> = Arc::clone(&game.defs);
    if defs.policies.is_empty() || game.world.districts.is_empty() {
        return;
    }
    let pi = rng::range(&mut game.rng.daily, 0.0, defs.policies.len() as f32) as usize;
    let di = rng::range(&mut game.rng.daily, 0.0, game.world.districts.len() as f32) as usize;
    let policy = &defs.policies[pi.min(defs.policies.len() - 1)];
    let place = game.world.districts[di.min(game.world.districts.len() - 1)].name.clone();
    let b = Builder { text: &defs.text };
    let mut a = b.article(
        game.date,
        ArticleKind::Trivial,
        "trivial.daily",
        &[("official", &policy.official_name), ("name", &policy.name), ("place", &place)],
        1,
    );
    a.subjects.push(Subject::District(DistrictId::from_index(di.min(game.world.districts.len() - 1))));
    game.news.push(a);
}
