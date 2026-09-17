//! 画面（design.md §13）
//!
//! UI は `sim` を**読むだけ**で、書き換えは `tick_driver` と政策実行ボタンに閉じている。
//! 毎フレーム全再構築はせず、`Res<GameRes>` の変更検知で必要な箇所だけ書き換える（§13.3）。

pub mod budget_screen;
pub mod cup_screen;
pub mod news_feed;
pub mod policy_panel;
pub mod top_bar;

use bevy::prelude::*;

use super::GameRes;
use super::state::GameState;

// ───────────────────────── 配色と文字 ─────────────────────────

pub const BG: Color = Color::srgb(0.08, 0.09, 0.11);
pub const PANEL: Color = Color::srgb(0.13, 0.14, 0.17);
pub const PANEL_ALT: Color = Color::srgb(0.17, 0.19, 0.23);
pub const LINE: Color = Color::srgb(0.24, 0.26, 0.31);
pub const INK: Color = Color::srgb(0.88, 0.89, 0.92);
pub const INK_DIM: Color = Color::srgb(0.58, 0.60, 0.66);
pub const ACCENT: Color = Color::srgb(0.45, 0.72, 0.52);
pub const WARN: Color = Color::srgb(0.80, 0.45, 0.38);

/// 日本語を出すため、埋め込みの FiraMono ではなくシステムの sans-serif を引く。
/// （`system_font_discovery` feature が有効なときだけ解決される）
pub fn font(size: f32) -> TextFont {
    TextFont { font: FontSource::SansSerif, font_size: FontSize::Px(size), ..default() }
}

pub fn label(text: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(text.into()), font(size), TextColor(color))
}

/// 押せるボタン。`marker` に押下時の意味を持たせる。
pub fn button(marker: impl Component, enabled: bool) -> impl Bundle {
    (
        Button,
        marker,
        Node {
            padding: UiRect::axes(px(12), px(8)),
            margin: UiRect::bottom(px(6)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(4)),
            flex_direction: FlexDirection::Column,
            row_gap: px(2),
            ..default()
        },
        BorderColor::all(if enabled { LINE } else { PANEL }),
        BackgroundColor(if enabled { PANEL_ALT } else { PANEL }),
    )
}

// ───────────────────────── 骨格 ─────────────────────────

#[derive(Component)]
pub struct TopBar;
#[derive(Component)]
pub struct PolicyPanel;
#[derive(Component)]
pub struct MapArea;
#[derive(Component)]
pub struct NewsFeed;
/// 年次イベントのモーダルを載せる場所。
#[derive(Component)]
pub struct ModalRoot;

/// design.md §13.1 のレイアウト。
pub fn spawn_root(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(GameState::InGame),
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            ..default()
        },
        BackgroundColor(BG),
        children![
            (
                TopBar,
                Node {
                    width: percent(100),
                    padding: UiRect::axes(px(16), px(10)),
                    column_gap: px(24),
                    align_items: AlignItems::Center,
                    border: UiRect::bottom(px(1)),
                    ..default()
                },
                BorderColor::all(LINE),
                BackgroundColor(PANEL),
            ),
            (
                Node { width: percent(100), flex_grow: 1.0, min_height: px(0), ..default() },
                children![
                    (
                        PolicyPanel,
                        Node {
                            width: px(360),
                            min_width: px(300),
                            height: percent(100),
                            flex_direction: FlexDirection::Column,
                            padding: UiRect::all(px(12)),
                            row_gap: px(6),
                            overflow: Overflow::scroll_y(),
                            border: UiRect::right(px(1)),
                            ..default()
                        },
                        BorderColor::all(LINE),
                        BackgroundColor(PANEL),
                    ),
                    (
                        MapArea,
                        Node {
                            flex_grow: 1.0,
                            height: percent(100),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                    ),
                ],
            ),
            (
                NewsFeed,
                Node {
                    width: percent(100),
                    height: px(190),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(px(12)),
                    row_gap: px(4),
                    overflow: Overflow::scroll_y(),
                    border: UiRect::top(px(1)),
                    ..default()
                },
                BorderColor::all(LINE),
                BackgroundColor(PANEL),
            ),
            (
                ModalRoot,
                Node {
                    position_type: PositionType::Absolute,
                    width: percent(100),
                    height: percent(100),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
            ),
        ],
    ));
}

/// 地図はフェーズ8。それまでは場所だけ取っておく。
pub fn spawn_map_placeholder(
    mut commands: Commands,
    area: Single<Entity, With<MapArea>>,
    game: Res<GameRes>,
) {
    let text = game.defs.text.get("ui.map.placeholder").to_string();
    commands.entity(*area).with_children(|p| {
        p.spawn(label(text, 16.0, INK_DIM));
    });
}

// ───────────────────────── 年次イベントのモーダル ─────────────────────────

/// 予算編成・世界大会の共通の枠。
pub fn modal() -> impl Bundle {
    (
        Node {
            min_width: px(520),
            max_width: px(760),
            flex_direction: FlexDirection::Column,
            row_gap: px(12),
            padding: UiRect::all(px(24)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(6)),
            ..default()
        },
        BorderColor::all(LINE),
        BackgroundColor(PANEL_ALT),
    )
}

/// モーダルを閉じたあとの行き先。残日数があれば進行を再開する（FR-TIME-06）。
pub fn resume_phase(game: &GameRes) -> super::state::Phase {
    if game.pending_days > 0 {
        super::state::Phase::Advancing
    } else {
        super::state::Phase::Planning
    }
}
