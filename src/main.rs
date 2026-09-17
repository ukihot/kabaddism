//! App 構築（design.md §2 / §3）

mod app;

use bevy::prelude::*;

fn main() {
    let mut a = App::new();
    a.add_plugins(DefaultPlugins).add_plugins(app::KabaddismPlugin);
    if let Some(seed) = seed_arg() {
        a.insert_resource(app::Seed(seed));
    }
    a.run();
}

/// `--seed N` だけ見る。ほかの引数はヘッドレスのハーネス側にある。
fn seed_arg() -> Option<u64> {
    let argv: Vec<String> = std::env::args().collect();
    let i = argv.iter().position(|s| s == "--seed")?;
    argv.get(i + 1)?.parse().ok()
}
