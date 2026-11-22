// What the trainer keeps of each reviewed game, and the numbers of the weakness map and the
// endgame panel (docs/trainer.md §8.5, §14). All of it is computed in the browser.
import type { EndgameFamily } from "../../api/generated/EndgameFamily";
import type { Eval } from "../../api/generated/Eval";
import type { ReviewResult } from "../../api/generated/ReviewResult";
import type { Side } from "../../api/generated/Side";
import type { Tag } from "../../api/generated/Tag";
import { endgameFamily, FAMILIES, knownEnding } from "./endgame";

/** Advantage (cp) that makes the endgame entry better or worse rather than equal. */
export const ENTRY_MARGIN_CP = 200;
/** Positions this lopsided for the player (expected points) are left out of endgame accuracy. */
const DECIDED_EP = 0.1;
/** Moves after leaving the book whose losses are added up. */
const AFTER_BOOK_MOVES = 5;

export type Entry = "better" | "equal" | "worse";

/** Opening habits counted per game (the played move's own coach tags). */
export const HABIT_TAGS: Partial<Record<Tag, string>> = {
  too_early_queen: "Queen out early",
  moves_same_piece: "Same piece twice",
  weakens_king: "Early f-pawn move",
  loses_castling: "Lost castling rights",
};

export interface Sum {
  sum: number;
  n: number;
}

export interface GameStats {
  /** The player's side. */
  side: Side;
  white: string | null;
  black: string | null;
  opponent: string | null;
  date: string | null;
  site: string | null;
  result: string | null;
  /** 1 won, ½ drawn, 0 lost; null when unfinished. */
  score: number | null;
  accuracy: { overall: number | null; opening: number | null; middlegame: number | null; endgame: number | null };
  opening: { eco: string; name: string } | null;
  /** The first ply played out of book by the player, and the expected points they lost over the next moves. */
  book_exit: { ply: number; ep_lost: number } | null;
  habits: Record<string, number>;
  endgame: {
    reached: boolean;
    entry: Entry | null;
    /** Endgame move accuracy by family (the player's moves, decided positions left out). */
    families: Record<EndgameFamily, Sum>;
    /** Known endings reached (each once per game). */
    types: string[];
  };
}

function mine(e: Eval, side: Side): number {
  if (e.kind === "cp") return side === "white" ? e.cp : -e.cp;
  const sign = e.winner === side ? 1 : -1;
  return sign * 100_000;
}

function scoreOf(result: string | null, side: Side): number | null {
  if (result === "1-0") return side === "white" ? 1 : 0;
  if (result === "0-1") return side === "black" ? 1 : 0;
  if (result === "1/2-1/2" || result === "½-½") return 0.5;
  return null;
}

/** A key for a game: its URL when it has one, else its players, date and moves. */
export function gameKey(review: ReviewResult): string {
  const url = review.game.site && /^https?:\/\//.test(review.game.site) ? review.game.site : null;
  if (url) return url;
  const moves = review.plies.map((p) => p.uci).join(" ");
  let h = 2166136261;
  for (let i = 0; i < moves.length; i++) h = Math.imul(h ^ moves.charCodeAt(i), 16777619) >>> 0;
  return [review.game.white ?? "?", review.game.black ?? "?", review.game.date ?? "?", h.toString(16)].join("|");
}

