# Trainer

The trainer turns the player's own games into short drills. Every drill starts from a position
the player reached, names one idea, and grades the idea rather than only the exact move. Progress
lives in the browser; the server stays stateless.

## Flow

1. A review finishes. The server runs `train::generate` on it (no engine search) and sends the
   drills in `ReviewResult.train_items`.
2. The browser keeps the drills of the player's side (the review's `player_side`, or the side
   whose name is one of the player's usernames) in IndexedDB (`knightreview-trainer`).
3. A session picks drills by schedule and weakness. A drill shows the position before the
   opponent's last move, plays that move (the *pitch* move), and asks for the player's move.
4. The browser engine searches the position after the move when the drill's stored engine lines
   do not cover it; `POST /api/train/grade` grades it with the review's own rules.

## Drill kinds

Tried in this order; the first that applies to a move wins. Inaccuracies make no drills.

| Kind | When | Theme from |
|---|---|---|
| Opening | Mistake, Blunder or Miss in the opening | a bad habit of the move, else an idea the best move had |
| Strategy | the same in the middlegame, the best line not already lost and the move not getting mated | a pawn or piece error of the move, else an idea of the best move |
| Endgame | the best move keeps at least 0.00, the game move drops to −1.00 or worse, no mate scores, neither line wins or loses more than a pawn | the endgame family; the concept found on the best line |
| Miss | a Miss | free piece, then a tactic winning material at once, then later in the line; a missed mate |
| Blunder | a Blunder from a better position | what the move allowed, as for a Miss |

The endgame kind is behind `TrainConfig.endgame_maker` (off by default). On 577 stored
calibration reviews it makes 73 drills; a hand check of them found technique errors, with some
trades and one-pawn grabs among the best moves.

Limits: at most `max_per_game` drills per side (largest losses first), no drills in time trouble
(under 10 s or 5 % of the starting time), none where the best move is the only legal move or a
forced recapture, none in Chess960 games.

The game phase of drills uses the reference phase rule (`board::reference_phase`) whatever the
review's own `phase_rule`.

## Grading

| Drill | Correct | Not incorrect | Incorrect |
|---|---|---|---|
| Miss, Blunder | the best move, or a move that carries the drill's idea (same tactic, a free piece, a Brilliant sacrifice, a mate) | any other move graded Good or better | Inaccuracy or worse; any move but the best in a critical position |
| Opening, Strategy, Endgame | the best move, or any move graded Good or better | – | Inaccuracy or worse |

The move played in the game is *same as game*. Without an engine line for the move, or without
the server, only the best move is correct.

## Schedule

Leitner boxes 0–5 with 0, 1, 3, 7, 16 and 35 days. A clean first try moves a drill up a box (from
the top box it retires); a small hint or a playable move keeps the box; a wrong move or a shown
move sends it back to box 0. Each theme keeps a success rate with a half-life of 10 attempts:
new (under 3 attempts), shaky (under 60 %), learning, solid (over 85 % after 8 or more).

A session of 10: 4 due drills of the weakest family (endgames count as weaker until solid),
3 due drills of the others, 2 new drills from the latest games, 1 mastered drill, filled with new
drills; never two drills of one game in a row where it can be helped.

## Endgame Lab

- Play-out: an endgame drill whose best line holds for 6 plies or more goes on as a game against
  the browser engine after the right first move: win it (best line at +2.00 or more) or hold it
  (below +0.50). A win counts by mate, or after a promotion held at +5.00 for 4 moves; it slips at
  +0.50. A draw counts by a drawn ending or the move limit (twice the line plus 10 plies); it slips
  at −2.00.
- Exact verdicts: when the player turns it on, positions with 7 pieces or fewer are sent to the
  Lichess tablebase API (`tablebase.lichess.ovh`) and its verdict decides the play-out. Answers
  are cached in the browser. Only the FEN is sent.
- Stats from stored games: endgame accuracy by family (decided positions left out), the score
  after entering the endgame ahead, level or behind (±2.00), and the known endings reached.
- Concept cards with positions composed for this project; every answer list was checked against
  the tablebase.

## API

- `POST /api/train/grade`: `TrainGradeRequest { item, user_move, before?, after? }` →
  `TrainGradeResponse { grade, user_move, classification, miss_kind, ep_lost, on_theme,
  fallback, reply, coach, show_line }`.
- `POST /api/train/generate`: `{ review, config? }` → `{ items, generator_version }`, for drills
  of an older generator version.
- `GET /api/train/themes`: every theme with its family, name and meaning.

## Not done yet

- Board-event tags the drills could use but the coach does not detect yet: castling into an
  attack, opening the bishop's scope, the first-move centre pawn, preventing castling, the
  development variants (last piece, with tempo, recapturing), the opposition and rook-behind-pawn
  tags as coach sentences, trades into won or drawn pawn endings, ladder mates, blockades and the
  remaining strategy tags (space, rook lifts, doubled rooks, pawn-chain repairs).
- The position attribute of strategy drills (attacking, defending, space, activity, structure).
- "Play the next book move" drills from where the player leaves the book.
- Per-phase accuracy measured against the reference with the new phase rule.
