// Endgame play-out (docs/trainer.md §8.1): after the right first move, the drill goes on as a game
// against the engine until the player reaches the goal or lets it slip.
import type { Eval } from "../../api/generated/Eval";
import type { PlayOutGoal } from "../../api/generated/PlayOutGoal";
import type { Side } from "../../api/generated/Side";
import type { Ending } from "../chess";
import type { Verdict } from "./tablebase";

/** A win counts once the player is this far ahead (cp) ... */
export const DECISIVE_CP = 500;
/** ... for this many of their moves in a row after promoting. */
export const HOLD_MOVES = 4;
/** A win slips at or below this (cp), a draw at or below `DRAW_LOST_CP`. */
export const WIN_LOST_CP = 50;
export const DRAW_LOST_CP = -200;

export type PlayOutState = "playing" | "won" | "lost";

export interface PlayOutStep {
  /** The evaluation after the latest move (White's view), if searched. */
  eval: Eval | null;
  /** The game ended on the board, and how. */
  ending: Ending | null;
  /** Who is to move after the latest move. */
  toMove: Side;
  /** The latest move promoted a pawn of the player. */
  promoted: boolean;
  /** The tablebase verdict for the side to move, when it was asked. */
  verdict: Verdict | null;
}

export interface PlayOut {
  goal: PlayOutGoal;
  side: Side;
  /** Plies allowed before a draw goal is met (and a win goal fails). */
  limit: number;
  plies: number;
  /** Player moves in a row at a decisive score since a promotion. */
  held: number;
  promotedOnce: boolean;
  state: PlayOutState;
}

export function startPlayOut(goal: PlayOutGoal, side: Side, bestLineLength: number): PlayOut {
  return { goal, side, limit: 2 * bestLineLength + 10, plies: 0, held: 0, promotedOnce: false, state: "playing" };
}

function forSide(e: Eval, side: Side): number {
  if (e.kind === "cp") return side === "white" ? e.cp : -e.cp;
  return e.winner === side ? 100_000 : -100_000;
}

/** The play-out after one more move (either side's). */
export function stepPlayOut(p: PlayOut, s: PlayOutStep): PlayOut {
  if (p.state !== "playing") return p;
  const next = { ...p, plies: p.plies + 1, promotedOnce: p.promotedOnce || s.promoted };
  const playerMoved = s.toMove !== p.side;
  // The tablebase, when asked, decides: its verdict is for the side to move.
  const tb = s.verdict ? (s.toMove === p.side ? s.verdict : s.verdict === "win" ? "loss" : s.verdict === "loss" ? "win" : "draw") : null;

  if (s.ending === "checkmate") return { ...next, state: playerMoved ? "won" : "lost" };
  if (s.ending) return { ...next, state: p.goal === "draw" ? "won" : "lost" };

  if (p.goal === "win") {
    if (tb === "draw" || tb === "loss") return { ...next, state: "lost" };
    const cp = s.eval ? forSide(s.eval, p.side) : null;
    if (cp !== null && cp <= WIN_LOST_CP) return { ...next, state: "lost" };
    if (playerMoved && next.promotedOnce && (cp ?? 0) >= DECISIVE_CP) {
      next.held = p.held + 1;
      if (next.held >= HOLD_MOVES) return { ...next, state: "won" };
    }
    if (next.plies >= p.limit) return { ...next, state: "lost" };
    return next;
  }
  if (tb === "loss") return { ...next, state: "lost" };
  const cp = s.eval ? forSide(s.eval, p.side) : null;
  if (cp !== null && cp <= DRAW_LOST_CP) return { ...next, state: "lost" };
  if (next.plies >= p.limit) return { ...next, state: "won" };
  return next;
}
