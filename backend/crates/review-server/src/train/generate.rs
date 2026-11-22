//! Drills from a finished review (`train::generate`).

use review_core::expected_points::node_expected_points;
use review_core::{
    Classification, Eval, MissKind, Phase, PhaseRule, Precision, Side, TrainConfig, Value,
};
use shakmaty::fen::Fen;
use shakmaty::{CastlingMode, Chess, Move, Position, Role};

use super::endgame;
use super::text;
use super::themes::{self, TACTIC_ORDER};
use super::{
    EndgameFamily, GENERATOR_VERSION, PlayOutGoal, TrainBucket, TrainContext, TrainItem, TrainKind,
    TrainLine, TrainSource, TrainTheme,
};
use crate::analysis::detectors::{lands_safely, walk_line};
use crate::analysis::model::{MoveRef, PlyReview, ReviewResult};
use crate::board;
use crate::coach::CoachEvent;
use crate::coach::circ::Circ;
use crate::coach::detect::Tag;

/// Plies of a line inspected for material (the review's default horizon).
const HORIZON: u32 = 6;
/// Endgame technique drills: the best move keeps at least this (mover's view, cp) ...
const ENDGAME_BEST_MIN_CP: i32 = 0;
/// ... and the game move leaves at most this.
const ENDGAME_PLAYED_MAX_CP: i32 = -100;
/// Material (cp) the played or best line may win or lose for a drill to be about technique.
const ENDGAME_MATERIAL_MAX_CP: i32 = 100;
/// Strategy drills: the best line is not already lost (mover's view, cp).
const STRATEGY_BEST_MIN_CP: i32 = -100;
/// Play-out: a best line at least this good is a win to convert; one below `PLAY_OUT_DRAW_CP` a
/// draw to hold. Lines shorter than `PLAY_OUT_MIN_PLIES` are not played out.
const PLAY_OUT_WIN_CP: i32 = 200;
const PLAY_OUT_DRAW_CP: i32 = 50;
const PLAY_OUT_MIN_PLIES: usize = 6;
/// The critical-position test (recovered): the best move keeps at least `CRITICAL_BEST_EP`, the
/// second line is below `CRITICAL_ALT_EP` and loses at least `CRITICAL_ALT_LOSS`.
const CRITICAL_BEST_EP: f64 = 0.35;
const CRITICAL_ALT_EP: f64 = 0.80;
const CRITICAL_ALT_LOSS: f64 = 0.10;
/// A recapture is forced when the second line loses at least this much.
const FORCED_RECAPTURE_LOSS: f64 = 0.10;

/// The drills of a finished review: at most one per move of the player (both sides when the
/// player is not known), at most `max_per_game` per side, each from a move whose parent is not
/// the start of the game (a drill starts with the opponent's move before it).
pub fn generate(review: &ReviewResult, cfg: &TrainConfig) -> Vec<TrainItem> {
    if !cfg.enabled || review.game.chess960 || review.plies.len() < 2 {
        return Vec::new();
    }
    let Some(game) = Game::new(review) else {
        return Vec::new();
    };
    let user = review.settings.player_side;
    let mut items: Vec<TrainItem> = (1..review.plies.len())
        .filter(|&i| user.is_none_or(|s| review.plies[i].mover == s))
        .filter_map(|i| make(&game, i, cfg))
        .collect();

    // At most `max_per_game` per side: the largest losses, then the earlier kind.
    let mut kept = Vec::new();
    for side in [Side::White, Side::Black] {
        let mut mine: Vec<TrainItem> = items.extract_if(.., |it| it.user_side == side).collect();
        mine.sort_by(|a, b| b.ep_lost.total_cmp(&a.ep_lost).then(a.kind.cmp(&b.kind)));
        mine.truncate(cfg.max_per_game as usize);
        kept.extend(mine);
    }
    kept.sort_by_key(|it| it.source.ply);
    kept
}

/// The review with its positions replayed.
struct Game<'a> {
    review: &'a ReviewResult,
    /// `before[i]` is the position before ply `i` (0-based).
    before: Vec<Chess>,
    /// The phase of the position after ply `i` (the reference's rule).
    phase: Vec<Phase>,
    /// The starting time of the time control (ms), when the PGN gives one.
    base_ms: Option<u64>,
}

