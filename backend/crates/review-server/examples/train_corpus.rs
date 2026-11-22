//! Drill counts over stored reviews: `cargo run --example train_corpus -- <dir> [--endgame]
//! [--sample N] [--kind endgame]`.
//!
//! Reads every `*.json` in `<dir>` (a finished `ReviewResult`, or a calibration record with the
//! review under `result`), makes its drills with the current rules and prints how many of each
//! kind and theme there are, then a sample of drills to check by hand. No engine is needed.

use std::collections::BTreeMap;
use std::path::Path;

use review_core::TrainConfig;
use review_server::analysis::model::ReviewResult;
use review_server::train::{self, TrainItem, TrainKind};

fn load(path: &Path) -> Option<ReviewResult> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let review = value.get("result").cloned().unwrap_or(value);
    match serde_json::from_value(review) {
        Ok(r) => Some(r),
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            None
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(dir) = args.first() else {
        eprintln!("usage: train_corpus <dir> [--endgame] [--sample N] [--kind <kind>]");
        std::process::exit(2);
    };
    let flag = |name: &str| args.iter().any(|a| a == name);
    let value = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let cfg = TrainConfig {
        endgame_maker: flag("--endgame"),
        ..TrainConfig::default()
    };
    let sample: usize = value("--sample").and_then(|s| s.parse().ok()).unwrap_or(0);
    let kind_filter = value("--kind");

    let mut files: Vec<_> = std::fs::read_dir(dir)
        .expect("a readable directory")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();

    let mut games = 0usize;
    let mut items: Vec<(String, TrainItem)> = Vec::new();
    let started = std::time::Instant::now();
    for path in &files {
        let Some(review) = load(path) else { continue };
        games += 1;
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        items.extend(
            train::generate(&review, &cfg)
                .into_iter()
                .map(|it| (name.clone(), it)),
        );
    }
    let elapsed = started.elapsed();

    let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_theme: BTreeMap<String, usize> = BTreeMap::new();
    for (_, it) in &items {
        *by_kind.entry(format!("{:?}", it.kind)).or_default() += 1;
        *by_theme
            .entry(format!("{:?}/{:?}", it.kind, it.theme))
            .or_default() += 1;
    }
    println!(
        "games {games}, drills {} ({:.2} per game), {:.2} ms per game",
        items.len(),
        items.len() as f64 / games.max(1) as f64,
        elapsed.as_secs_f64() * 1000.0 / games.max(1) as f64
    );
    for (k, n) in &by_kind {
        println!("  {k:<10} {n}");
    }
    for (k, n) in &by_theme {
        println!("    {k:<40} {n}");
    }

    if sample > 0 {
        let pool: Vec<&(String, TrainItem)> = items
            .iter()
            .filter(|(_, it)| {
                kind_filter
                    .as_deref()
                    .is_none_or(|k| train::text::key(it.kind) == k)
            })
            .collect();
        // A fixed stride through the pool: the same sample every run.
        let step = pool.len().checked_div(sample).unwrap_or(1).max(1);
        for (game, it) in pool.iter().step_by(step).take(sample) {
            println!(
                "\n{game} ply {} {:?}/{:?} {:?} lost {:.2}\n  {}\n  played {} best {}  line {}\n  family {:?} concept {:?} known {:?} play-out {:?}\n  goal: {}\n  hint: {}",
                it.source.ply,
                it.kind,
                it.theme,
                it.classification,
                it.ep_lost,
                it.start_fen,
                it.played_move.san,
                it.correct_move.san,
                it.best_line.join(" "),
                it.endgame_family,
                it.endgame_concept,
                it.endgame_type,
                it.play_out,
                it.goal,
                it.hint,
            );
        }
    }
    let endgame = items
        .iter()
        .filter(|(_, it)| it.kind == TrainKind::Endgame)
        .count();
    println!("\nendgame drills: {endgame}");
}
