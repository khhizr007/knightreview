//! Board helpers on top of `shakmaty`: material, perspective, FEN, phases.

use review_core::{Phase, PhaseRule, Side};
use shakmaty::fen::{Epd, Fen};
use shakmaty::uci::UciMove;
use shakmaty::zobrist::Zobrist64;
use shakmaty::{
    Board, CastlingMode, Chess, Color, EnPassantMode, Move, Position, Rank, Role, Square,
};

pub fn side(color: Color) -> Side {
    match color {
        Color::White => Side::White,
        Color::Black => Side::Black,
    }
}

/// Material values used by the reference engine's sacrifice gates (cp).
pub fn role_value(role: Role) -> i32 {
    match role {
        Role::Pawn => 100,
        Role::Knight | Role::Bishop => 300,
        Role::Rook => 500,
        Role::Queen => 900,
        Role::King => 0,
    }
}

pub fn material(board: &Board, color: Color) -> i32 {
    let m = board.material_side(color);
    i32::from(m.pawn) * 100
        + i32::from(m.knight) * 300
        + i32::from(m.bishop) * 300
        + i32::from(m.rook) * 500
        + i32::from(m.queen) * 900
}

/// Material balance from `color`'s point of view (cp).
pub fn balance(board: &Board, color: Color) -> i32 {
    material(board, color) - material(board, color.other())
}

pub fn fen(pos: &Chess) -> String {
    Fen::from_position(pos, EnPassantMode::Legal).to_string()
}

/// Position key without move counters (used for book lookup and repetition).
pub fn epd(pos: &Chess) -> String {
    Epd::from_position(pos, EnPassantMode::Legal).to_string()
}

/// Polyglot key of the position (64-bit Zobrist hash, Polyglot book format; en passant
/// counted only when a legal capture exists).
pub fn polyglot_key(pos: &Chess) -> u64 {
    pos.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0
}

pub fn uci(m: Move, mode: CastlingMode) -> String {
    UciMove::from_move(m, mode).to_string()
}

pub fn parse_uci(pos: &Chess, text: &str) -> Option<Move> {
    let uci: UciMove = text.parse().ok()?;
    uci.to_move(pos).ok()
}

/// Draw by rule visible on the board alone (repetition is tracked by the caller).
pub fn is_board_draw(pos: &Chess) -> bool {
    pos.is_stalemate() || pos.is_insufficient_material() || pos.halfmoves() >= 100
}

/// The position with the other side to move and no en passant square (a null
/// move). `None` when the side to move is in check, when the result is not a
/// legal position, or when the new side to move has no legal move.
pub fn null_move(pos: &Chess) -> Option<Chess> {
    if pos.is_check() {
        return None;
    }
    pos.clone()
        .swap_turn()
        .ok()
        .filter(|p| !p.legal_moves().is_empty())
}

/// Non-pawn, non-king pieces on the board (both colours).
fn piece_count(board: &Board) -> u32 {
    (board.knights() | board.bishops() | board.rooks() | board.queens()).count() as u32
}

fn back_rank_sparse(board: &Board, color: Color) -> bool {
    let rank = if color == Color::White {
        Rank::First
    } else {
        Rank::Eighth
    };
    let own_on_rank = (board.by_color(color) & shakmaty::Bitboard::from_rank(rank)).count();
    own_on_rank < 4
}

/// Fallback phase heuristic (functional spec §9, open question 9). Callers
/// make it monotonic over the game.
pub fn raw_phase(board: &Board, ply: u32) -> Phase {
    let pieces = piece_count(board);
    if pieces <= 6 {
        Phase::Endgame
    } else if pieces <= 10
        || back_rank_sparse(board, Color::White)
        || back_rank_sparse(board, Color::Black)
        || ply >= 40
    {
        Phase::Middlegame
    } else {
        Phase::Opening
    }
}

