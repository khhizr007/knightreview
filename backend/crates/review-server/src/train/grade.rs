//! Grading the player's move in a drill (`POST /api/train/grade`).
//!
//! The move is classified with the review's own rules (`analysis::facts::judge_move`) on engine
//! lines the browser sends, or the review's lines kept in the drill, so grading needs nothing
//! stored on the server. The grade rule is the reference's (recovered):
//!
//! 1. the move played in the game: *same as game*;
//! 2. the best move: *correct*;
//! 3. Miss and Blunder drills: any move in a critical position, and any move graded Inaccuracy
//!    or worse, is *incorrect*; otherwise the move must carry the drill's idea to be *correct*
//!    (take a free piece, be a Brilliant sacrifice, win material with the same tactic now or later
//!    in its line), else it is *not incorrect*;
//! 4. Opening, Strategy and Endgame drills: a move graded Inaccuracy or worse is *incorrect*,
//!    anything else is *correct*;
//! 5. with no engine line for the move, only the best move is correct.

use review_core::{Classification, Eval, MissKind, ReviewConfig};
use serde::{Deserialize, Serialize};
use shakmaty::fen::Fen;
use shakmaty::{CastlingMode, Chess, Move, Position};
use thiserror::Error;
use ts_rs::TS;

use super::text;
use super::themes;
use super::{TrainBucket, TrainItem, TrainKind, TrainLine, TrainTheme};
use crate::analysis::facts::{CoachContext, MoveInput, PreviousMoves, coach_note, judge_move};
use crate::analysis::model::MoveRef;
use crate::analysis::to_san_line;
use crate::board::{self, side};
use crate::coach::circ::Circ;
use crate::coach::{CoachEvent, CoachNote};
use crate::engine::EngineLine;
use crate::openings::OpeningBook;

/// How a drill move is graded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TrainGrade {
    Correct,
    /// A fine move, but not the drill's idea.
    NotIncorrect,
    Incorrect,
    /// The move played in the game.
    SameAsGame,
}

/// A drill move to grade.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct TrainGradeRequest {
    /// The drill, as `train_items` gave it.
    pub item: TrainItem,
    /// The player's move (UCI).
    pub user_move: String,
    /// Engine lines of the start position, best first; the drill's own lines when left out.
    #[ts(optional)]
    pub before: Option<Vec<TrainLine>>,
    /// The best line from the position after the player's move (needed unless the move is the
    /// first move of one of the lines before, or ends the game).
    #[ts(optional)]
    pub after: Option<TrainLine>,
}

/// A graded drill move.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TrainGradeResponse {
    pub grade: TrainGrade,
    pub user_move: MoveRef,
    /// The move's classification by the review's rules (absent without engine lines).
    pub classification: Option<Classification>,
    pub miss_kind: Option<MissKind>,
    /// Expected points the move lost against the best move.
    pub ep_lost: Option<f64>,
    /// The move carries the drill's idea.
    pub on_theme: bool,
    /// The grade was decided without an engine line for the move (only the best move counts).
    pub fallback: bool,
    /// What the coach says, one sentence per entry.
    pub reply: Vec<String>,
    /// The coach's note on the move, as for a game move.
    pub coach: CoachNote,
    /// The line to replay from the start position (UCI): the best line for a correct move, the
    /// move and the reply that shows what is wrong with it otherwise.
    pub show_line: Vec<String>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum GradeError {
    #[error("the drill's positions could not be read")]
    BadItem,
    #[error("{0} is not a legal move in the drill's position")]
    IllegalMove(String),
    #[error("the engine lines do not start with a legal move")]
    BadLines,
}

fn position(fen: &str) -> Option<Chess> {
    fen.parse::<Fen>()
        .ok()?
        .into_position(CastlingMode::Standard)
        .ok()
}

