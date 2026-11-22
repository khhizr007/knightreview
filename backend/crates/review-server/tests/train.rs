//! The trainer: drills made from reviews (`train::generate`) and the grading of drill moves
//! (`train::grade`).

mod common;

use std::sync::Arc;

use common::FakeEngine;
use review_core::{Classification, Eval, ReviewConfig, Side, TrainConfig};
use review_server::analysis::model::ReviewResult;
use review_server::analysis::{ReviewRequest, run_review};
use review_server::engine::{Evaluator, SearchLimit};
use review_server::openings::OpeningBook;
use review_server::pgn::parse_pgn;
use review_server::train::grade::{TrainGrade, TrainGradeRequest, grade_move, grade_rule};
use review_server::train::{self, TrainBucket, TrainItem, TrainKind, TrainLine, TrainTheme};
use shakmaty::fen::Fen;
use shakmaty::{CastlingMode, Chess, Position};
use tokio_util::sync::CancellationToken;

const FIXTURES: [&str; 5] = [
    "missed_mate.pgn",
    "scholars_mate.pgn",
    "opera_game.pgn",
    "back_rank.pgn",
    "special_moves.pgn",
];

async fn review(fixture: &str, player_side: Option<Side>, config: ReviewConfig) -> ReviewResult {
    let game = parse_pgn(&common::fixture(fixture))
        .unwrap()
        .remove(0)
        .unwrap();
    let engine: Arc<dyn Evaluator> = Arc::new(FakeEngine::default());
    run_review(
        ReviewRequest {
            game,
            limit: SearchLimit::Depth { depth: 1 },
            multipv: 3,
            config,
            player_side,
        },
        engine,
        Arc::new(OpeningBook::empty()),
        CancellationToken::new(),
        Arc::new(|_, _| {}),
    )
    .await
    .unwrap()
}

async fn all_items() -> Vec<TrainItem> {
    let mut out = Vec::new();
    for f in FIXTURES {
        out.extend(review(f, None, ReviewConfig::default()).await.train_items);
    }
    out
}

fn pos(fen: &str) -> Chess {
    fen.parse::<Fen>()
        .unwrap()
        .into_position(CastlingMode::Standard)
        .unwrap()
}

#[tokio::test]
async fn reviews_carry_drills_that_start_one_move_early() {
    let items = all_items().await;
    assert!(!items.is_empty(), "the fixtures give at least one drill");
    for it in &items {
        // The pitch move leads from the pre-pitch position to the start position.
        let pre = pos(&it.pre_pitch_fen);
        let m = review_server::board::parse_uci(&pre, &it.pitch_move.uci).expect("legal pitch");
        let mut after = pre.clone();
        after.play_unchecked(m);
        assert_eq!(
            review_server::board::epd(&after),
            review_server::board::epd(&pos(&it.start_fen))
        );
        // The player is to move at the start, and both moves are legal there.
        let start = pos(&it.start_fen);
        assert_eq!(review_server::board::side(start.turn()), it.user_side);
        assert!(review_server::board::parse_uci(&start, &it.correct_move.uci).is_some());
        assert!(review_server::board::parse_uci(&start, &it.played_move.uci).is_some());
        assert_ne!(it.correct_move.uci, it.played_move.uci);
        assert_eq!(it.best_line.first(), Some(&it.correct_move.uci));
        assert_eq!(it.played_line.first(), Some(&it.played_move.uci));
        assert!(!it.goal.is_empty() && !it.hint.is_empty());
        assert!(
            matches!(
                it.classification,
                Classification::Mistake | Classification::Blunder | Classification::Miss
            ) || it.kind == TrainKind::Endgame
        );
        if matches!(it.kind, TrainKind::Miss | TrainKind::Blunder) {
            assert!(it.bucket.is_some());
        }
        assert_eq!(it.generator_version, train::GENERATOR_VERSION);
    }
}