/// The reference's phase of a position on its own (recovered; checked against the reference on
/// 20,000 positions): an endgame below a material weight of 10 (minor = 1, rook = 2, queen = 3,
/// both sides), else a middlegame when the development score passes 25. The score grows as
/// pieces and pawns leave their home ranks, queens come off and castling rights go.
pub fn reference_phase(pos: &Chess) -> Phase {
    let board = pos.board();
    let minors = (board.knights() | board.bishops()).count() as i32;
    let rooks = board.rooks().count() as i32;
    let queens = board.queens().count() as i32;
    if minors + 2 * rooks + 3 * queens < 10 {
        return Phase::Endgame;
    }
    let mut home = [0i32; 4]; // minors, rooks and queens on their own back rank; pawns on their first two ranks
    for color in [Color::White, Color::Black] {
        let own = board.by_color(color);
        let (back, second) = if color == Color::White {
            (Rank::First, Rank::Second)
        } else {
            (Rank::Eighth, Rank::Seventh)
        };
        let back = shakmaty::Bitboard::from_rank(back);
        let start = back | shakmaty::Bitboard::from_rank(second);
        home[0] += (own & (board.knights() | board.bishops()) & back).count() as i32;
        home[1] += (own & board.rooks() & back).count() as i32;
        home[2] += (own & board.queens() & back).count() as i32;
        home[3] += (own & board.pawns() & start).count() as i32;
    }
    let pawns = board.pawns().count() as i32;
    let castles = pos.castles();
    let lost = [Color::White, Color::Black]
        .into_iter()
        .filter(|c| !castles.has_color(*c))
        .count() as i32
        * 3;
    let score = 3 * (16 - minors - rooks - home[1]) - 2 * home[0] - pawns - home[3] - 4 * home[2]
        + 5 * (2 - queens)
        + lost
        + 56;
    if score > 25 {
        Phase::Middlegame
    } else {
        Phase::Opening
    }
}

/// A development move by the reference's rule: castling, or the first move of a knight, bishop,
/// rook or queen from its starting square. `moved` holds the squares earlier moves left from or
/// went to: a piece on an untouched starting square has not moved yet.
fn is_development_move(before: &Chess, m: Move, moved: shakmaty::Bitboard) -> bool {
    if m.is_castle() {
        return true;
    }
    let Some(from) = m.from() else { return false };
    let color = before.turn();
    let start = match m.role() {
        Role::Knight => shakmaty::Bitboard::from(Square::B1) | Square::G1,
        Role::Bishop => shakmaty::Bitboard::from(Square::C1) | Square::F1,
        Role::Rook => shakmaty::Bitboard::from(Square::A1) | Square::H1,
        Role::Queen => shakmaty::Bitboard::from(Square::D1),
        Role::Pawn | Role::King => return false,
    };
    let start = if color == Color::White {
        start
    } else {
        start.flip_vertical()
    };
    start.contains(from) && !moved.contains(from)
}

/// The phase of every position of a game (`positions[k + 1]` follows `moves[k]`).
///
/// - [`PhaseRule::Legacy`]: [`raw_phase`], never going back.
/// - [`PhaseRule::Reference`]: the larger of the previous phase and [`reference_phase`], except
///   that the opening goes on through a move that castles, gives or answers check, captures or
///   develops a piece, and the middlegame goes on while the next move gives or answers check or
///   captures.
pub fn game_phases(positions: &[Chess], moves: &[Move], rule: PhaseRule) -> Vec<Phase> {
    let mut out = Vec::with_capacity(positions.len());
    match rule {
        PhaseRule::Legacy => {
            let mut current = Phase::Opening;
            for (ply, pos) in positions.iter().enumerate() {
                current = current.max(raw_phase(pos.board(), ply as u32));
                out.push(current);
            }
        }
        PhaseRule::Reference => {
            let Some(first) = positions.first() else {
                return out;
            };
            out.push(reference_phase(first));
            let mut moved = shakmaty::Bitboard::EMPTY;
            for (k, pos) in positions.iter().enumerate().skip(1) {
                let parent = out[k - 1];
                let before = &positions[k - 1];
                let phase = match moves.get(k - 1) {
                    Some(&m) => {
                        let holds_opening = parent == Phase::Opening
                            && (m.is_castle()
                                || pos.is_check()
                                || before.is_check()
                                || m.is_capture()
                                || is_development_move(before, m, moved));
                        let next = moves.get(k);
                        let holds_middlegame = parent == Phase::Middlegame
                            && next.is_some_and(|n| {
                                n.is_capture()
                                    || pos.is_check()
                                    || positions.get(k + 1).is_some_and(Position::is_check)
                            });
                        if holds_opening {
                            Phase::Opening
                        } else if holds_middlegame {
                            Phase::Middlegame
                        } else {
                            parent.max(reference_phase(pos))
                        }
                    }
                    None => parent.max(reference_phase(pos)),
                };
                if let Some(&m) = moves.get(k - 1) {
                    if let Some(from) = m.from() {
                        moved.add(from);
                    }
                    moved.add(m.to());
                }
                out.push(phase);
            }
        }
    }
    out
}

/// Squares of a move for board highlighting (castling uses king → destination).
pub fn move_squares(m: Move) -> (Option<Square>, Square) {
    match m {
        Move::Castle { king, rook } => {
            let to = Square::from_coords(
                if rook.file() > king.file() {
                    shakmaty::File::G
                } else {
                    shakmaty::File::C
                },
                king.rank(),
            );
            (Some(king), to)
        }
        other => (other.from(), other.to()),
    }
}

