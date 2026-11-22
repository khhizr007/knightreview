//! The reference game-phase rule (`board::reference_phase`) on 20,000 positions from the Lichess
//! puzzle database (CC0), against phases computed by an independent port of the same rule that
//! agreed with the reference engine on every one of them.

use review_core::{Phase, PhaseRule};
use review_server::board::{game_phases, reference_phase};
use shakmaty::fen::Fen;
use shakmaty::{CastlingMode, Chess, Position};

const FIXTURE: &str = include_str!("fixtures/phase_positions.tsv");

#[test]
fn reference_phase_matches_the_fixture_on_every_position() {
    let mut checked = 0;
    let mut wrong = Vec::new();
    for line in FIXTURE.lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
        let (fen, want) = line.split_once('\t').expect("fen and phase");
        let pos: Chess = fen
            .parse::<Fen>()
            .expect("fixture FEN parses")
            .into_position(CastlingMode::Standard)
            .expect("fixture position is legal");
        let want = match want.trim() {
            "opening" => Phase::Opening,
            "middlegame" => Phase::Middlegame,
            "endgame" => Phase::Endgame,
            other => panic!("unknown phase {other}"),
        };
        let got = reference_phase(&pos);
        if got != want && wrong.len() < 10 {
            wrong.push(format!("{fen}: got {got:?}, want {want:?}"));
        }
        checked += 1;
    }
    assert_eq!(checked, 20_000);
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn the_middlegame_holds_while_the_next_move_captures() {
    // Qxd8+ trades queens, which takes the material weight below 10, but the recapture comes
    // next: the middlegame holds until Rxd8 is played.
    let start: Chess = "r2q2k1/8/2n5/8/8/2N5/8/R2Q2K1 w - - 0 20"
        .parse::<Fen>()
        .unwrap()
        .into_position(CastlingMode::Standard)
        .unwrap();
    let mut positions = vec![start.clone()];
    let mut moves = Vec::new();
    let mut pos = start;
    for san in ["Qxd8+", "Rxd8"] {
        let m = san
            .parse::<shakmaty::san::San>()
            .unwrap()
            .to_move(&pos)
            .unwrap();
        pos.play_unchecked(m);
        moves.push(m);
        positions.push(pos.clone());
    }
    assert_eq!(reference_phase(&positions[1]), Phase::Endgame);
    assert_eq!(
        game_phases(&positions, &moves, PhaseRule::Reference),
        [Phase::Middlegame, Phase::Middlegame, Phase::Endgame]
    );
}
