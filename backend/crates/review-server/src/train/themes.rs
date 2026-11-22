//! The drill themes: their wording, their family, and the coach tags behind each one.

use serde::Serialize;
use ts_rs::TS;

use super::{TrainKind, TrainTheme};
use crate::coach::detect::Tag;

/// The family a theme is shown under on the weakness map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ThemeFamily {
    Tactics,
    Opening,
    Strategy,
    Endgame,
}

impl ThemeFamily {
    /// The family of a drill kind.
    pub fn of_kind(kind: TrainKind) -> ThemeFamily {
        match kind {
            TrainKind::Opening => ThemeFamily::Opening,
            TrainKind::Strategy => ThemeFamily::Strategy,
            TrainKind::Endgame => ThemeFamily::Endgame,
            TrainKind::Miss | TrainKind::Blunder => ThemeFamily::Tactics,
        }
    }
}

/// A theme as the trainer shows it.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ThemeInfo {
    pub theme: TrainTheme,
    /// The family it is listed under (a theme shared by the opening and the middlegame is listed
    /// under the opening).
    pub family: ThemeFamily,
    /// Short name ("Fork").
    pub name: String,
    /// What it means, in one line.
    pub about: String,
}

const fn row(
    theme: TrainTheme,
    family: ThemeFamily,
    name: &'static str,
    about: &'static str,
) -> (TrainTheme, ThemeFamily, &'static str, &'static str) {
    (theme, family, name, about)
}

use ThemeFamily::{Endgame as E, Opening as O, Strategy as S, Tactics as T};
use TrainTheme as Th;