impl<'a> Game<'a> {
    fn new(review: &'a ReviewResult) -> Option<Game<'a>> {
        let parse = |fen: &str| -> Option<Chess> {
            fen.parse::<Fen>()
                .ok()?
                .into_position(CastlingMode::Standard)
                .ok()
        };
        let before: Vec<Chess> = review
            .plies
            .iter()
            .map(|p| parse(&p.fen_before))
            .collect::<Option<_>>()?;
        let last = parse(&review.plies.last()?.fen_after)?;
        let moves: Vec<Move> = review
            .plies
            .iter()
            .zip(&before)
            .map(|(p, pos)| board::parse_uci(pos, &p.uci))
            .collect::<Option<_>>()?;
        let mut positions = before.clone();
        positions.push(last);
        let phase = board::game_phases(&positions, &moves, PhaseRule::Reference)
            .into_iter()
            .skip(1)
            .collect();
        Some(Game {
            review,
            before,
            phase,
            base_ms: base_time_ms(review),
        })
    }

    fn header(&self, name: &str) -> Option<String> {
        self.review
            .game
            .headers
            .iter()
            .find(|h| h.name == name)
            .map(|h| h.value.clone())
    }
}

/// The starting time of a `TimeControl` header ("600+5" → 600 s), in ms.
fn base_time_ms(review: &ReviewResult) -> Option<u64> {
    let tc = review
        .game
        .headers
        .iter()
        .find(|h| h.name == "TimeControl")?;
    let base = tc.value.split(['+', '/']).next()?.trim();
    base.parse::<u64>()
        .ok()
        .filter(|s| *s > 0)
        .map(|s| s * 1000)
}

fn eval_after_first_move(eval: Eval, mover: Side) -> Eval {
    match eval {
        Eval::Mate { winner, moves } if winner == mover => Eval::Mate {
            winner,
            moves: moves.saturating_sub(1),
        },
        other => other,
    }
}

fn line_ucis(p: &PlyReview, k: usize) -> Vec<String> {
    p.lines
        .get(k)
        .map(|l| l.moves.iter().map(|m| m.uci.clone()).collect())
        .unwrap_or_default()
}

/// The move was graded an error a key moment answers: Mistake or Blunder, a missed mate, or not
/// graded.
fn erred(p: &PlyReview) -> bool {
    matches!(
        p.precision,
        None | Some(Precision::Mistake | Precision::Blunder)
    ) || matches!(p.miss_kind, Some(MissKind::Mate | MissKind::FasterMate))
}

fn is_bad(c: Classification) -> bool {
    matches!(
        c,
        Classification::Mistake | Classification::Blunder | Classification::Miss
    )
}

/// The mover was short of time before the move: their clock after their own previous move.
fn in_time_trouble(game: &Game<'_>, i: usize, cfg: &TrainConfig) -> bool {
    let Some(clock) = i.checked_sub(2).and_then(|k| game.review.plies[k].clock_ms) else {
        return false;
    };
    let clock = u64::from(clock);
    clock < u64::from(cfg.skip_time_trouble_ms)
        || game
            .base_ms
            .is_some_and(|base| (clock as f64) < base as f64 * cfg.skip_time_trouble_fraction)
}

/// The position is critical: only the best move keeps the game (the reference's
/// `is_position_critical`, recovered).
fn is_critical(before: &Chess, p: &PlyReview, rating: i32) -> bool {
    let (Some(best), Some(alt)) = (p.lines.first(), p.lines.get(1)) else {
        return false;
    };
    let ep = |eval: Eval| {
        node_expected_points(
            Value::after_move(eval_after_first_move(eval, p.mover), p.mover),
            false,
            rating,
        )
    };
    let unsafe_block = before.is_check()
        && alt
            .moves
            .first()
            .and_then(|m| board::parse_uci(before, &m.uci))
            .is_some_and(|a| a.role() != Role::King && !a.is_capture() && !lands_safely(before, a));
    let (best_ep, alt_ep) = (ep(best.eval), ep(alt.eval));
    !unsafe_block
        && best_ep >= CRITICAL_BEST_EP
        && alt_ep < CRITICAL_ALT_EP
        && best_ep - alt_ep >= CRITICAL_ALT_LOSS
}

/// The best move just takes back on the square the pitch move captured on, and nothing else
/// holds: no idea to practise.
fn forced_recapture(before: &Chess, pitch: &PlyReview, p: &PlyReview, rating: i32) -> bool {
    let Some(best) = p.best_move.as_ref() else {
        return false;
    };
    let takes_back = pitch.flags.capture
        && board::parse_uci(before, &best.uci)
            .is_some_and(|m| m.is_capture() && m.to().to_string() == pitch.to);
    if !takes_back {
        return false;
    }
    let ep = |eval: Eval| {
        node_expected_points(
            Value::after_move(eval_after_first_move(eval, p.mover), p.mover),
            false,
            rating,
        )
    };
    match (p.lines.first(), p.lines.get(1)) {
        (Some(b), Some(a)) => ep(b.eval) - ep(a.eval) >= FORCED_RECAPTURE_LOSS,
        _ => true,
    }
}

/// The events of a move whose chain starts with one of `first` and continues with `then`
/// (`None`: the chain has no second element).
fn chain_is(e: &CoachEvent, first: &[Circ], then: Option<Circ>) -> bool {
    e.chain.first().is_some_and(|c| first.contains(c)) && e.chain.get(1).copied() == then
}

fn is_mate_tag(tag: Tag) -> bool {
    tag.is_mate()
}

/// The bucket and theme of a Miss (on MISSES events) or Blunder (on PERMITS events), in the
/// reference's order: a free piece, then the tactical themes winning material at once, then the
/// same themes winning it later. A mate (ours) comes last.
fn tactic_bucket(events: &[CoachEvent], circ: &[Circ]) -> Option<(TrainBucket, TrainTheme)> {
    let free = events.iter().any(|e| {
        (chain_is(e, circ, Some(Circ::WinsMaterial)) && themes::is_free_piece(e.tag))
            || (circ.contains(&Circ::MissesSame) && chain_is(e, &[Circ::MissesSame], None))
    });
    if free {
        return Some((TrainBucket::FreePieces, TrainTheme::FreePiece));
    }
    for (then, bucket) in [
        (Circ::WinsMaterial, TrainBucket::Tactics),
        (Circ::LeadsTo, TrainBucket::LeadsTo),
    ] {
        for theme in TACTIC_ORDER {
            if events.iter().any(|e| {
                chain_is(e, circ, Some(then)) && themes::tactic_theme(e.tag) == Some(theme)
            }) {
                return Some((bucket, theme));
            }
        }
    }
    events
        .iter()
        .any(|e| chain_is(e, circ, None) && is_mate_tag(e.tag))
        .then_some((TrainBucket::Tactics, TrainTheme::Mate))
}

/// What a maker found: the kind and theme, and the extra facts of its kind.
struct Found {
    kind: TrainKind,
    theme: TrainTheme,
    bucket: Option<TrainBucket>,
    is_critical: bool,
    endgame: Option<EndgameFound>,
}

struct EndgameFound {
    family: EndgameFamily,
    concept: Option<super::EndgameConcept>,
    known: Option<String>,
    play_out: Option<PlayOutGoal>,
}

fn found(kind: TrainKind, theme: TrainTheme) -> Found {
    Found {
        kind,
        theme,
        bucket: None,
        is_critical: false,
        endgame: None,
    }
}

fn opening(p: &PlyReview, phase: Phase, class: Classification) -> Option<Found> {
    if phase != Phase::Opening || !is_bad(class) {
        return None;
    }
    let events = &p.coach.events;
    events
        .iter()
        .filter(|e| chain_is(e, &[Circ::Base], None))
        .find_map(|e| themes::opening_error(e.tag))
        .or_else(|| {
            events
                .iter()
                .filter(|e| chain_is(e, &[Circ::Misses], None))
                .find_map(|e| themes::opening_idea(e.tag))
        })
        .map(|theme| found(TrainKind::Opening, theme))
}

fn strategy(
    p: &PlyReview,
    phase: Phase,
    class: Classification,
    best: Value,
    played: Value,
) -> Option<Found> {
    if phase != Phase::Middlegame
        || !is_bad(class)
        || best.0 < STRATEGY_BEST_MIN_CP
        || played.mate_moves().is_some_and(|n| n < 0)
    {
        return None;
    }
    let events = &p.coach.events;
    events
        .iter()
        .filter(|e| chain_is(e, &[Circ::Base], None))
        .find_map(|e| themes::strategy_error(e.tag))
        .or_else(|| {
            events
                .iter()
                .filter(|e| chain_is(e, &[Circ::Misses], None))
                .find_map(|e| themes::strategy_idea(e.tag))
        })
        .map(|theme| found(TrainKind::Strategy, theme))
}

fn endgame_technique(
    before: &Chess,
    p: &PlyReview,
    next: Option<&PlyReview>,
    phase: Phase,
    best: Value,
    played: Value,
) -> Option<Found> {
    if phase != Phase::Endgame
        || best.0 < ENDGAME_BEST_MIN_CP
        || played.0 > ENDGAME_PLAYED_MAX_CP
        || best.is_mate()
        || played.is_mate()
    {
        return None;
    }
    let mover = before.turn();
    let mut played_line = vec![p.uci.clone()];
    played_line.extend(next.map(|n| line_ucis(n, 0)).unwrap_or_default());
    let played_walk = walk_line(before, mover, &played_line, HORIZON);
    let best_line = line_ucis(p, 0);
    let best_walk = walk_line(before, mover, &best_line, HORIZON);
    if played_walk.net() > ENDGAME_MATERIAL_MAX_CP
        || p.material_lost_cp.unwrap_or(0) > ENDGAME_MATERIAL_MAX_CP
        || best_walk.net() > ENDGAME_MATERIAL_MAX_CP
    {
        return None;
    }
    let best_move = board::parse_uci(before, best_line.first()?)?;
    let family = endgame::family(before.board());
    let long_enough = p
        .lines
        .first()
        .is_some_and(|l| l.moves.len() >= PLAY_OUT_MIN_PLIES);
    let play_out = if !long_enough {
        None
    } else if best.0 >= PLAY_OUT_WIN_CP {
        Some(PlayOutGoal::Win)
    } else if best.0 < PLAY_OUT_DRAW_CP {
        Some(PlayOutGoal::Draw)
    } else {
        None
    };
    Some(Found {
        endgame: Some(EndgameFound {
            family,
            concept: endgame::concept(before, best_move, &best_line),
            known: endgame::known_ending(before.board()),
            play_out,
        }),
        ..found(TrainKind::Endgame, family_theme(family))
    })
}

fn family_theme(family: EndgameFamily) -> TrainTheme {
    match family {
        EndgameFamily::Queen => TrainTheme::EndgameQueen,
        EndgameFamily::RookAndPawn => TrainTheme::EndgameRookAndPawn,
        EndgameFamily::KingAndPawn => TrainTheme::EndgameKingAndPawn,
        EndgameFamily::Other => TrainTheme::EndgameOther,
    }
}

fn miss(before: &Chess, p: &PlyReview, class: Classification, rating: i32) -> Option<Found> {
    if class != Classification::Miss {
        return None;
    }
    let (bucket, theme) = if p.miss_kind == Some(MissKind::Brilliant) {
        (TrainBucket::Brilliant, TrainTheme::Brilliant)
    } else {
        tactic_bucket(&p.coach.events, &[Circ::Misses, Circ::MissesSame])?
    };
    Some(Found {
        bucket: Some(bucket),
        is_critical: is_critical(before, p, rating),
        ..found(TrainKind::Miss, theme)
    })
}

fn blunder(p: &PlyReview, class: Classification, best: Value) -> Option<Found> {
    if class != Classification::Blunder || best.0 <= 0 {
        return None;
    }
    let (bucket, theme) = tactic_bucket(&p.coach.events, &[Circ::Permits])?;
    Some(Found {
        bucket: Some(bucket),
        ..found(TrainKind::Blunder, theme)
    })
}

/// A stable 128-bit hex id (two FNV-1a hashes with different seeds).
fn stable_id(text: &str) -> String {
    let fnv = |seed: u64| {
        text.bytes().fold(seed, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
        })
    };
    format!(
        "{:016x}{:016x}",
        fnv(0xcbf2_9ce4_8422_2325),
        fnv(0x8422_2325_cbf2_9ce4)
    )
}

