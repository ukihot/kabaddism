//! bevy 側（design.md §1.1 の app 層）
//!
//! `sim` を**読むだけ**。書き換えるのは [`tick_driver::run_pending_days`] と、
//! プレイヤー操作を表す少数のボタンハンドラのみ。書き込み競合は原理的に起きない。

pub mod state;
pub mod tick_driver;
pub mod title;
pub mod ui;

use std::sync::Arc;

use bevy::prelude::*;

use kbism::sim::Game;
use kbism::sim::defs::Defs;

use state::{GameState, Phase};

/// `Game` は bevy を知らないので、Resource にするのは app 側の包み紙の仕事。
#[derive(Resource, Deref, DerefMut)]
pub struct GameRes(pub Game);

/// 起動時のシード。コマンドライン `--seed` で上書きする。
#[derive(Resource)]
pub struct Seed(pub u64);

pub struct KabaddismPlugin;

impl Plugin for KabaddismPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GameState>()
            .add_sub_state::<Phase>()
            .add_systems(Startup, setup_camera)
            .add_systems(OnEnter(GameState::Boot), boot)
            .add_systems(OnEnter(GameState::Title), title::spawn)
            .add_systems(Update, title::click.run_if(in_state(GameState::Title)))
            // 画面の骨格 → 各パネルの中身、の順に組む
            .add_systems(
                OnEnter(GameState::InGame),
                (ui::spawn_root, (ui::top_bar::spawn, ui::spawn_map_placeholder)).chain(),
            )
            .add_systems(OnEnter(Phase::Planning), ui::policy_panel::spawn)
            .add_systems(OnEnter(Phase::Advancing), ui::policy_panel::spawn_advancing)
            .add_systems(OnEnter(Phase::Budget), ui::budget_screen::spawn)
            .add_systems(OnEnter(Phase::Cup), ui::cup_screen::spawn)
            .add_systems(
                Update,
                (
                    ui::policy_panel::click.run_if(in_state(Phase::Planning)),
                    tick_driver::run_pending_days.run_if(in_state(Phase::Advancing)),
                    ui::budget_screen::click.run_if(in_state(Phase::Budget)),
                    (ui::cup_screen::click_run, ui::cup_screen::click_close)
                        .run_if(in_state(Phase::Cup)),
                    // sim → 表示の一方向同期（§13.3）
                    (ui::top_bar::sync, ui::news_feed::sync),
                )
                    .chain()
                    .run_if(in_state(GameState::InGame)),
            );
    }
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

/// データを読んで世界を作る。`Defs` の探索は `sim` 側が面倒を見る。
fn boot(mut commands: Commands, seed: Option<Res<Seed>>, mut next: ResMut<NextState<GameState>>) {
    let defs = match Defs::load_default() {
        Ok(d) => Arc::new(d),
        Err(e) => {
            error!("データの読み込みに失敗しました: {e}");
            std::process::exit(1);
        }
    };
    let seed = seed.map(|s| s.0).unwrap_or(42);
    commands.insert_resource(GameRes(Game::new(seed, defs)));
    next.set(GameState::Title);
}
