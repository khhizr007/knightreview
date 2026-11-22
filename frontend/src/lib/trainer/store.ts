// The trainer's operations on its local store: adding a reviewed game, recording an attempt, and
// reading everything the screens show.
import { api } from "../../api/client";
import type { ReviewResult } from "../../api/generated/ReviewResult";
import type { Side } from "../../api/generated/Side";
import type { TrainItem } from "../../api/generated/TrainItem";
import type { Attempt, ScheduleEntry, StoredGame, StoredItem, ThemeStat, TrainerDb } from "./db";
import { type Outcome, nextSchedule, nextThemeStat } from "./schedule";
import { gameKey, gameStats } from "./stats";

export interface TrainerState {
  items: StoredItem[];
  games: StoredGame[];
  schedule: Map<string, ScheduleEntry>;
  themes: Map<string, ThemeStat>;
  attempts: Attempt[];
  streak: Streak;
}

export interface Streak {
  /** Consecutive days with at least `STREAK_DRILLS` drills, ending today or yesterday. */
  days: number;
  /** Drills done today. */
  today: number;
}

export const STREAK_DRILLS = 5;

function dayOf(t: number): string {
  const d = new Date(t);
  return `${d.getFullYear()}-${d.getMonth() + 1}-${d.getDate()}`;
}

export function streakOf(attempts: Attempt[], now: number): Streak {
  const perDay = new Map<string, number>();
  for (const a of attempts) if (!a.retry) perDay.set(dayOf(a.at), (perDay.get(dayOf(a.at)) ?? 0) + 1);
  const today = perDay.get(dayOf(now)) ?? 0;
  let days = 0;
  // Today counts once it reaches the bar; until then the streak runs to yesterday.
  let t = today >= STREAK_DRILLS ? now : now - 86_400_000;
  while ((perDay.get(dayOf(t)) ?? 0) >= STREAK_DRILLS) {
    days++;
    t -= 86_400_000;
  }
  return { days, today };
}

export async function loadState(db: TrainerDb, now = Date.now()): Promise<TrainerState> {
  const [items, games, schedule, themes, attempts] = await Promise.all([
    db.all("items"),
    db.all("games"),
    db.all("schedule"),
    db.all("themes"),
    db.all("attempts"),
  ]);
  return {
    items,
    games,
    schedule: new Map(schedule.map((s) => [s.item_id, s])),
    themes: new Map(themes.map((t) => [t.theme, t])),
    attempts,
    streak: streakOf(attempts, now),
  };
}

/** The side the drills of a review are for: the review's player side, else the one given. */
export function reviewSide(review: ReviewResult, fallback: Side | null): Side | null {
  return review.settings.player_side ?? fallback;
}

/**
 * Stores a finished review's drills and game stats. Without a known player side nothing is
 * stored (the drills would mix both players). A drill already stored keeps its schedule and
 * attempts; its content is replaced when it comes from a newer generator.
 */
export async function addReview(db: TrainerDb, review: ReviewResult, side: Side | null, now = Date.now()): Promise<number> {
  const player = reviewSide(review, side);
  if (!player || review.game.chess960) return 0;
  const key = gameKey(review);
  const items = review.train_items.filter((it) => it.user_side === player);
  const existing = new Map((await db.all("items")).map((i) => [i.id, i]));
  const rows: StoredItem[] = items.map((it) => ({
    ...it,
    added_at: existing.get(it.id)?.added_at ?? now,
    game_key: key,
  }));
  await db.put("items", rows);
  const known = await db.get("games", key);
  await db.put("games", [{ ...gameStats(review, player), game_key: key, added_at: known?.added_at ?? now }]);
  return rows.filter((r) => !existing.has(r.id)).length;
}

/** Records an attempt: its history, the drill's next review and its theme's progress. */
export async function recordAttempt(
  db: TrainerDb,
  item: TrainItem,
  outcome: Outcome & { ms: number },
  now = Date.now(),
): Promise<ScheduleEntry> {
  await db.put("attempts", [
    { item_id: item.id, at: now, grade: outcome.grade, hint: outcome.hint, retry: outcome.retry, ms: outcome.ms },
  ]);
  const schedule = nextSchedule(item.id, await db.get("schedule", item.id), outcome, now);
  await db.put("schedule", [schedule]);
  await db.put("themes", [nextThemeStat(item.theme, await db.get("themes", item.theme), outcome, now)]);
  return schedule;
}

/** Asks the server to remake drills stored from an older generator (needs the review itself). */
export async function regenerate(review: ReviewResult): Promise<TrainItem[]> {
  return (await api.generateDrills({ review })).items;
}
