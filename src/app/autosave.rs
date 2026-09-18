//! オートセーブ（design.md §14）
//!
//! 年次イベント直前と政策実行直前の2箇所だけで呼ぶ。スロットはローテーション3枠。
//! `sim::save` が探索・書き込み・保存先の決定を持っているので、ここは呼び出すだけ。

use bevy::prelude::*;

use kbism::sim::save;

use super::GameRes;

/// 何回目のオートセーブか。3枠のローテーションはこの型が決める。
#[derive(Resource, Default)]
pub struct AutosaveCounter(u32);

impl AutosaveCounter {
    /// 次に書くスロット名を返し、カウンタを進める。
    fn next_slot(&mut self) -> String {
        let slot = save::autosave_slot(self.0);
        self.0 += 1;
        slot
    }
}

pub fn run(game: &GameRes, counter: &mut AutosaveCounter) {
    let path = save::slot_path(&counter.next_slot());
    if let Err(e) = save::write_to(&path, game) {
        error!("オートセーブに失敗しました: {e}");
    }
}

/// 予算編成・世界大会の直前に呼ぶ（`OnEnter(Phase::Budget)` / `OnEnter(Phase::Cup)`）。
pub fn on_enter_modal(game: Res<GameRes>, mut counter: ResMut<AutosaveCounter>) {
    run(&game, &mut counter);
}