fn engine_lines(lines: &[TrainLine]) -> Vec<EngineLine> {
    lines
        .iter()
        .enumerate()
        .map(|(i, l)| EngineLine {
            rank: u8::try_from(i + 1).unwrap_or(u8::MAX),
            eval: l.eval,
            depth: l.depth,
            pv: l.pv.clone(),
        })
        .collect()
}

/// The events of the player's move that carry the drill's idea.
fn on_theme(item: &TrainItem, class: Option<Classification>, events: &[CoachEvent]) -> bool {
    let base_with = |then: Option<Circ>, wanted: &dyn Fn(&CoachEvent) -> bool| {
        events.iter().any(|e| {
            e.chain.first() == Some(&Circ::Base) && e.chain.get(1).copied() == then && wanted(e)
        })
    };
    let theme = item.theme;
    match item.bucket {
        Some(TrainBucket::Brilliant) => class == Some(Classification::Brilliant),
        Some(TrainBucket::FreePieces) => {
            base_with(Some(Circ::WinsMaterial), &|e| themes::is_free_piece(e.tag))
        }
        Some(TrainBucket::Tactics) | Some(TrainBucket::LeadsTo) if theme == TrainTheme::Mate => {
            base_with(None, &|e| e.tag.is_mate())
        }
        Some(TrainBucket::Tactics) => base_with(Some(Circ::WinsMaterial), &|e| {
            themes::tactic_theme(e.tag) == Some(theme)
        }),
        Some(TrainBucket::LeadsTo) => base_with(Some(Circ::LeadsTo), &|e| {
            themes::tactic_theme(e.tag) == Some(theme)
        }),
        None => false,
    }
}

fn bad(class: Option<Classification>) -> bool {
    class.is_some_and(Classification::is_bad)
}

/// The grade rule (see the module docs), given the move's classification and coach events.
pub fn grade_rule(
    item: &TrainItem,
    user_move: &str,
    class: Option<Classification>,
    events: &[CoachEvent],
) -> (TrainGrade, bool) {
    let themed = on_theme(item, class, events);
    if user_move == item.played_move.uci {
        return (TrainGrade::SameAsGame, false);
    }
    if user_move == item.correct_move.uci {
        return (TrainGrade::Correct, themed);
    }
    let grade = match item.kind {
        TrainKind::Miss | TrainKind::Blunder => {
            if item.is_critical || bad(class) {
                TrainGrade::Incorrect
            } else if themed {
                TrainGrade::Correct
            } else {
                TrainGrade::NotIncorrect
            }
        }
        TrainKind::Opening | TrainKind::Strategy | TrainKind::Endgame => {
            if bad(class) {
                TrainGrade::Incorrect
            } else {
                TrainGrade::Correct
            }
        }
    };
    (grade, themed)
}

