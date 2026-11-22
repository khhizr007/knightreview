//! Endgame facts for the trainer: the family of an ending, the known endings by material, and
//! the idea the best move of a technique drill carries out.

use shakmaty::{Bitboard, Board, Chess, Color, File, Move, Position, Rank, Role, Square};

use super::{EndgameConcept, EndgameFamily, PositionType};
use crate::board;
use crate::coach::facts;

/// The family of an ending by what is on the board besides kings and pawns (see
/// [`EndgameFamily`]).
pub fn family(board: &Board) -> EndgameFamily {
    if board.pawns().is_empty() {
        EndgameFamily::Other
    } else if board.queens().any() {
        EndgameFamily::Queen
    } else if (board.knights() | board.bishops()).any() {
        EndgameFamily::Other
    } else if board.rooks().any() {
        EndgameFamily::RookAndPawn
    } else {
        EndgameFamily::KingAndPawn
    }
}

/// Known endings by material, each named once (either side may have the extra material). The
/// set the reference recognises (recovered); names list the stronger side's pieces in the
/// usual order.
const KNOWN: [(&str, &str); 21] = [
    ("KP", "K"),
    ("KPP", "K"),
    ("KQ", "K"),
    ("KR", "K"),
    ("KR", "KP"),
    ("KBN", "K"),
    ("KPP", "KR"),
    ("KP", "KB"),
    ("KP", "KN"),
    ("KQP", "KQ"),
    ("KQ", "KP"),
    ("KR", "KB"),
    ("KRP", "KR"),
    ("KRPP", "KR"),
    ("KRB", "KB"),
    ("KRB", "KR"),
    ("KR", "KR"),
    ("KR", "KN"),
    ("KBP", "KB"),
    ("KRN", "KR"),
    ("KPPP", "KR"),
];

/// One side's material as letters: K, then Q, R, B, N and P as many times as there are.
fn letters(board: &Board, color: Color) -> String {
    let own = board.by_color(color);
    let mut out = String::from("K");
    for (role, ch) in [
        (Role::Queen, 'Q'),
        (Role::Rook, 'R'),
        (Role::Bishop, 'B'),
        (Role::Knight, 'N'),
        (Role::Pawn, 'P'),
    ] {
        for _ in 0..(own & board.by_role(role)).count() {
            out.push(ch);
        }
    }
    out
}

/// The name of a known ending ("KRPvKR"), when the material on the board is one.
pub fn known_ending(board: &Board) -> Option<String> {
    let white = letters(board, Color::White);
    let black = letters(board, Color::Black);
    KNOWN
        .iter()
        .find(|(a, b)| (white == *a && black == *b) || (white == *b && black == *a))
        .map(|(a, b)| format!("{a}v{b}"))
}

/// Only kings and pawns are on the board.
pub fn is_pawn_ending(board: &Board) -> bool {
    (board.kings() | board.pawns()) == board.occupied()
}

/// The pawn structure of a middlegame position, roughly as the reference reads it: closed when
/// the d- and e-pawns are both locked head to head, open when at most one pawn is left on the
/// d- and e-files, semi-open otherwise. An approximation (the reference's rule also reads
/// half-open files and was not recovered in full).
pub fn position_type(pos: &Chess) -> Option<PositionType> {
    let board = pos.board();
    let pawns = board.pawns();
    let centre = Bitboard::from_file(File::D) | Bitboard::from_file(File::E);
    if (pawns & centre).count() < 2 {
        return Some(PositionType::Open);
    }
    let locked = |file: File| {
        let white = board.white() & pawns & Bitboard::from_file(file);
        let black = board.black() & pawns & Bitboard::from_file(file);
        white
            .into_iter()
            .any(|sq| sq.offset(8).is_some_and(|ahead| black.contains(ahead)))
    };
    if locked(File::D) && locked(File::E) {
        Some(PositionType::Closed)
    } else {
        Some(PositionType::SemiOpen)
    }
}