/// Every theme with its family, name and one-line meaning, in display order.
const TABLE: &[(TrainTheme, ThemeFamily, &str, &str)] = &[
    row(Th::Fork, T, "Fork", "One piece attacks two at once."),
    row(
        Th::Skewer,
        T,
        "Skewer",
        "A valuable piece is attacked and must step aside, leaving the one behind it.",
    ),
    row(
        Th::Pin,
        T,
        "Pin",
        "A piece cannot move without exposing a more valuable one behind it.",
    ),
    row(
        Th::Discovery,
        T,
        "Discovered attack",
        "Moving one piece uncovers an attack by another.",
    ),
    row(
        Th::RemovesDefender,
        T,
        "Remove the defender",
        "Take or chase away the piece that guards another.",
    ),
    row(
        Th::TrappedPiece,
        T,
        "Trapped piece",
        "A piece with no safe square can be won.",
    ),
    row(
        Th::FreePiece,
        T,
        "Free piece",
        "A piece left unguarded can simply be taken.",
    ),
    row(
        Th::Brilliant,
        T,
        "Sacrifice",
        "Giving up material for something bigger.",
    ),
    row(Th::Mate, T, "Checkmate", "A forced mate was on the board."),
    row(
        Th::QueenOutEarly,
        O,
        "Queen out early",
        "The queen came out before the knights and bishops.",
    ),
    row(
        Th::SamePieceTwice,
        O,
        "Same piece twice",
        "One piece moved again while others still wait at home.",
    ),
    row(
        Th::LosesCastling,
        O,
        "Losing castling",
        "A king or rook move gave up the right to castle.",
    ),
    row(
        Th::PlaysF3,
        O,
        "Weakening f-pawn",
        "An early f-pawn move opens the king's diagonal.",
    ),
    row(Th::Develop, O, "Develop", "Bring a new piece into play."),
    row(
        Th::DevelopToCenter,
        O,
        "Develop to the centre",
        "Bring a knight or bishop to a central square.",
    ),
    row(
        Th::PrepareBishop,
        O,
        "Free the bishop",
        "A pawn move opens the way for a bishop at home.",
    ),
    row(
        Th::BishopScope,
        O,
        "Bishop's diagonal",
        "Put the bishop on a longer diagonal.",
    ),
    row(
        Th::Fianchetto,
        O,
        "Fianchetto",
        "The bishop develops to the long diagonal.",
    ),
    row(
        Th::PrepareCastleKingside,
        O,
        "Get ready to castle",
        "Clear the way between the king and the h-rook.",
    ),
    row(
        Th::PrepareCastleQueenside,
        O,
        "Get ready to castle long",
        "Clear the way between the king and the a-rook.",
    ),
    row(
        Th::CastleKingside,
        O,
        "Castle",
        "Tuck the king away and bring the rook in.",
    ),
    row(
        Th::CastleQueenside,
        O,
        "Castle long",
        "Castle on the queenside.",
    ),
    row(
        Th::CastleSameSide,
        O,
        "Castle",
        "Castle on the same side as the opponent's king.",
    ),
    row(
        Th::CastleOppositeSide,
        O,
        "Castle opposite",
        "Castle on the other side and race with pawns.",
    ),
    row(
        Th::PawnToCenter,
        O,
        "Take the centre",
        "Put a pawn on a central square.",
    ),
    row(
        Th::GainTempo,
        O,
        "Gain time",
        "Develop with an attack, so the opponent has to answer.",
    ),
    row(
        Th::ConnectRooks,
        O,
        "Connect the rooks",
        "Clear the back rank so the rooks see each other.",
    ),
    row(
        Th::DoublesPawns,
        S,
        "Doubled pawns",
        "Two pawns of the same side on one file.",
    ),
    row(
        Th::IsolatesPawn,
        S,
        "Isolated pawn",
        "A pawn with no friendly pawn on the files next to it.",
    ),
    row(
        Th::BackwardPawn,
        S,
        "Backward pawn",
        "A pawn left behind its neighbours that cannot safely advance.",
    ),
    row(
        Th::PawnInFrontOfKing,
        S,
        "King's pawn cover",
        "Moving the pawns in front of a castled king loosens it.",
    ),
    row(
        Th::PassiveSquare,
        S,
        "Passive piece",
        "A piece went where it does much less.",
    ),
    row(
        Th::Outpost,
        S,
        "Outpost",
        "A square a pawn guards and no enemy pawn can attack.",
    ),
    row(
        Th::CentraliseKnight,
        S,
        "Centralise the knight",
        "A knight in the centre reaches the most squares.",
    ),
    row(
        Th::RookOnSeventh,
        S,
        "Rook on the seventh",
        "A rook on the opponent's second rank attacks pawns and boxes in the king.",
    ),
    row(
        Th::TakeOpenFile,
        S,
        "Open file",
        "A rook belongs on a file with no pawns.",
    ),
    row(
        Th::ActiveSquare,
        S,
        "Activate a piece",
        "Move a piece to where it does more.",
    ),
    row(
        Th::StrikeAtCenter,
        S,
        "Strike at the centre",
        "Hit the opponent's central pawns with a pawn.",
    ),
    row(
        Th::CentralBreak,
        S,
        "Central break",
        "Open the centre with a pawn break.",
    ),
    row(
        Th::PrepareCentralPush,
        S,
        "Prepare a central push",
        "Support the square a central pawn wants to reach.",
    ),
    row(
        Th::AttackPawnChain,
        S,
        "Attack the pawn chain",
        "Hit a chain of pawns at its base.",
    ),
    row(
        Th::MakePassedPawn,
        S,
        "Make a passed pawn",
        "Create a pawn no enemy pawn can stop.",
    ),
    row(
        Th::PushPassedPawn,
        S,
        "Push the passed pawn",
        "A passed pawn wants to move on.",
    ),
    row(
        Th::CreateLuft,
        S,
        "Make room for the king",
        "Give the castled king an escape square.",
    ),
    row(
        Th::EndgameKingAndPawn,
        E,
        "King and pawn endings",
        "Only kings and pawns: every king move counts.",
    ),
    row(
        Th::EndgameRookAndPawn,
        E,
        "Rook endings",
        "Rooks and pawns: the most common endings.",
    ),
    row(
        Th::EndgameQueen,
        E,
        "Queen endings",
        "Queens and pawns: watch for checks.",
    ),
    row(
        Th::EndgameOther,
        E,
        "Minor piece endings",
        "Knights, bishops and endings without pawns.",
    ),
];

/// Every theme, in display order.
pub fn all() -> Vec<ThemeInfo> {
    TABLE
        .iter()
        .map(|&(theme, family, name, about)| ThemeInfo {
            theme,
            family,
            name: name.to_owned(),
            about: about.to_owned(),
        })
        .collect()
}

/// The wording of one theme.
pub fn info(theme: TrainTheme) -> ThemeInfo {
    all()
        .into_iter()
        .find(|t| t.theme == theme)
        .expect("every theme is in the table")
}

/// The tactical theme of a coach tag, in the order the drill maker looks for them (fork first).
/// The reference's theme lists, mapped onto our tags.
pub const TACTIC_ORDER: [TrainTheme; 6] = [
    TrainTheme::Fork,
    TrainTheme::Skewer,
    TrainTheme::Pin,
    TrainTheme::Discovery,
    TrainTheme::RemovesDefender,
    TrainTheme::TrappedPiece,
];

/// The tactical theme a tag belongs to, if any.
pub fn tactic_theme(tag: Tag) -> Option<TrainTheme> {
    Some(match tag {
        Tag::RoyalFork | Tag::Fork => TrainTheme::Fork,
        Tag::Skewer => TrainTheme::Skewer,
        Tag::Pin | Tag::ExploitsPin => TrainTheme::Pin,
        Tag::DoubleCheck | Tag::DiscoveredCheck | Tag::DiscoveredAttack => TrainTheme::Discovery,
        Tag::RemovesDefender | Tag::Overloaded => TrainTheme::RemovesDefender,
        Tag::TrappedPiece => TrainTheme::TrappedPiece,
        _ => return None,
    })
}