/// Grades the player's move in a drill.
pub fn grade_move(
    req: &TrainGradeRequest,
    openings: &OpeningBook,
    cfg: &ReviewConfig,
) -> Result<TrainGradeResponse, GradeError> {
    let item = &req.item;
    let start = position(&item.start_fen).ok_or(GradeError::BadItem)?;
    let user = board::parse_uci(&start, &req.user_move)
        .ok_or_else(|| GradeError::IllegalMove(req.user_move.clone()))?;
    // The move in the backend's UCI (castling as the king's move).
    let uci = board::uci(user, CastlingMode::Standard);
    let mut after = start.clone();
    after.play_unchecked(user);
    let user_ref = to_san_line(&start, std::slice::from_ref(&uci), 1)
        .into_iter()
        .next()
        .ok_or(GradeError::BadItem)?;
    let previous: Option<Move> =
        position(&item.pre_pitch_fen).and_then(|p| board::parse_uci(&p, &item.pitch_move.uci));

    if uci == item.played_move.uci {
        return Ok(TrainGradeResponse {
            grade: TrainGrade::SameAsGame,
            user_move: user_ref,
            classification: Some(item.classification),
            miss_kind: item.miss_kind,
            ep_lost: Some(item.ep_lost),
            on_theme: false,
            fallback: false,
            reply: text::reply(item, TrainGrade::SameAsGame, false, true),
            coach: CoachNote::default(),
            show_line: item.played_line.clone(),
        });
    }

    let lines = engine_lines(req.before.as_deref().unwrap_or(&item.lines));
    if lines.is_empty()
        || lines.iter().any(|l| {
            l.pv.first()
                .is_none_or(|m| board::parse_uci(&start, m).is_none())
        })
    {
        return Err(GradeError::BadLines);
    }
    let in_lines = lines.iter().any(|l| l.pv.first() == Some(&uci));
    let game_over = after.legal_moves().is_empty();
    let after_line = req.after.as_ref().map(|l| EngineLine {
        rank: 1,
        eval: l.eval,
        depth: l.depth,
        pv: l.pv.clone(),
    });
    let eval_after = if game_over {
        if after.is_checkmate() {
            Some(Eval::Mate {
                winner: side(after.turn().other()),
                moves: 0,
            })
        } else {
            Some(Eval::DRAW)
        }
    } else if let Some(l) = &after_line {
        Some(l.eval)
    } else if in_lines {
        Some(lines[0].eval)
    } else {
        None
    };

    // No engine line for the move: only the best move is correct.
    let Some(eval_after) = eval_after else {
        let grade = if uci == item.correct_move.uci {
            TrainGrade::Correct
        } else {
            TrainGrade::Incorrect
        };
        return Ok(TrainGradeResponse {
            grade,
            user_move: user_ref,
            classification: None,
            miss_kind: None,
            ep_lost: None,
            on_theme: false,
            fallback: true,
            reply: text::reply(item, grade, false, false),
            coach: CoachNote::default(),
            show_line: if grade == TrainGrade::Correct {
                item.best_line.clone()
            } else {
                vec![uci.clone()]
            },
        });
    };

    let input = MoveInput {
        before: &start,
        after: &after,
        mv: user,
        uci: &uci,
        previous,
        lines: &lines,
        eval_before: lines[0].eval,
        before_is_draw: board::is_board_draw(&start),
        eval_after,
        after_line: after_line.as_ref(),
        after_is_draw: board::is_board_draw(&after),
        is_book: openings.is_book_move(&start, &after),
        phase_after: item.phase,
        has_previous_move: true,
        shallow_best: None,
        opponent_previous_ep_lost: Some(item.context.previous_ep_lost),
        previous_moves: PreviousMoves {
            opponent_erred: item.context.previous_erred,
            ..PreviousMoves::default()
        },
        rating: item.context.rating,
        highest_elo: item.context.highest_elo,
        own_history: &[],
    };
    let judged = judge_move(&input, cfg);
    let coach = coach_note(
        &input,
        &judged,
        &CoachContext {
            threat: None,
            prev_threat: None,
            best_threat: None,
            // Every idea counts for grading, whatever the player's rating.
            rating: None,
            difficulty: None,
            player_side: Some(item.user_side),
            seed: text::seed(&item.id) as u32,
        },
        cfg,
    );
    let class = judged.outcome.classification;
    let (grade, themed) = grade_rule(item, &uci, class, &coach.events);
    let own_line: Vec<String> = std::iter::once(uci.clone())
        .chain(
            lines
                .iter()
                .find(|l| l.pv.first() == Some(&uci))
                .map(|l| l.pv[1..].to_vec())
                .or_else(|| after_line.as_ref().map(|l| l.pv.clone()))
                .unwrap_or_default(),
        )
        .collect();
    let show_line = match grade {
        TrainGrade::Correct if uci == item.correct_move.uci => item.best_line.clone(),
        _ => own_line,
    };
    Ok(TrainGradeResponse {
        grade,
        user_move: user_ref,
        classification: class,
        miss_kind: judged.outcome.miss_kind,
        ep_lost: Some(judged.outcome.ep_lost),
        on_theme: themed,
        fallback: false,
        reply: text::reply(item, grade, themed, true),
        coach,
        show_line,
    })
}
