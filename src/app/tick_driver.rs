//! `pending_days` の消化（design.md §3.4 / NFR-03）
//!
//! `sim` を書き換えるのはここと、政策実行・予算確定・大会実行の各ボタンだけ。
//! 60日を1フレームで回すとウィンドウが固まるので、フレーム予算で分割する。

use std::time::{Duration, Instant};

use bevy::prelude::*;

use kbism::sim::calendar::StopReason;

use super::GameRes;
use super::state::Phase;

/// 1フレームあたりに `sim` へ使ってよい時間。
const FRAME_BUDGET: Duration = Duration::from_millis(6);

pub fn run_pending_days(mut game: ResMut<GameRes>, mut next: ResMut<NextState<Phase>>) {
    if game.pending_days == 0 {
        next.set(Phase::Planning);
        return;
    }
    let deadline = Instant::now() + FRAME_BUDGET;
    while game.pending_days > 0 && Instant::now() < deadline {
        match game.advance_one() {
            Some(StopReason::Budget) => {
                next.set(Phase::Budget);
                return;
            }
            Some(StopReason::Cup) => {
                next.set(Phase::Cup);
                return;
            }
            None => {}
        }
    }
    if game.pending_days == 0 {
        next.set(Phase::Planning);
    }
}