/// Tags that take a free piece (the free-piece bucket).
pub fn is_free_piece(tag: Tag) -> bool {
    matches!(tag, Tag::FreePiece | Tag::WinsTrade)
}

/// Opening bad habits of the played move (said about the move itself).
pub fn opening_error(tag: Tag) -> Option<TrainTheme> {
    Some(match tag {
        Tag::TooEarlyQueen => TrainTheme::QueenOutEarly,
        Tag::MovesSamePiece => TrainTheme::SamePieceTwice,
        Tag::LosesCastling => TrainTheme::LosesCastling,
        Tag::WeakensKing => TrainTheme::PlaysF3,
        _ => return None,
    })
}

/// Opening ideas the best move had.
pub fn opening_idea(tag: Tag) -> Option<TrainTheme> {
    Some(match tag {
        Tag::DevelopsOpening | Tag::DevelopsMiddlegame => TrainTheme::Develop,
        Tag::DevelopsCenter => TrainTheme::DevelopToCenter,
        Tag::PreparesBishopDevelopment => TrainTheme::PrepareBishop,
        Tag::BishopGainsScope => TrainTheme::BishopScope,
        Tag::Fianchetto => TrainTheme::Fianchetto,
        Tag::PreparesToCastleKingside => TrainTheme::PrepareCastleKingside,
        Tag::PreparesToCastleQueenside => TrainTheme::PrepareCastleQueenside,
        Tag::CastlesKingside => TrainTheme::CastleKingside,
        Tag::CastlesQueenside => TrainTheme::CastleQueenside,
        Tag::CastlesSameSide => TrainTheme::CastleSameSide,
        Tag::CastlesOppositeSide => TrainTheme::CastleOppositeSide,
        Tag::PutsPawnInCenter => TrainTheme::PawnToCenter,
        Tag::GainsTempo | Tag::Kicks => TrainTheme::GainTempo,
        Tag::ConnectsRooks => TrainTheme::ConnectRooks,
        _ => return None,
    })
}

/// Middlegame errors of the played move.
pub fn strategy_error(tag: Tag) -> Option<TrainTheme> {
    Some(match tag {
        Tag::DoubledPawns => TrainTheme::DoublesPawns,
        Tag::IsolatedPawn => TrainTheme::IsolatesPawn,
        Tag::BackwardPawn => TrainTheme::BackwardPawn,
        Tag::MovesPawnInFrontOfKing => TrainTheme::PawnInFrontOfKing,
        Tag::MovesToPassiveSquare => TrainTheme::PassiveSquare,
        _ => return None,
    })
}

/// Middlegame ideas the best move had.
pub fn strategy_idea(tag: Tag) -> Option<TrainTheme> {
    Some(match tag {
        Tag::Outpost => TrainTheme::Outpost,
        Tag::CentralizesKnight => TrainTheme::CentraliseKnight,
        Tag::BishopGainsScope => TrainTheme::BishopScope,
        Tag::ConnectsRooks => TrainTheme::ConnectRooks,
        Tag::RookOnSeventh => TrainTheme::RookOnSeventh,
        Tag::TakesOpenFile => TrainTheme::TakeOpenFile,
        Tag::MovesToActiveSquare => TrainTheme::ActiveSquare,
        Tag::StrikesAtCenter => TrainTheme::StrikeAtCenter,
        Tag::CentralBreak => TrainTheme::CentralBreak,
        Tag::PreparesCentralPush => TrainTheme::PrepareCentralPush,
        Tag::PutsPawnInCenter => TrainTheme::PawnToCenter,
        Tag::AttacksPawnChain => TrainTheme::AttackPawnChain,
        Tag::MakesPassedPawn => TrainTheme::MakePassedPawn,
        Tag::PushesPassedPawn | Tag::PushesPassedPawnEndgame => TrainTheme::PushPassedPawn,
        Tag::CreatesLuft => TrainTheme::CreateLuft,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_has_one_row() {
        let rows = all();
        for (i, a) in rows.iter().enumerate() {
            assert!(
                rows[i + 1..].iter().all(|b| b.theme != a.theme),
                "{:?} twice",
                a.theme
            );
        }
        // Every tag mapping lands on a theme of the table.
        for tag in Tag::ALL {
            for theme in [
                tactic_theme(tag),
                opening_error(tag),
                opening_idea(tag),
                strategy_error(tag),
                strategy_idea(tag),
            ]
            .into_iter()
            .flatten()
            {
                info(theme);
            }
        }
        assert_eq!(info(TrainTheme::Fork).family, ThemeFamily::Tactics);
    }
}