/// Distance of a square from the four centre squares (0 on them, 3 in a corner).
fn centre_distance(sq: Square) -> i32 {
    let f = i32::from(sq.file() as u8);
    let r = i32::from(sq.rank() as u8);
    let df = (2 * f - 7).abs() / 2;
    let dr = (2 * r - 7).abs() / 2;
    df.max(dr)
}

/// How far up the board a square is for `color` (0 on its own back rank).
fn advance(sq: Square, color: Color) -> i32 {
    let r = i32::from(sq.rank() as u8);
    if color == Color::White { r } else { 7 - r }
}

/// The move takes the direct opposition (the reference's `gains_direct_opposition`, recovered,
/// without its easy-to-find test): a king move in a position with only kings and pawns, after
/// which the kings stand on one file two ranks apart or on one rank two files apart with the
/// square between them empty, and the opponent's king is at least as far up the board as ours,
/// seen from our side.
pub fn takes_opposition(before: &Chess, m: Move) -> bool {
    if m.is_castle() || m.role() != Role::King || !is_pawn_ending(before.board()) {
        return false;
    }
    let mover = before.turn();
    let mut after = before.clone();
    after.play_unchecked(m);
    let board = after.board();
    let (Some(ours), Some(theirs)) = (board.king_of(mover), board.king_of(mover.other())) else {
        return false;
    };
    let df = i32::from(ours.file() as u8) - i32::from(theirs.file() as u8);
    let dr = i32::from(ours.rank() as u8) - i32::from(theirs.rank() as u8);
    let facing = (df == 0 && dr.abs() == 2) || (dr == 0 && df.abs() == 2);
    if !facing {
        return false;
    }
    let between = Square::from_coords(
        File::new(((i32::from(ours.file() as u8) + i32::from(theirs.file() as u8)) / 2) as u32),
        Rank::new(((i32::from(ours.rank() as u8) + i32::from(theirs.rank() as u8)) / 2) as u32),
    );
    !board.occupied().contains(between) && advance(theirs, mover) >= advance(ours, mover)
}

/// The rook moved to stand behind a passed pawn on its file, its own or the opponent's, with
/// nothing between: behind is the side the pawn came from.
pub fn rook_behind_passer(before: &Chess, m: Move) -> bool {
    if m.role() != Role::Rook || m.is_castle() {
        return false;
    }
    let mut after = before.clone();
    after.play_unchecked(m);
    let board = after.board();
    let rook = m.to();
    let file_pawns = board.pawns() & Bitboard::from_file(rook.file());
    file_pawns.into_iter().any(|p| {
        let Some(color) = board.color_at(p) else {
            return false;
        };
        if !facts::is_passed(board, p) {
            return false;
        }
        // The pawn moves toward rank 8 (White) or rank 1 (Black); behind it is the other way.
        let behind = if color == Color::White {
            rook.rank() < p.rank()
        } else {
            rook.rank() > p.rank()
        };
        behind && (shakmaty::attacks::between(rook, p) & board.occupied()).is_empty()
    })
}

/// A piece (not a pawn) moved onto the square right in front of an enemy passed pawn.
pub fn blockades(before: &Chess, m: Move) -> bool {
    if m.role() == Role::Pawn || m.is_castle() {
        return false;
    }
    let mover = before.turn();
    let enemy = mover.other();
    let board = before.board();
    let to = m.to();
    let behind = if enemy == Color::White {
        to.offset(-8)
    } else {
        to.offset(8)
    };
    behind.is_some_and(|p| {
        board.piece_at(p)
            == Some(shakmaty::Piece {
                color: enemy,
                role: Role::Pawn,
            })
            && facts::is_passed(board, p)
    })
}

/// The move pushes a pawn that was passed.
pub fn pushes_passer(before: &Chess, m: Move) -> bool {
    m.role() == Role::Pawn
        && !m.is_capture()
        && m.from()
            .is_some_and(|from| facts::is_passed(before.board(), from))
}

/// The king walks toward the centre.
pub fn king_toward_centre(m: Move) -> bool {
    m.role() == Role::King
        && !m.is_castle()
        && m.from()
            .is_some_and(|from| centre_distance(m.to()) < centre_distance(from))
}

