//! 下部ニュース（FR-UI-01 / §11.2）
//!
//! 記事が増えたときだけ組み直す。表示は上限つきで、古いものは履歴画面（フェーズ8）へ回す。

use bevy::prelude::*;

use super::super::GameRes;
use super::inspector::OpenSubject;
use super::{ACCENT, INK, INK_DIM, NewsFeed, label};

/// 一度に見せる本数。これを超える分は履歴から探す（§13.3）。
const SHOWN: usize = 12;

pub fn sync(
    mut commands: Commands,
    feed: Single<Entity, With<NewsFeed>>,
    game: Res<GameRes>,
    mut last: Local<usize>,
) {
    let n = game.news.articles.len();
    if !game.is_changed() || (n == *last && n != 0) {
        return;
    }
    *last = n;

    let heading = game.defs.text.get("ui.news.heading").to_string();
    let rows: Vec<(String, String, bool, Option<OpenSubject>)> = game
        .news
        .articles
        .iter()
        .rev()
        .take(SHOWN)
        .map(|a| {
            (
                format!("{}　{}", a.date, a.headline),
                a.body.clone(),
                a.pinned || a.weight >= 6,
                a.subjects.first().map(|s| OpenSubject(*s)),
            )
        })
        .collect();
    let empty = game.defs.text.get("ui.news.empty").to_string();

    commands.entity(*feed).despawn_related::<Children>().with_children(|p| {
        p.spawn(label(heading, 11.0, INK_DIM));
        if rows.is_empty() {
            p.spawn(label(empty, 13.0, INK_DIM));
        }
        for (head, body, major, subject) in rows {
            let row = Node {
                flex_direction: FlexDirection::Column,
                margin: UiRect::bottom(px(3)),
                ..default()
            };
            let mut spawned = match subject {
                Some(s) => p.spawn((row, Button, s)),
                None => p.spawn(row),
            };
            spawned.with_children(|c| {
                c.spawn(label(head, 14.0, if major { ACCENT } else { INK }));
                c.spawn(label(body, 12.0, INK_DIM));
            });
        }
    });
}
