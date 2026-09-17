//! ヘッドレスのプレイハーネス（design.md §15）
//!
//! bevy を一切構築せずに1年が回る。バランス調整はこちらで回すほうが速い。
//! `cargo run --bin harness -- --seed 42 --years 1`

use std::sync::Arc;

use kbism::sim::calendar::StopReason;
use kbism::sim::defs::Defs;
use kbism::sim::{Game, policy};

struct Args {
    seed: u64,
    years: u16,
    quiet: bool,
}

fn parse_args() -> Args {
    let mut a = Args { seed: 42, years: 1, quiet: false };
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--seed" => {
                i += 1;
                a.seed = argv.get(i).and_then(|s| s.parse().ok()).unwrap_or(a.seed);
            }
            "--years" => {
                i += 1;
                a.years = argv.get(i).and_then(|s| s.parse().ok()).unwrap_or(a.years);
            }
            "--quiet" => a.quiet = true,
            _ => {}
        }
        i += 1;
    }
    a
}

fn main() {
    let args = parse_args();
    let defs = match Defs::load_default() {
        Ok(d) => Arc::new(d),
        Err(e) => {
            eprintln!("データの読み込みに失敗しました: {e}");
            std::process::exit(1);
        }
    };

    let mut game = Game::new(args.seed, Arc::clone(&defs));
    println!(
        "国: {} / 人口 {} / 地区 {} / 追跡人物 {}",
        defs.scenario.nation_name,
        game.world.population(),
        game.world.districts.len(),
        game.world.people.len()
    );

    let end_year = game.date.year + args.years;
    let mut executed = 0usize;

    while game.date.year < end_year {
        // 進行中でなければ、推奨枠から1枚選んで実行する
        if game.pending_days == 0 {
            let picks = game.recommended_policies(6);
            let chosen = picks.into_iter().find(|(id, target, _)| {
                defs.policy(id)
                    .map(|d| policy::check(&game, d, *target).is_empty())
                    .unwrap_or(false)
            });
            match chosen {
                Some((id, target, issue)) => {
                    if game.execute_policy(&id, target).is_ok() {
                        executed += 1;
                        if !args.quiet {
                            let name = defs.policy(&id).map(|d| d.name.as_str()).unwrap_or(&id);
                            println!(
                                "{} 実行: {} → {}{}",
                                game.date,
                                name,
                                policy::target_name(&game, target),
                                if issue.is_empty() { String::new() } else { format!("（{issue} への対応）") }
                            );
                        }
                    } else {
                        game.pending_days = 1;
                    }
                }
                None => game.pending_days = 1,
            }
        }

        match game.advance_all() {
            Some(StopReason::Budget) => {
                let brief = game.budget_briefing();
                let plan = brief.default_plan();
                if !args.quiet {
                    println!(
                        "{} 予算編成: 収入見込 {:.0}K / 継続費用 {:.0}K / 配分可能 {:.0}K",
                        game.date,
                        brief.revenue_estimate,
                        brief.committed_upkeep,
                        brief.allocatable()
                    );
                }
                game.apply_budget(plan);
            }
            Some(StopReason::Cup) => {
                let preview = game.cup_prepare();
                if !args.quiet {
                    println!(
                        "{} 世界大会: 代表{}名 / 戦力 {:.1} / 賭ける領地 {} / 最大損失 {:.0}K",
                        game.date,
                        preview.squad.len(),
                        preview.squad_strength,
                        preview
                            .staked
                            .iter()
                            .map(|d| game.world.district(*d).name.clone())
                            .collect::<Vec<_>>()
                            .join("・"),
                        preview.max_loss
                    );
                }
                let result = game.cup_run();
                if !args.quiet {
                    println!(
                        "{} 結果: {}位 / 獲得 {} / 喪失 {} / 実損 {:.0}K",
                        game.date,
                        result.home_rank + 1,
                        result.gained.len(),
                        result.lost.len(),
                        result.realized_loss
                    );
                }
            }
            None => {}
        }

        if !args.quiet && game.date.doy % 30 == 0 {
            print_status(&game);
        }
    }

    println!("\n── {}年の終わり ──", end_year - 1);
    print_status(&game);
    println!("実行した政策: {executed} 件 / 記事 {} 本 / 問題 {} 件（未解決 {}）",
        game.news.articles.len(),
        game.issues.len(),
        game.issues.iter().filter(|i| i.closed.is_none()).count(),
    );

    if let Some(t) = game.stats.today() {
        println!(
            "GDP {:.0}K / 一人あたり {:.2}K / 平均能力 {:.1} / 参加率 {:.2} / 受取集中度 {:.2}",
            t.gdp, t.gdp_per_capita, t.mean_ability, t.participation, t.receipt_gini
        );
    }
}

fn print_status(game: &Game) {
    let Some(t) = game.stats.today() else { return };
    println!(
        "{} 人口{} GDP{:.0}K 平均能力{:.1} 参加率{:.2} 生活余力{:.2} 練習{:.0}分 疲労{:.2} 予算残{:.0}K 大会まで{}日",
        game.date,
        t.population,
        t.gdp,
        t.mean_ability,
        t.participation,
        t.life_slack,
        t.practice_minutes,
        t.fatigue,
        t.budget_available,
        t.days_to_cup
    );
}