/// The best line reaches a king-and-pawn ending: it starts with a capture and, once the
/// captures of its first four plies are played, only kings and pawns are left.
pub fn trades_into_pawn_ending(before: &Chess, line: &[String]) -> bool {
    let mut pos = before.clone();
    let mut first = true;
    for (k, uci) in line.iter().enumerate().take(6) {
        let Some(m) = board::parse_uci(&pos, uci) else {
            return false;
        };
        if first && !m.is_capture() {
            return false;
        }
        first = false;
        pos.play_unchecked(m);
        // Settled: the next move is not a capture.
        let next_captures = line
            .get(k + 1)
            .and_then(|u| board::parse_uci(&pos, u))
            .is_some_and(|n| n.is_capture());
        if !next_captures {
            return is_pawn_ending(pos.board());
        }
    }
    false
}

/// A basic mate: one side has only a queen or a rook (and its king) against a bare king.
pub fn basic_mate(board: &Board, mover: Color) -> bool {
    let own = letters(board, mover);
    let theirs = letters(board, mover.other());
    theirs == "K" && (own == "KQ" || own == "KR")
}

/// The endgame idea of the best move `best` from `before`, along its line, in the order they are
/// looked for: a basic mate, a promotion, a trade into a pawn ending, the opposition, a rook
/// behind a passed pawn, a blockade, a passed pawn moving on, the king walking in.
pub fn concept(before: &Chess, best: Move, line: &[String]) -> Option<EndgameConcept> {
    let board = before.board();
    let mover = before.turn();
    if basic_mate(board, mover) {
        Some(EndgameConcept::MatingTechnique)
    } else if best.is_promotion() {
        Some(EndgameConcept::Promotion)
    } else if !is_pawn_ending(board) && trades_into_pawn_ending(before, line) {
        Some(EndgameConcept::TradeIntoPawnEnding)
    } else if takes_opposition(before, best) {
        Some(EndgameConcept::Opposition)
    } else if rook_behind_passer(before, best) {
        Some(EndgameConcept::RookBehindPasser)
    } else if blockades(before, best) {
        Some(EndgameConcept::Blockade)
    } else if pushes_passer(before, best) {
        Some(EndgameConcept::PassedPawnPush)
    } else if king_toward_centre(best) {
        Some(EndgameConcept::KingActivity)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shakmaty::CastlingMode;
    use shakmaty::fen::Fen;

    fn pos(fen: &str) -> Chess {
        fen.parse::<Fen>()
            .unwrap()
            .into_position(CastlingMode::Standard)
            .unwrap()
    }

    fn mv(p: &Chess, uci: &str) -> Move {
        board::parse_uci(p, uci).unwrap()
    }

    #[test]
    fn families_follow_the_material() {
        let f = |fen: &str| family(pos(fen).board());
        assert_eq!(
            f("8/5k2/8/3p4/3P4/8/5K2/8 w - - 0 1"),
            EndgameFamily::KingAndPawn
        );
        assert_eq!(
            f("8/5k2/8/3p4/3P4/8/5K2/R7 w - - 0 1"),
            EndgameFamily::RookAndPawn
        );
        assert_eq!(
            f("8/5k2/8/3p4/3P4/8/5K2/Q7 w - - 0 1"),
            EndgameFamily::Queen
        );
        assert_eq!(
            f("8/5k2/8/3p4/3P4/8/5K2/B7 w - - 0 1"),
            EndgameFamily::Other
        );
        assert_eq!(f("8/5k2/8/8/8/8/5K2/R7 w - - 0 1"), EndgameFamily::Other);
    }

    #[test]
    fn known_endings_are_named_whichever_side_has_them() {
        let k = |fen: &str| known_ending(pos(fen).board());
        assert_eq!(
            k("8/5k2/8/3p4/8/8/5K2/R7 w - - 0 1").as_deref(),
            Some("KRvKP")
        );
        assert_eq!(
            k("r7/5k2/8/3P4/8/8/5K2/R7 w - - 0 1").as_deref(),
            Some("KRPvKR")
        );
        assert_eq!(
            k("8/5k2/8/8/8/8/5K2/1BN5 w - - 0 1").as_deref(),
            Some("KBNvK")
        );
        assert_eq!(
            k("r7/5k2/8/8/8/8/5K2/RN6 w - - 0 1").as_deref(),
            Some("KRNvKR")
        );
        assert_eq!(k("8/5k2/8/3p4/3P4/8/5K2/8 w - - 0 1"), None);
    }

    #[test]
    fn opposition_needs_facing_kings_and_only_pawns() {
        // Kd3-e4 does not face Kd6 (one file and two ranks apart).
        let p = pos("8/8/3k4/8/3P4/3K4/8/8 w - - 0 1");
        assert!(!takes_opposition(&p, mv(&p, "d3e4")));
        let p = pos("8/8/4k3/8/8/3K4/3P4/8 w - - 0 1");
        assert!(takes_opposition(&p, mv(&p, "d3e4")));
        assert!(!takes_opposition(&p, mv(&p, "d3d4")));
        // A rook on the board: not a king-and-pawn ending.
        let p = pos("8/8/4k3/8/8/3K4/3P4/7r w - - 0 1");
        assert!(!takes_opposition(&p, mv(&p, "d3e4")));
    }

    #[test]
    fn rook_behind_a_passed_pawn() {
        // White's a-pawn is passed; Ra1 stands behind it.
        let p = pos("6k1/8/8/P7/8/8/5K2/7R w - - 0 1");
        assert!(rook_behind_passer(&p, mv(&p, "h1a1")));
        assert!(!rook_behind_passer(&p, mv(&p, "h1h8")));
        // Behind the opponent's passed pawn: Black's a-pawn walks down, so behind is above it.
        let p = pos("6k1/8/8/8/p7/8/5K2/7R w - - 0 1");
        assert!(!rook_behind_passer(&p, mv(&p, "h1a1")));
        let p = pos("7R/6k1/8/8/p7/8/5K2/8 w - - 0 1");
        assert!(rook_behind_passer(&p, mv(&p, "h8a8")));
    }

    #[test]
    fn concepts_of_simple_endings() {
        let p = pos("8/8/4k3/8/8/3K4/3P4/8 w - - 0 1");
        assert_eq!(
            concept(&p, mv(&p, "d3e4"), &[]),
            Some(EndgameConcept::Opposition)
        );
        let p = pos("7k/8/8/8/8/8/1K6/Q7 w - - 0 1");
        assert_eq!(
            concept(&p, mv(&p, "a1a7"), &[]),
            Some(EndgameConcept::MatingTechnique)
        );
        // Rxd5 Kxd5 leaves only kings and pawns.
        let p = pos("8/8/4k3/3r4/8/3R4/5KPP/8 w - - 0 1");
        let line = vec!["d3d5".to_owned(), "e6d5".to_owned()];
        assert_eq!(
            concept(&p, mv(&p, "d3d5"), &line),
            Some(EndgameConcept::TradeIntoPawnEnding)
        );
        let p = pos("8/1P3k2/8/8/8/8/5K2/8 w - - 0 1");
        assert_eq!(
            concept(&p, mv(&p, "b7b8q"), &[]),
            Some(EndgameConcept::Promotion)
        );
    }

    #[test]
    fn position_types() {
        // The French advance: d4/d5 and e5/e6 both stand head to head.
        let p = pos("rnbqkbnr/pp3ppp/4p3/2ppP3/3P4/8/PPP2PPP/RNBQKBNR w KQkq - 0 4");
        assert_eq!(position_type(&p), Some(PositionType::Closed));
        // Only the d4 pawn faces d5: semi-open.
        let p = pos("rnbqkbnr/ppp2ppp/4p3/3p4/3P4/8/PPP1PPPP/RNBQKBNR w KQkq - 0 3");
        assert_eq!(position_type(&p), Some(PositionType::SemiOpen));
        let p = pos("r1bqkbnr/pppp1ppp/2n5/8/8/5N2/PPP2PPP/RNBQKB1R w KQkq - 0 4");
        assert_eq!(position_type(&p), Some(PositionType::Open));
    }
}