/** The trainer's record of a reviewed game, for the player's side. */
export function gameStats(review: ReviewResult, side: Side): GameStats {
  const acc = side === "white" ? review.white.accuracy : review.black.accuracy;
  const own = review.plies.filter((p) => p.mover === side);

  const exit = own.find((p) => !p.is_book);
  const bookExit =
    exit && review.plies.some((p) => p.is_book)
      ? {
          ply: exit.ply,
          ep_lost: own
            .filter((p) => p.ply >= exit.ply)
            .slice(0, AFTER_BOOK_MOVES)
            .reduce((s, p) => s + p.ep_lost, 0),
        }
      : null;

  const habits: Record<string, number> = {};
  for (const p of own) {
    if (p.phase !== "opening") continue;
    for (const e of p.coach.events) {
      const name = HABIT_TAGS[e.tag];
      if (name && e.chain[0] === "base" && e.chain.length === 1) habits[e.tag] = (habits[e.tag] ?? 0) + 1;
    }
  }

  const families = Object.fromEntries(FAMILIES.map((f) => [f, { sum: 0, n: 0 }])) as Record<EndgameFamily, Sum>;
  const types = new Set<string>();
  let entry: Entry | null = null;
  let reached = false;
  for (const p of review.plies) {
    if (p.phase !== "endgame") continue;
    if (!reached) {
      reached = true;
      const s = mine(p.eval_before, side);
      entry = Math.abs(s) >= ENTRY_MARGIN_CP ? (s > 0 ? "better" : "worse") : "equal";
    }
    const known = knownEnding(p.fen_after);
    if (known) types.add(known);
    if (p.mover !== side || p.accuracy === null) continue;
    if (p.ep_before < DECIDED_EP || p.ep_before > 1 - DECIDED_EP) continue;
    const family = endgameFamily(p.fen_after);
    if (!family) continue;
    families[family].sum += Math.max(0, Math.min(100, p.accuracy));
    families[family].n += 1;
  }

  return {
    side,
    white: review.game.white,
    black: review.game.black,
    opponent: side === "white" ? review.game.black : review.game.white,
    date: review.game.date,
    site: review.game.site,
    result: review.game.result,
    score: scoreOf(review.game.result, side),
    accuracy: { overall: acc.overall, opening: acc.opening, middlegame: acc.middlegame, endgame: acc.endgame },
    opening: review.game.opening ? { eco: review.game.opening.eco, name: review.game.opening.name } : null,
    book_exit: bookExit,
    habits,
    endgame: { reached, entry, families, types: [...types] },
  };
}

export interface PanelStats {
  games: number;
  /** Mean accuracy by phase over the games that have one. */
  phases: Record<"opening" | "middlegame" | "endgame", number | null>;
  overall: number | null;
  /** Endgame accuracy by family, and how many moves it rests on. */
  families: Record<EndgameFamily, { accuracy: number | null; moves: number }>;
  /** Result when entering the endgame better, equal or worse: games and mean score (0–1). */
  entries: Record<Entry, { games: number; score: number | null }>;
  /** Known endings reached, with the games and the mean score in them. */
  types: { type: string; games: number; score: number | null }[];
  habits: { tag: string; name: string; count: number }[];
}

function mean(xs: number[]): number | null {
  return xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : null;
}

export function panelStats(games: GameStats[]): PanelStats {
  const phase = (k: "opening" | "middlegame" | "endgame") =>
    mean(games.map((g) => g.accuracy[k]).filter((x): x is number => x !== null));
  const families = Object.fromEntries(
    FAMILIES.map((f) => {
      const sum = games.reduce((s, g) => s + g.endgame.families[f].sum, 0);
      const n = games.reduce((s, g) => s + g.endgame.families[f].n, 0);
      return [f, { accuracy: n ? sum / n : null, moves: n }];
    }),
  ) as PanelStats["families"];
  const entries = Object.fromEntries(
    (["better", "equal", "worse"] as Entry[]).map((e) => {
      const these = games.filter((g) => g.endgame.entry === e);
      return [e, { games: these.length, score: mean(these.map((g) => g.score).filter((s): s is number => s !== null)) }];
    }),
  ) as PanelStats["entries"];
  const byType = new Map<string, number[]>();
  for (const g of games) for (const t of g.endgame.types) byType.set(t, [...(byType.get(t) ?? []), g.score ?? NaN]);
  const types = [...byType.entries()]
    .map(([type, scores]) => ({ type, games: scores.length, score: mean(scores.filter((s) => !Number.isNaN(s))) }))
    .sort((a, b) => b.games - a.games || a.type.localeCompare(b.type));
  const habitCounts = new Map<string, number>();
  for (const g of games) for (const [tag, n] of Object.entries(g.habits)) habitCounts.set(tag, (habitCounts.get(tag) ?? 0) + n);
  const habits = [...habitCounts.entries()]
    .map(([tag, count]) => ({ tag, name: HABIT_TAGS[tag as Tag] ?? tag, count }))
    .sort((a, b) => b.count - a.count);
  return {
    games: games.length,
    phases: { opening: phase("opening"), middlegame: phase("middlegame"), endgame: phase("endgame") },
    overall: mean(games.map((g) => g.accuracy.overall).filter((x): x is number => x !== null)),
    families,
    entries,
    types,
    habits,
  };
}
