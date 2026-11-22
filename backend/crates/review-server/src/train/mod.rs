//! The trainer: short drills made from the player's own mistakes (`docs/trainer.md`).
//!
//! A finished review is turned into [`TrainItem`]s by [`generate`], with no engine search: every
//! drill starts one move early, at the position before the opponent's move that led to the
//! mistake (the *pitch* move), so the player sees the game go on and then has to find a better
//! move than the one they played. A drill names one idea (its [`TrainTheme`]) and is graded on
//! that idea ([`grade`]), not only on the exact move.
//!
//! The five kinds and their order follow the reference's drill maker (recovered): an opening
//! mistake, a middlegame (strategy) mistake, an endgame technique mistake, a missed chance (a
//! Miss), and a blunder; the first that applies to a move wins. The endgame kind is switched off
//! in the reference; here it is on behind `TrainConfig::endgame_maker`.

pub mod endgame;
mod generate;
pub mod grade;
pub mod text;
pub mod themes;

use review_core::{Classification, Eval, MissKind, Phase, Side};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::analysis::model::MoveRef;

pub use generate::generate;

/// Bumped whenever the rules that make drills change: a client holding drills of an older
/// version may ask for them again (`POST /api/train/generate`).
pub const GENERATOR_VERSION: u16 = 1;

/// What kind of mistake a drill comes from, in the order the makers are tried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TrainKind {
    /// A Mistake, Blunder or Miss in the opening, explained by a bad habit or a missed
    /// development idea.
    Opening,
    /// A Mistake, Blunder or Miss in the middlegame, explained by a pawn-structure or
    /// piece-placement idea.
    Strategy,
    /// In the endgame, a holdable or won position thrown away without losing material.
    Endgame,
    /// A Miss: the opponent's error was not punished.
    Miss,
    /// A Blunder from a better position: what the move allowed.
    Blunder,
}

/// How a Miss or Blunder drill wins: the group its grading looks at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TrainBucket {
    /// The best move was a sacrifice that works.
    Brilliant,
    /// A piece could be taken for free.
    FreePieces,
    /// A tactic wins material at once.
    Tactics,
    /// A tactic wins material later in the line.
    LeadsTo,
}

/// The idea a drill is about. Names are ours; [`themes::info`] has each one's wording and the
/// coach tags that produce it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TrainTheme {
    // Tactics (Miss and Blunder drills).
    Fork,
    Skewer,
    Pin,
    Discovery,
    RemovesDefender,
    TrappedPiece,
    FreePiece,
    Brilliant,
    /// A mate (not in the reference's lists: there a missed mate makes no drill).
    Mate,
    // Opening: bad habits of the played move.
    QueenOutEarly,
    SamePieceTwice,
    LosesCastling,
    PlaysF3,
    // Opening: ideas the best move had.
    Develop,
    DevelopToCenter,
    PrepareBishop,
    BishopScope,
    Fianchetto,
    PrepareCastleKingside,
    PrepareCastleQueenside,
    CastleKingside,
    CastleQueenside,
    CastleSameSide,
    CastleOppositeSide,
    PawnToCenter,
    GainTempo,
    ConnectRooks,
    // Strategy: errors of the played move.
    DoublesPawns,
    IsolatesPawn,
    BackwardPawn,
    PawnInFrontOfKing,
    PassiveSquare,
    // Strategy: ideas the best move had.
    Outpost,
    CentraliseKnight,
    RookOnSeventh,
    TakeOpenFile,
    ActiveSquare,
    StrikeAtCenter,
    CentralBreak,
    PrepareCentralPush,
    AttackPawnChain,
    MakePassedPawn,
    PushPassedPawn,
    CreateLuft,
    // Endgame families (by the material on the board).
    EndgameQueen,
    EndgameRookAndPawn,
    EndgameKingAndPawn,
    EndgameOther,
}

/// The endgame family of a position, by what is left besides kings and pawns (the reference's
/// endgame statistics buckets, recovered): no pawns → other; pawns with a queen → queen; pawns
/// with a knight or bishop → other; pawns with rooks → rook and pawn; pawns only → king and pawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum EndgameFamily {
    Queen,
    RookAndPawn,
    KingAndPawn,
    Other,
}

