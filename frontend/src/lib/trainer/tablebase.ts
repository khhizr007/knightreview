// Exact endgame verdicts from the Lichess tablebase API (docs/trainer.md §8.4): positions with at
// most 7 pieces, only when the player turns it on, cached locally. The position (a FEN) is the
// only thing sent.
import type { TrainerDb } from "./db";
import { pieceCount } from "./endgame";

export const TABLEBASE_URL = "https://tablebase.lichess.ovh/standard";
export const TABLEBASE_SOURCE = "Lichess tablebase (tablebase.lichess.ovh)";
export const MAX_PIECES = 7;
export const SETTING_KEY = "tablebase";

/** The tablebase's verdict for the side to move. */
export type Verdict = "win" | "draw" | "loss";

/** Its categories: win, cursed-win (a win the 50-move rule turns into a draw), draw, blessed-loss, loss… */
export function verdictOf(category: string): Verdict | null {
  if (category === "win" || category === "maybe-win") return "win";
  if (category === "loss" || category === "maybe-loss") return "loss";
  if (category === "draw" || category === "cursed-win" || category === "blessed-loss") return "draw";
  return null;
}

type Fetch = (url: string, init?: RequestInit) => Promise<Response>;

/** The verdict for the side to move of `fen`, or null when it is not in the tablebase or not reached. */
export async function probe(fen: string, db: TrainerDb | null, fetcher: Fetch = fetch, now = Date.now()): Promise<Verdict | null> {
  if (pieceCount(fen) > MAX_PIECES || pieceCount(fen) === 0) return null;
  const key = fen.split(" ").slice(0, 4).join(" ");
  const cached = await db?.get("tablebase", key);
  if (cached) return verdictOf(cached.category);
  try {
    const res = await fetcher(`${TABLEBASE_URL}?fen=${encodeURIComponent(fen)}`);
    if (!res.ok) return null;
    const body = (await res.json()) as { category?: unknown };
    if (typeof body.category !== "string") return null;
    await db?.put("tablebase", [{ fen: key, category: body.category, at: now }]);
    return verdictOf(body.category);
  } catch {
    return null;
  }
}

export async function tablebaseEnabled(db: TrainerDb): Promise<boolean> {
  return (await db.get("meta", SETTING_KEY))?.value === true;
}

export async function setTablebaseEnabled(db: TrainerDb, on: boolean): Promise<void> {
  await db.put("meta", [{ key: SETTING_KEY, value: on }]);
}