/// The square of the piece a capture takes (behind the destination for en
/// passant).
pub fn captured_square(m: Move) -> Square {
    match m {
        Move::EnPassant { from, to } => Square::from_coords(to.file(), from.rank()),
        other => other.to(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starting_position_is_opening_with_equal_material() {
        let pos = Chess::default();
        assert_eq!(balance(pos.board(), Color::White), 0);
        assert_eq!(material(pos.board(), Color::White), 3900);
        assert_eq!(raw_phase(pos.board(), 0), Phase::Opening);
    }

    #[test]
    fn polyglot_keys_match_the_published_values() {
        // Reference keys from the Polyglot book format description.
        let start = Chess::default();
        assert_eq!(polyglot_key(&start), 0x463b_9618_1691_fc9c);
        let mut pos = start;
        for (san, key) in [
            ("e4", 0x823c_9b50_fd11_4196),
            ("d5", 0x0756_b944_61c5_0fb0),
            ("e5", 0x662f_afb9_65db_29d4),
            ("f5", 0x22a4_8b5a_8e47_ff78), // en passant square f6 is capturable
            ("Ke2", 0x652a_607c_a3f2_42c1),
            ("Kf7", 0x00fd_d303_c946_bdd9),
        ] {
            let m = san
                .parse::<shakmaty::san::San>()
                .unwrap()
                .to_move(&pos)
                .unwrap();
            pos.play_unchecked(m);
            assert_eq!(polyglot_key(&pos), key, "after {san}");
        }
    }

    #[test]
    fn bare_kings_and_pawns_is_endgame() {
        let pos: Chess = "8/5k2/8/3p4/3P4/8/5K2/8 w - - 0 1"
            .parse::<Fen>()
            .unwrap()
            .into_position(CastlingMode::Standard)
            .unwrap();
        assert_eq!(raw_phase(pos.board(), 60), Phase::Endgame);
    }

    fn at(fen: &str) -> Chess {
        fen.parse::<Fen>()
            .unwrap()
            .into_position(CastlingMode::Standard)
            .unwrap()
    }

    #[test]
    fn reference_phase_of_simple_positions() {
        assert_eq!(reference_phase(&Chess::default()), Phase::Opening);
        // A rook and a queen each side: weight 2*2 + 3*2 = 10, not an endgame; no castling
        // rights, so the score is high.
        assert_eq!(
            reference_phase(&at("3qr1k1/5ppp/8/8/8/8/5PPP/3QR1K1 w - - 0 30")),
            Phase::Middlegame
        );
        // Rooks only: weight 8, an endgame.
        assert_eq!(
            reference_phase(&at("3rr1k1/5ppp/8/8/8/8/5PPP/3RR1K1 w - - 0 30")),
            Phase::Endgame
        );
    }

    #[test]
    fn the_reference_opening_holds_through_development_and_captures() {
        let mut pos = Chess::default();
        let mut positions = vec![pos.clone()];
        let mut moves = Vec::new();
        for san in ["e4", "e5", "Nf3", "Nc6", "Bb5", "a6", "Bxc6", "dxc6", "O-O"] {
            let m = san
                .parse::<shakmaty::san::San>()
                .unwrap()
                .to_move(&pos)
                .unwrap();
            pos.play_unchecked(m);
            moves.push(m);
            positions.push(pos.clone());
        }
        let phases = game_phases(&positions, &moves, PhaseRule::Reference);
        assert_eq!(phases.len(), positions.len());
        assert!(phases.iter().all(|p| *p == Phase::Opening), "{phases:?}");
        // Monotonic: never back to an earlier phase.
        assert!(phases.windows(2).all(|w| w[0] <= w[1]));
        let legacy = game_phases(&positions, &moves, PhaseRule::Legacy);
        assert!(legacy.iter().all(|p| *p == Phase::Opening));
    }

    #[test]
    fn development_needs_an_untouched_starting_square() {
        let pos = Chess::default();
        let nf3 = parse_uci(&pos, "g1f3").unwrap();
        assert!(is_development_move(&pos, nf3, shakmaty::Bitboard::EMPTY));
        assert!(!is_development_move(
            &pos,
            nf3,
            shakmaty::Bitboard::from(Square::G1)
        ));
        let e4 = parse_uci(&pos, "e2e4").unwrap();
        assert!(!is_development_move(&pos, e4, shakmaty::Bitboard::EMPTY));
    }
}