/// The FEN without its move counters.
fn position_key(fen: &str) -> String {
    fen.split_whitespace().take(4).collect::<Vec<_>>().join(" ")
}

fn make(game: &Game<'_>, i: usize, cfg: &TrainConfig) -> Option<TrainItem> {
    let review = game.review;
    let p = &review.plies[i];
    let pitch = &review.plies[i - 1];
    let before = &game.before[i];
    let class = p.classification?;
    let best_move = p.best_move.clone()?;
    if p.played_is_best || before.legal_moves().len() <= 1 || in_time_trouble(game, i, cfg) {
        return None;
    }
    let rating = review.rating_used;
    if forced_recapture(before, pitch, p, rating) {
        return None;
    }
    let mover = p.mover;
    let best = Value::after_move(p.eval_best.unwrap_or(p.eval_before), mover);
    let played = Value::after_move(p.eval_after, mover);
    let phase = game.phase[i];
    let next = review.plies.get(i + 1);

    let found = opening(p, phase, class)
        .or_else(|| strategy(p, phase, class, best, played))
        .or_else(|| {
            cfg.endgame_maker
                .then(|| endgame_technique(before, p, next, phase, best, played))
                .flatten()
        })
        .or_else(|| miss(before, p, class, rating))
        .or_else(|| blunder(p, class, best))?;

    let mut played_line = vec![p.uci.clone()];
    played_line.extend(next.map(|n| line_ucis(n, 0)).unwrap_or_default());
    let id = stable_id(&format!(
        "{}|{}|{}",
        position_key(&p.fen_before),
        p.uci,
        text::key(found.kind)
    ));
    let seed = text::seed(&id);
    let family = found.endgame.as_ref().map(|e| e.family);
    let concept = found.endgame.as_ref().and_then(|e| e.concept);
    let name = |side: Side| match side {
        Side::White => review.game.white.clone(),
        Side::Black => review.game.black.clone(),
    };
    let highest_elo = review
        .game
        .white_elo
        .unwrap_or(0)
        .max(review.game.black_elo.unwrap_or(0));
    Some(TrainItem {
        id,
        kind: found.kind,
        theme: found.theme,
        bucket: found.bucket,
        user_side: mover,
        pre_pitch_fen: pitch.fen_before.clone(),
        pitch_move: MoveRef {
            san: pitch.san.clone(),
            uci: pitch.uci.clone(),
            fen: pitch.fen_after.clone(),
        },
        start_fen: p.fen_before.clone(),
        correct_move: best_move,
        played_move: MoveRef {
            san: p.san.clone(),
            uci: p.uci.clone(),
            fen: p.fen_after.clone(),
        },
        best_line: line_ucis(p, 0),
        played_line,
        lines: p
            .lines
            .iter()
            .map(|l| TrainLine {
                eval: l.eval,
                depth: l.depth,
                pv: l.moves.iter().map(|m| m.uci.clone()).collect(),
            })
            .collect(),
        classification: class,
        miss_kind: p.miss_kind,
        phase,
        is_critical: found.is_critical,
        position_type: (found.kind == TrainKind::Strategy)
            .then(|| endgame::position_type(before))
            .flatten(),
        eco: (found.kind == TrainKind::Opening)
            .then(|| review.game.opening.as_ref().map(|o| o.eco.clone()))
            .flatten(),
        opening_name: (found.kind == TrainKind::Opening)
            .then(|| review.game.opening.as_ref().map(|o| o.name.clone()))
            .flatten(),
        endgame_family: family,
        endgame_concept: concept,
        endgame_type: found.endgame.as_ref().and_then(|e| e.known.clone()),
        play_out: found.endgame.as_ref().and_then(|e| e.play_out),
        ep_lost: p.ep_lost,
        context: TrainContext {
            rating,
            highest_elo,
            previous_ep_lost: pitch.ep_lost,
            previous_erred: erred(pitch),
        },
        goal: text::goal(found.kind, found.bucket, found.theme, family, seed),
        hint: text::hint(found.theme, concept),
        source: TrainSource {
            ply: p.ply,
            move_number: p.move_number,
            white: review.game.white.clone(),
            black: review.game.black.clone(),
            opponent: name(mover.opponent()),
            date: review.game.date.clone(),
            site: review.game.site.clone(),
            event: review.game.event.clone(),
            result: review.game.result.clone(),
            time_control: game.header("TimeControl"),
        },
        generator_version: GENERATOR_VERSION,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_stable_and_distinct() {
        let a = stable_id("x|e2e4|miss");
        assert_eq!(a, stable_id("x|e2e4|miss"));
        assert_ne!(a, stable_id("x|e2e4|blunder"));
        assert_eq!(a.len(), 32);
        assert_eq!(
            position_key("8/8/8/8/8/8/8/K6k w - - 12 40"),
            "8/8/8/8/8/8/8/K6k w - -"
        );
    }
}
