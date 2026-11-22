// Endgame facts in the browser, as the server reads them (train/endgame.rs): the family of an
// ending and the known endings by material.
import { parseFen } from "chessops/fen";
import type { EndgameFamily } from "../../api/generated/EndgameFamily";

interface Counts {
  q: number;
  r: number;
  b: number;
  n: number;
  p: number;
}

interface Material {
  white: Counts;
  black: Counts;
}

/** The pieces on the board of a FEN, by side (kings left out); null for an invalid FEN. */
export function material(fen: string): Material | null {
  const setup = parseFen(fen);
  if (setup.isErr) return null;
  const board = setup.value.board;
  const side = (color: "white" | "black"): Counts => {
    const own = board[color];
    return {
      q: board.queen.intersect(own).size(),
      r: board.rook.intersect(own).size(),
      b: board.bishop.intersect(own).size(),
      n: board.knight.intersect(own).size(),
      p: board.pawn.intersect(own).size(),
    };
  };
  return { white: side("white"), black: side("black") };
}

/**
 * The family of an ending: no pawns → other; pawns with a queen → queen; pawns with a knight or
 * bishop → other; pawns with rooks → rook and pawn; pawns only → king and pawn.
 */
export function endgameFamily(fen: string): EndgameFamily | null {
  const m = material(fen);
  if (!m) return null;
  const all = (k: keyof Counts) => m.white[k] + m.black[k];
  if (all("p") === 0) return "other";
  if (all("q") > 0) return "queen";
  if (all("b") + all("n") > 0) return "other";
  if (all("r") > 0) return "rook_and_pawn";
  return "king_and_pawn";
}

export const FAMILY_LABELS: Record<EndgameFamily, string> = {
  king_and_pawn: "King and pawn",
  rook_and_pawn: "Rook and pawn",
  queen: "Queen",
  other: "Minor pieces and other",
};

export const FAMILIES: EndgameFamily[] = ["king_and_pawn", "rook_and_pawn", "queen", "other"];

/** The known endings, one side's letters against the other's (either side may be stronger). */
const KNOWN: [string, string][] = [
  ["KP", "K"],
  ["KPP", "K"],
  ["KQ", "K"],
  ["KR", "K"],
  ["KR", "KP"],
  ["KBN", "K"],
  ["KPP", "KR"],
  ["KP", "KB"],
  ["KP", "KN"],
  ["KQP", "KQ"],
  ["KQ", "KP"],
  ["KR", "KB"],
  ["KRP", "KR"],
  ["KRPP", "KR"],
  ["KRB", "KB"],
  ["KRB", "KR"],
  ["KR", "KR"],
  ["KR", "KN"],
  ["KBP", "KB"],
  ["KRN", "KR"],
  ["KPPP", "KR"],
];

function letters(c: Counts): string {
  return "K" + "Q".repeat(c.q) + "R".repeat(c.r) + "B".repeat(c.b) + "N".repeat(c.n) + "P".repeat(c.p);
}

/** The name of a known ending ("KRPvKR"), when the material of the FEN is one. */
export function knownEnding(fen: string): string | null {
  const m = material(fen);
  if (!m) return null;
  const w = letters(m.white);
  const b = letters(m.black);
  const hit = KNOWN.find(([x, y]) => (w === x && b === y) || (w === y && b === x));
  return hit ? `${hit[0]}v${hit[1]}` : null;
}

/** Pieces on the board, kings included. */
export function pieceCount(fen: string): number {
  const setup = parseFen(fen);
  return setup.isErr ? 0 : setup.value.board.occupied.size();
}
