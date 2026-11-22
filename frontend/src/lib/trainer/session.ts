// Building a training session (docs/trainer.md §10.3).
import type { ThemeFamily } from "../../api/generated/ThemeFamily";
import type { TrainKind } from "../../api/generated/TrainKind";
import type { ScheduleEntry, StoredItem, ThemeStat } from "./db";
import { isDue, mastery } from "./schedule";

export const SESSION_SIZE = 10;
/** Endgames weigh this much more while their mastery is below solid (the trainer's first focus). */
export const ENDGAME_WEIGHT = 1.5;

export function familyOfKind(kind: TrainKind): ThemeFamily {
  if (kind === "opening") return "opening";
  if (kind === "strategy") return "strategy";
  if (kind === "endgame") return "endgame";
  return "tactics";
}

export type SessionFilter =
  | { kind: "today" }
  | { kind: "family"; family: ThemeFamily }
  | { kind: "theme"; theme: string }
  | { kind: "game"; gameKey: string };

export interface SessionInput {
  items: StoredItem[];
  schedule: Map<string, ScheduleEntry>;
  themes: Map<string, ThemeStat>;
  now: number;
  size?: number;
  filter?: SessionFilter;
}

/** Success of a family: the mean theme success over its drills' themes (unknown counts as 0.5). */
function familyScore(family: ThemeFamily, items: StoredItem[], themes: Map<string, ThemeStat>): number {
  const ts = [...new Set(items.filter((i) => familyOfKind(i.kind) === family).map((i) => i.theme))];
  if (ts.length === 0) return 1;
  const score = ts.reduce((s, t) => s + (themes.get(t)?.ewma ?? 0.5), 0) / ts.length;
  const solid = ts.every((t) => mastery(themes.get(t)) === "solid");
  // A lower score is weaker; the endgame counts as weaker while it is not solid.
  return family === "endgame" && !solid ? score / ENDGAME_WEIGHT : score;
}

/** Puts two drills of the same game apart where it can. */
export function spreadGames(list: StoredItem[]): StoredItem[] {
  const out: StoredItem[] = [];
  const rest = [...list];
  while (rest.length) {
    const last = out.at(-1)?.game_key;
    const i = rest.findIndex((it) => it.game_key !== last);
    out.push(...rest.splice(i >= 0 ? i : 0, 1));
  }
  return out;
}

/**
 * A session: due drills of the weakest family first (4), then due drills of the others (3), then
 * new drills from the most recent games (2), then one mastered drill to check it holds. Short
 * parts are filled with new drills, then any other due ones. Filters narrow the pool first.
 */
export function buildSession(input: SessionInput): StoredItem[] {
  const { schedule, themes, now } = input;
  const size = input.size ?? SESSION_SIZE;
  const f = input.filter ?? { kind: "today" };
  const pool = input.items.filter((it) => {
    if (f.kind === "family") return familyOfKind(it.kind) === f.family;
    if (f.kind === "theme") return it.theme === f.theme;
    if (f.kind === "game") return it.game_key === f.gameKey;
    return true;
  });
  if (f.kind === "game") return spreadGames([...pool].sort((a, b) => a.source.ply - b.source.ply)).slice(0, size);

  const tried = (it: StoredItem) => schedule.has(it.id);
  const due = pool
    .filter((it) => tried(it) && isDue(schedule.get(it.id), now))
    .sort((a, b) => (schedule.get(a.id)!.due_at - schedule.get(b.id)!.due_at) || b.ep_lost - a.ep_lost);
  const fresh = pool.filter((it) => !tried(it)).sort((a, b) => b.added_at - a.added_at || b.ep_lost - a.ep_lost);
  const mastered = pool.filter((it) => schedule.get(it.id)?.retired).sort((a, b) => schedule.get(a.id)!.due_at - schedule.get(b.id)!.due_at);

  const families: ThemeFamily[] = ["endgame", "opening", "strategy", "tactics"];
  const weakest = [...families].sort((a, b) => familyScore(a, pool, themes) - familyScore(b, pool, themes))[0]!;

  const chosen: StoredItem[] = [];
  const take = (from: StoredItem[], n: number) => {
    for (const it of from) {
      if (n <= 0 || chosen.length >= size) break;
      if (chosen.includes(it)) continue;
      chosen.push(it);
      n--;
    }
  };
  const scale = (n: number) => Math.max(1, Math.round((n * size) / SESSION_SIZE));
  take(due.filter((it) => familyOfKind(it.kind) === weakest), scale(4));
  take(due.filter((it) => familyOfKind(it.kind) !== weakest), scale(3));
  take(fresh, scale(2));
  take(mastered, 1);
  take(fresh, size);
  take(due, size);
  return spreadGames(chosen);
}

/** Drills due now (not counting new ones). */
export function dueCount(items: StoredItem[], schedule: Map<string, ScheduleEntry>, now: number): number {
  return items.filter((it) => schedule.has(it.id) && isDue(schedule.get(it.id), now)).length;
}