/// The endgame idea a technique drill is about, when one is found on the best line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum EndgameConcept {
    /// The kings face each other with one square between, the other side to move.
    Opposition,
    /// The king walks toward the centre or the pawns.
    KingActivity,
    /// A passed pawn moves on.
    PassedPawnPush,
    /// A rook behind a passed pawn, its own or the opponent's.
    RookBehindPasser,
    /// A piece stops a passed pawn by standing in front of it.
    Blockade,
    /// A trade that leaves a king-and-pawn ending.
    TradeIntoPawnEnding,
    /// The technique of a basic mate (queen or rook against a bare king).
    MatingTechnique,
    /// A pawn becomes a queen.
    Promotion,
}

/// What an endgame drill asks for when it goes on as a game against the engine (play-out mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PlayOutGoal {
    Win,
    Draw,
}

/// The pawn structure of a middlegame position (an approximation, see `endgame::position_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PositionType {
    Closed,
    SemiOpen,
    Open,
}

/// An engine line from the drill's start position, as the review searched it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TrainLine {
    /// Evaluation if the line is played (White's view).
    pub eval: Eval,
    pub depth: u32,
    /// UCI moves from the start position.
    pub pv: Vec<String>,
}

/// The game context a drill's grading needs, kept with the drill so grading does not need the
/// review on the server.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TrainContext {
    /// The rating fed to the expected-points curve in the review.
    pub rating: i32,
    /// The higher of the two players' Elo (0 when unknown).
    pub highest_elo: u32,
    /// Expected points the opponent lost with the pitch move.
    pub previous_ep_lost: f64,
    /// The pitch move was a Mistake or worse (a key moment answers it).
    pub previous_erred: bool,
}

/// Where a drill comes from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TrainSource {
    /// 1-based ply of the user's move in the game.
    pub ply: u32,
    pub move_number: u32,
    pub white: Option<String>,
    pub black: Option<String>,
    pub opponent: Option<String>,
    pub date: Option<String>,
    pub site: Option<String>,
    pub event: Option<String>,
    pub result: Option<String>,
    pub time_control: Option<String>,
}

/// One drill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TrainItem {
    /// Stable id: the same moment of a game reviewed again gives the same id (a hash of the start
    /// position without move counters, the game move and the kind).
    pub id: String,
    pub kind: TrainKind,
    pub theme: TrainTheme,
    /// Miss and Blunder drills only.
    pub bucket: Option<TrainBucket>,
    /// The side the player plays in the drill.
    pub user_side: Side,
    /// The drill starts here, then the opponent's `pitch_move` is played.
    pub pre_pitch_fen: String,
    pub pitch_move: MoveRef,
    /// The position after the pitch move, the player to move.
    pub start_fen: String,
    /// The engine's best move.
    pub correct_move: MoveRef,
    /// The move played in the game.
    pub played_move: MoveRef,
    /// The best line (UCI, the best move first), for "show me".
    pub best_line: Vec<String>,
    /// The game move and the opponent's best answer to it (UCI, the game move first).
    pub played_line: Vec<String>,
    /// The review's engine lines from `start_fen`, best first: grading reuses them.
    pub lines: Vec<TrainLine>,
    /// The game move's classification.
    pub classification: Classification,
    pub miss_kind: Option<MissKind>,
    /// The phase of the game move (the reference's phase rule, whatever the review's own rule).
    pub phase: Phase,
    /// Only one move works: any other move is graded incorrect (Miss drills).
    pub is_critical: bool,
    /// Strategy drills: the pawn structure.
    pub position_type: Option<PositionType>,
    /// Opening drills: the game's opening.
    pub eco: Option<String>,
    pub opening_name: Option<String>,
    /// Endgame drills: the family, the idea found on the best line, the material signature of a
    /// known endgame ("KRPvKR"), and what a play-out asks for.
    pub endgame_family: Option<EndgameFamily>,
    pub endgame_concept: Option<EndgameConcept>,
    pub endgame_type: Option<String>,
    pub play_out: Option<PlayOutGoal>,
    /// Expected points the game move lost.
    pub ep_lost: f64,
    pub context: TrainContext,
    /// What the coach says before the move: the task, never the answer.
    pub goal: String,
    /// The idea in one sentence (the second hint).
    pub hint: String,
    pub source: TrainSource,
    pub generator_version: u16,
}
