// Spaced repetition (docs/trainer.md §10): a Leitner box per drill, and a mastery level per theme.
import type { TrainGrade } from "../../api/generated/TrainGrade";
import type { TrainTheme } from "../../api/generated/TrainTheme";
import type { ScheduleEntry, ThemeStat } from "./db";

export const DAY_MS = 24 * 60 * 60 * 1000;
/** Days until a drill comes back, by box. */
export const BOX_DAYS = [0, 1, 3, 7, 16, 35] as const;
export const TOP_BOX = BOX_DAYS.length - 1;

export interface Outcome {
  grade: TrainGrade;
  /** Hints used: 0 none, 1 the piece, 2 the idea, 3 the move. */
  hint: 0 | 1 | 2 | 3;
  /** The answer came on a second try. */
  retry: boolean;
}

/** The first try without hints found it. */
export function cleanCorrect(o: Outcome): boolean {
  return o.grade === "correct" && o.hint === 0 && !o.retry;
}

/**
 * The drill's schedule after an attempt: a clean correct answer moves it up a box (from the top
 * box it is mastered and retires); a correct answer with a small hint, or a fine move that was not
 * the idea, keeps its box; anything else, or seeing the move, sends it back to box 0.
 */
export function nextSchedule(itemId: string, current: ScheduleEntry | undefined, o: Outcome, now: number): ScheduleEntry {
  const box = current?.box ?? 0;
  let next: number;
  let retired = false;
  if (cleanCorrect(o)) {
    if (box >= TOP_BOX && current) retired = true;
    next = Math.min(box + 1, TOP_BOX);
  } else if ((o.grade === "correct" && o.hint < 3) || o.grade === "not_incorrect") {
    next = box;
  } else {
    next = 0;
  }
  return { item_id: itemId, box: next, due_at: now + BOX_DAYS[next]! * DAY_MS, retired };
}

/** A drill not yet tried, due at once. */
export function newSchedule(itemId: string, now: number): ScheduleEntry {
  return { item_id: itemId, box: 0, due_at: now, retired: false };
}

export function isDue(entry: ScheduleEntry | undefined, now: number): boolean {
  return !entry || (!entry.retired && entry.due_at <= now);
}

/** Half-life of the theme success rate, in attempts. */
export const HALF_LIFE = 10;
const ALPHA = 1 - Math.pow(0.5, 1 / HALF_LIFE);

/** How much an attempt counts as a success (1 clean, ½ with help or a fine move, 0 wrong). */
export function successOf(o: Outcome): number {
  if (o.grade === "correct") return o.hint === 0 && !o.retry ? 1 : o.hint >= 3 ? 0 : 0.5;
  if (o.grade === "not_incorrect") return 0.5;
  return 0;
}

export function nextThemeStat(theme: TrainTheme, current: ThemeStat | undefined, o: Outcome, now: number): ThemeStat {
  const s = successOf(o);
  const ewma = current ? current.ewma + ALPHA * (s - current.ewma) : s;
  return { theme, ewma, count: (current?.count ?? 0) + 1, last_at: now };
}

export type Mastery = "new" | "shaky" | "learning" | "solid";

export const MASTERY_LABELS: Record<Mastery, string> = {
  new: "New",
  shaky: "Shaky",
  learning: "Learning",
  solid: "Solid",
};

/** Mastery bands: new below 3 attempts, shaky below 60 %, solid above 85 % over 8 or more. */
export function mastery(stat: ThemeStat | undefined): Mastery {
  if (!stat || stat.count < 3) return "new";
  if (stat.ewma < 0.6) return "shaky";
  if (stat.ewma > 0.85 && stat.count >= 8) return "solid";
  return "learning";
}