#[tokio::test]
async fn the_same_game_gives_the_same_drill_ids() {
    let a = review("missed_mate.pgn", None, ReviewConfig::default()).await;
    let b = review("missed_mate.pgn", None, ReviewConfig::default()).await;
    let ids = |r: &ReviewResult| {
        r.train_items
            .iter()
            .map(|i| i.id.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&a), ids(&b));
    // Regenerating from the stored review gives the same drills.
    assert_eq!(train::generate(&a, &a.config.train), a.train_items);
}

#[tokio::test]
async fn only_the_players_moves_and_at_most_the_cap() {
    for f in FIXTURES {
        for side in [Side::White, Side::Black] {
            let r = review(f, Some(side), ReviewConfig::default()).await;
            assert!(r.train_items.iter().all(|it| it.user_side == side));
        }
        let mut cfg = ReviewConfig::default();
        cfg.train.max_per_game = 1;
        let r = review(f, None, cfg).await;
        for side in [Side::White, Side::Black] {
            assert!(r.train_items.iter().filter(|i| i.user_side == side).count() <= 1);
        }
    }
    let mut off = ReviewConfig::default();
    off.train.enabled = false;
    assert!(
        review("missed_mate.pgn", None, off)
            .await
            .train_items
            .is_empty()
    );
}

#[tokio::test]
async fn reviews_round_trip_with_their_drills_and_old_reviews_have_none() {
    let r = review("missed_mate.pgn", None, ReviewConfig::default()).await;
    let json = serde_json::to_value(&r).unwrap();
    let back: ReviewResult = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(back.train_items, r.train_items);
    let mut old = json;
    old.as_object_mut().unwrap().remove("train_items");
    let old: ReviewResult = serde_json::from_value(old).unwrap();
    assert!(old.train_items.is_empty());
}

fn request(item: &TrainItem, user_move: &str, after: Option<TrainLine>) -> TrainGradeRequest {
    TrainGradeRequest {
        item: item.clone(),
        user_move: user_move.to_owned(),
        before: None,
        after,
    }
}

#[tokio::test]
async fn grading_the_game_move_the_best_move_and_without_an_engine() {
    let items = all_items().await;
    let it = &items[0];
    let book = OpeningBook::empty();
    let cfg = ReviewConfig::default();

    let same = grade_move(&request(it, &it.played_move.uci, None), &book, &cfg).unwrap();
    assert_eq!(same.grade, TrainGrade::SameAsGame);
    assert_eq!(same.show_line, it.played_line);

    let best = grade_move(&request(it, &it.correct_move.uci, None), &book, &cfg).unwrap();
    assert_eq!(best.grade, TrainGrade::Correct);
    assert!(!best.fallback, "the best move is in the drill's own lines");
    assert_eq!(best.show_line, it.best_line);

    // A move with no line of its own and no search after it: only the best move counts.
    let start = pos(&it.start_fen);
    let other = start
        .legal_moves()
        .into_iter()
        .map(|m| review_server::board::uci(m, CastlingMode::Standard))
        .find(|u| {
            *u != it.correct_move.uci
                && *u != it.played_move.uci
                && it.lines.iter().all(|l| l.pv.first() != Some(u))
        });
    if let Some(other) = other {
        let g = grade_move(&request(it, &other, None), &book, &cfg).unwrap();
        assert!(g.fallback);
        assert_eq!(g.grade, TrainGrade::Incorrect);
        // With a line after it, the move is classified.
        let after = TrainLine {
            eval: it.lines[0].eval,
            depth: 1,
            pv: Vec::new(),
        };
        let g = grade_move(&request(it, &other, Some(after)), &book, &cfg).unwrap();
        assert!(!g.fallback);
        assert!(g.classification.is_some());
    }

    let illegal = grade_move(&request(it, "a1a8", None), &book, &cfg);
    assert!(illegal.is_err());
}

