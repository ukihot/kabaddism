//! 画面状態（design.md §3.2）
//!
//! 時間はプレイヤー操作でのみ進む。`Planning` は時間が止まっている状態で、
//! 政策カードの実行だけが `Advancing` への入口になる（FR-TIME-02）。

use bevy::prelude::*;

#[derive(States, Default, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum GameState {
    #[default]
    Boot,
    Title,
    InGame,
}

/// `InGame` の中だけに存在する進行フェーズ。
#[derive(SubStates, Default, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[source(GameState = GameState::InGame)]
pub enum Phase {
    /// 政策カード選択。時間は止まっている。
    #[default]
    Planning,
    /// `pending_days` を消化中。
    Advancing,
    /// 年次予算編成（モーダル）。
    Budget,
    /// 世界大会（モーダル）。
    Cup,
}