fn synthetic(kind: TrainKind, bucket: Option<TrainBucket>, theme: TrainTheme) -> TrainItem {
    let start = "4k3/8/8/8/8/8/8/4K2R w K - 0 1".to_owned();
    let m = |uci: &str| review_server::analysis::model::MoveRef {
        san: uci.to_owned(),
        uci: uci.to_owned(),
        fen: start.clone(),
    };
    TrainItem {
        id: "x".to_owned(),
        kind,
        theme,
        bucket,
        user_side: Side::White,
        pre_pitch_fen: start.clone(),
        pitch_move: m("e8e7"),
        start_fen: start.clone(),
        correct_move: m("h1h8"),
        played_move: m("e1e2"),
        best_line: vec!["h1h8".to_owned()],
        played_line: vec!["e1e2".to_owned()],
        lines: vec![TrainLine {
            eval: Eval::Cp { cp: 500 },
            depth: 1,
            pv: vec!["h1h8".to_owned()],
        }],
        classification: Classification::Blunder,
        miss_kind: None,
        phase: review_core::Phase::Endgame,
        is_critical: false,
        position_type: None,
        eco: None,
        opening_name: None,
        endgame_family: None,
        endgame_concept: None,
        endgame_type: None,
        play_out: None,
        ep_lost: 0.3,
        context: train::TrainContext {
            rating: 1500,
            highest_elo: 1500,
            previous_ep_lost: 0.0,
            previous_erred: false,
        },
        goal: String::new(),
        hint: String::new(),
        source: train::TrainSource {
            ply: 2,
            move_number: 1,
            white: None,
            black: None,
            opponent: None,
            date: None,
            site: None,
            event: None,
            result: None,
            time_control: None,
        },
        generator_version: 1,
    }
}

#[test]
fn the_grade_rule_branches() {
    use Classification as C;
    let tactic = synthetic(
        TrainKind::Miss,
        Some(TrainBucket::Tactics),
        TrainTheme::Fork,
    );
    assert_eq!(
        grade_rule(&tactic, "e1e2", Some(C::Best), &[]).0,
        TrainGrade::SameAsGame
    );
    assert_eq!(
        grade_rule(&tactic, "h1h8", Some(C::Best), &[]).0,
        TrainGrade::Correct
    );
    // A good move without the fork is fine, not the idea.
    assert_eq!(
        grade_rule(&tactic, "h1h7", Some(C::Good), &[]).0,
        TrainGrade::NotIncorrect
    );
    // Inaccuracy or worse is wrong.
    assert_eq!(
        grade_rule(&tactic, "h1h7", Some(C::Inaccuracy), &[]).0,
        TrainGrade::Incorrect
    );
    // In a critical position only the best move works.
    let mut critical = tactic.clone();
    critical.is_critical = true;
    assert_eq!(
        grade_rule(&critical, "h1h7", Some(C::Excellent), &[]).0,
        TrainGrade::Incorrect
    );
    // A Brilliant drill wants a Brilliant move.
    let brilliant = synthetic(
        TrainKind::Miss,
        Some(TrainBucket::Brilliant),
        TrainTheme::Brilliant,
    );
    assert_eq!(
        grade_rule(&brilliant, "h1h7", Some(C::Brilliant), &[]).0,
        TrainGrade::Correct
    );
    assert_eq!(
        grade_rule(&brilliant, "h1h7", Some(C::Best), &[]).0,
        TrainGrade::NotIncorrect
    );
    // Idea drills: any decent move passes.
    let idea = synthetic(TrainKind::Endgame, None, TrainTheme::EndgameRookAndPawn);
    assert_eq!(
        grade_rule(&idea, "h1h7", Some(C::Good), &[]).0,
        TrainGrade::Correct
    );
    assert_eq!(
        grade_rule(&idea, "h1h7", Some(C::Book), &[]).0,
        TrainGrade::Correct
    );
    assert_eq!(
        grade_rule(&idea, "h1h7", Some(C::Mistake), &[]).0,
        TrainGrade::Incorrect
    );
    assert_eq!(
        grade_rule(&idea, "h1h7", Some(C::Miss), &[]).0,
        TrainGrade::Incorrect
    );
}

#[tokio::test]
async fn no_drills_for_chess960() {
    let r = review("chess960.pgn", None, ReviewConfig::default()).await;
    assert!(r.game.chess960);
    assert!(r.train_items.is_empty());
    let _ = TrainConfig::default();
}
