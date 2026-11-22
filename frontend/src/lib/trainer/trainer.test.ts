import { describe, expect, it, vi } from "vitest";
import type { ReviewResult } from "../../api/generated/ReviewResult";
import type { TrainGradeResponse } from "../../api/generated/TrainGradeResponse";
import type { TrainItem } from "../../api/generated/TrainItem";
import { playMove } from "../chess";
import { CARDS } from "./concepts";
import { exportAll, importAll, memoryDb, type StoredItem } from "./db";
import { endgameFamily, knownEnding } from "./endgame";
import { fallbackGrade, gradeDrillMove } from "./grade";
import { startPlayOut, stepPlayOut } from "./playout";
import { BOX_DAYS, DAY_MS, isDue, mastery, nextSchedule, nextThemeStat, TOP_BOX } from "./schedule";
import { buildSession, spreadGames } from "./session";
import { gameKey, gameStats, panelStats } from "./stats";
import { addReview, loadState, recordAttempt, streakOf } from "./store";
import { probe, verdictOf } from "./tablebase";

const NOW = Date.UTC(2026, 10, 22, 12);

function item(over: Partial<TrainItem> = {}): TrainItem {
  const start = "4k3/8/8/8/8/8/8/4K2R w K - 0 1";
  const ref = (uci: string) => ({ san: uci, uci, fen: start });
  return {
    id: "a",
    kind: "miss",
    theme: "fork",
    bucket: "tactics",
    user_side: "white",
    pre_pitch_fen: "4k3/8/8/8/8/8/8/4K2R b K - 0 1",
    pitch_move: ref("e8e7"),
    start_fen: start,
    correct_move: ref("h1h8"),
    played_move: ref("e1e2"),
    best_line: ["h1h8"],
    played_line: ["e1e2", "e8d7"],
    lines: [{ eval: { kind: "cp", cp: 500 }, depth: 12, pv: ["h1h8"] }],
    classification: "miss",
    miss_kind: "tactic",
    phase: "endgame",
    is_critical: false,
    position_type: null,
    eco: null,
    opening_name: null,
    endgame_family: null,
    endgame_concept: null,
    endgame_type: null,
    play_out: null,
    ep_lost: 0.3,
    context: { rating: 1500, highest_elo: 1500, previous_ep_lost: 0, previous_erred: false },
    goal: "Find it.",
    hint: "Look.",
    source: {
      ply: 10,
      move_number: 5,
      white: "me",
      black: "you",
      opponent: "you",
      date: null,
      site: null,
      event: null,
      result: null,
      time_control: null,
    },
    generator_version: 1,
    ...over,
  };
}

function stored(over: Partial<StoredItem> = {}): StoredItem {
  return { ...item(over), added_at: over.added_at ?? NOW, game_key: over.game_key ?? "g1" };
}

describe("schedule", () => {
  it("moves a clean answer up a box and retires it from the top", () => {
    let s = nextSchedule("a", undefined, { grade: "correct", hint: 0, retry: false }, NOW);
    expect(s.box).toBe(1);
    expect(s.due_at).toBe(NOW + BOX_DAYS[1] * DAY_MS);
    for (let i = 0; i < TOP_BOX; i++) s = nextSchedule("a", s, { grade: "correct", hint: 0, retry: false }, NOW);
    expect(s.box).toBe(TOP_BOX);
    expect(s.retired).toBe(true);
    expect(isDue(s, NOW + 1000 * DAY_MS)).toBe(false);
  });

  it("keeps the box with help and resets it on a wrong answer", () => {
    const s = { item_id: "a", box: 3, due_at: NOW, retired: false };
    expect(nextSchedule("a", s, { grade: "correct", hint: 1, retry: false }, NOW).box).toBe(3);
    expect(nextSchedule("a", s, { grade: "not_incorrect", hint: 0, retry: false }, NOW).box).toBe(3);
    expect(nextSchedule("a", s, { grade: "correct", hint: 3, retry: false }, NOW).box).toBe(0);
    expect(nextSchedule("a", s, { grade: "incorrect", hint: 0, retry: false }, NOW).box).toBe(0);
    expect(nextSchedule("a", s, { grade: "same_as_game", hint: 0, retry: false }, NOW).box).toBe(0);
  });

  it("bands theme mastery", () => {
    expect(mastery(undefined)).toBe("new");
    let t = undefined as ReturnType<typeof nextThemeStat> | undefined;
    for (let i = 0; i < 10; i++) t = nextThemeStat("fork", t, { grade: "correct", hint: 0, retry: false }, NOW);
    expect(mastery(t)).toBe("solid");
    for (let i = 0; i < 10; i++) t = nextThemeStat("fork", t, { grade: "incorrect", hint: 0, retry: false }, NOW);
    expect(mastery(t)).toBe("shaky");
  });
});

describe("session", () => {
  it("never puts two drills of one game together when it can help it", () => {
    const list = [stored({ id: "1", game_key: "a" }), stored({ id: "2", game_key: "a" }), stored({ id: "3", game_key: "b" })];
    const out = spreadGames(list).map((i) => i.game_key);
    expect(out).toEqual(["a", "b", "a"]);
  });

  it("takes due drills of the weakest family, then new ones, up to the size", () => {
    const items = [
      ...Array.from({ length: 6 }, (_, i) => stored({ id: `e${i}`, kind: "endgame", theme: "endgame_rook_and_pawn", game_key: `g${i}` })),
      ...Array.from({ length: 6 }, (_, i) => stored({ id: `m${i}`, kind: "miss", theme: "fork", game_key: `h${i}` })),
      ...Array.from({ length: 6 }, (_, i) => stored({ id: `n${i}`, kind: "opening", theme: "develop", game_key: `k${i}`, added_at: NOW + i })),
    ];
    const schedule = new Map(
      items.filter((i) => i.kind !== "opening").map((i) => [i.id, { item_id: i.id, box: 1, due_at: NOW - 1, retired: false }]),
    );
    const s = buildSession({ items, schedule, themes: new Map(), now: NOW });
    expect(s).toHaveLength(10);
    expect(new Set(s.map((i) => i.id)).size).toBe(10);
    // Endgames count as weakest while not solid: they come in first.
    expect(s.filter((i) => i.kind === "endgame").length).toBeGreaterThanOrEqual(4);
    expect(s.some((i) => i.kind === "opening")).toBe(true);
  });

  it("filters by family, theme and game", () => {
    const items = [stored({ id: "1", kind: "endgame" }), stored({ id: "2", theme: "pin" }), stored({ id: "3", game_key: "x" })];
    const base = { items, schedule: new Map(), themes: new Map(), now: NOW };
    expect(buildSession({ ...base, filter: { kind: "family", family: "endgame" } }).map((i) => i.id)).toEqual(["1"]);
    expect(buildSession({ ...base, filter: { kind: "theme", theme: "pin" } }).map((i) => i.id)).toEqual(["2"]);
    expect(buildSession({ ...base, filter: { kind: "game", gameKey: "x" } }).map((i) => i.id)).toEqual(["3"]);
  });
});

describe("store", () => {
  it("round-trips an export and merges attempts on import", async () => {
    const db = memoryDb();
    await db.put("items", [stored()]);
    await recordAttempt(db, item(), { grade: "correct", hint: 0, retry: false, ms: 900 }, NOW);
    const file = await exportAll(db, NOW);
    const other = memoryDb();
    await other.put("attempts", [{ item_id: "z", at: NOW, grade: "incorrect", hint: 0, retry: false, ms: 1 }]);
    expect(await importAll(other, JSON.parse(JSON.stringify(file)))).toBeGreaterThan(0);
    const state = await loadState(other, NOW);
    expect(state.items.map((i) => i.id)).toEqual(["a"]);
    expect(state.attempts).toHaveLength(2);
    expect(state.schedule.get("a")?.box).toBe(1);
    await expect(importAll(other, { nope: true })).rejects.toThrow();
  });

  it("counts a streak of days with five drills", () => {
    const day = (d: number, n: number) =>
      Array.from({ length: n }, () => ({ item_id: "a", at: NOW - d * DAY_MS, grade: "correct" as const, hint: 0 as const, retry: false, ms: 1 }));
    expect(streakOf([...day(0, 5), ...day(1, 5), ...day(2, 4)], NOW).days).toBe(2);
    expect(streakOf([...day(1, 5)], NOW)).toEqual({ days: 1, today: 0 });
  });

  it("keeps a review's drills for the player's side only", async () => {
    const db = memoryDb();
    const review = {
      game: { site: "https://lichess.org/abc", white: "me", black: "you", date: null, result: "1-0", opening: null, chess960: false },
      settings: { player_side: "white" },
      white: { accuracy: { overall: 80, opening: 90, middlegame: 75, endgame: 70, moves_counted: 20 } },
      black: { accuracy: { overall: 60, opening: 70, middlegame: 55, endgame: 50, moves_counted: 20 } },
      plies: [],
      train_items: [item({ id: "w" }), item({ id: "b", user_side: "black" })],
    } as unknown as ReviewResult;
    expect(await addReview(db, review, null, NOW)).toBe(1);
    expect(await addReview(db, review, null, NOW)).toBe(0);
    const state = await loadState(db, NOW);
    expect(state.items.map((i) => i.id)).toEqual(["w"]);
    expect(state.games[0]?.game_key).toBe(gameKey(review));
    expect(state.games[0]?.score).toBe(1);
  });
});

describe("stats", () => {
  it("buckets endgame accuracy by family and the entry by score", () => {
    const ply = (fen: string, mover: "white" | "black", acc: number) => ({
      ply: 1,
      mover,
      uci: "e1e2",
      phase: "endgame",
      fen_after: fen,
      eval_before: { kind: "cp", cp: 250 },
      ep_before: 0.6,
      ep_lost: 0,
      accuracy: acc,
      is_book: false,
      coach: { events: [] },
    });
    const review = {
      game: { site: null, white: "me", black: "you", date: null, result: "1/2-1/2", opening: null },
      white: { accuracy: { overall: 80, opening: null, middlegame: null, endgame: 70, moves_counted: 2 } },
      black: { accuracy: { overall: 80, opening: null, middlegame: null, endgame: 70, moves_counted: 2 } },
      plies: [ply("8/5k2/8/3p4/8/8/5K2/R7 b - - 0 1", "white", 90), ply("8/5k2/8/3p4/3P4/8/5K2/8 w - - 0 1", "white", 60)],
    } as unknown as ReviewResult;
    const g = gameStats(review, "white");
    expect(g.endgame.entry).toBe("better");
    expect(g.endgame.families.rook_and_pawn).toEqual({ sum: 90, n: 1 });
    expect(g.endgame.families.king_and_pawn).toEqual({ sum: 60, n: 1 });
    expect(g.endgame.types).toContain("KRvKP");
    const p = panelStats([g]);
    expect(p.entries.better).toEqual({ games: 1, score: 0.5 });
    expect(p.families.rook_and_pawn.accuracy).toBe(90);
  });

  it("names families and known endings as the server does", () => {
    expect(endgameFamily("8/5k2/8/3p4/3P4/8/5K2/8 w - - 0 1")).toBe("king_and_pawn");
    expect(endgameFamily("8/5k2/8/3p4/3P4/8/5K2/Q7 w - - 0 1")).toBe("queen");
    expect(endgameFamily("8/5k2/8/3p4/3P4/8/5K2/B7 w - - 0 1")).toBe("other");
    expect(endgameFamily("8/5k2/8/8/8/8/5K2/R7 w - - 0 1")).toBe("other");
    expect(knownEnding("r7/5k2/8/3P4/8/8/5K2/R7 w - - 0 1")).toBe("KRPvKR");
    expect(knownEnding("r7/5k2/8/8/8/8/5K2/RN6 w - - 0 1")).toBe("KRNvKR");
    expect(knownEnding("8/5k2/8/3p4/3P4/8/5K2/8 w - - 0 1")).toBeNull();
  });
});

describe("grading in the browser", () => {
  const answer = (over: Partial<TrainGradeResponse>): TrainGradeResponse => ({ ...fallbackGrade(item(), "h1h8"), fallback: false, ...over });

  it("searches after a move the stored lines do not cover, then asks the server", async () => {
    const engine = { bestLine: vi.fn().mockResolvedValue({ eval: { kind: "cp", cp: 0 }, depth: 14, pv: ["e8d7"] }) };
    const grade = vi.fn().mockResolvedValue(answer({ grade: "not_incorrect" }));
    const r = await gradeDrillMove(item(), "h1h7", { engine, grade });
    expect(r.grade).toBe("not_incorrect");
    expect(engine.bestLine).toHaveBeenCalledOnce();
    expect(grade.mock.calls[0]![0].after.pv).toEqual(["e8d7"]);
    // The best move is in the stored lines: no search.
    await gradeDrillMove(item(), "h1h8", { engine, grade });
    expect(engine.bestLine).toHaveBeenCalledOnce();
  });

  it("falls back to the exact best move when the server fails", async () => {
    const grade = vi.fn().mockRejectedValue(new Error("down"));
    expect((await gradeDrillMove(item(), "h1h8", { engine: null, grade })).grade).toBe("correct");
    const wrong = await gradeDrillMove(item(), "h1h7", { engine: null, grade });
    expect(wrong.grade).toBe("incorrect");
    expect(wrong.fallback).toBe(true);
    expect((await gradeDrillMove(item(), "e1e2", { engine: null, grade })).grade).toBe("same_as_game");
  });
});

describe("play-out", () => {
  const step = (over: Partial<Parameters<typeof stepPlayOut>[1]>) => ({
    eval: null,
    ending: null,
    toMove: "black" as const,
    promoted: false,
    verdict: null,
    ...over,
  });

  it("wins by mate and loses when the advantage slips", () => {
    const p = startPlayOut("win", "white", 6);
    expect(p.limit).toBe(22);
    expect(stepPlayOut(p, step({ ending: "checkmate" })).state).toBe("won");
    expect(stepPlayOut(p, step({ eval: { kind: "cp", cp: 20 } })).state).toBe("lost");
    expect(stepPlayOut(p, step({ eval: { kind: "cp", cp: 400 } })).state).toBe("playing");
  });

  it("converts after a promotion held for four moves", () => {
    let p = startPlayOut("win", "white", 6);
    p = stepPlayOut(p, step({ promoted: true, eval: { kind: "cp", cp: 900 } }));
    for (let i = 0; i < 3; i++) {
      p = stepPlayOut(p, step({ toMove: "white", eval: { kind: "cp", cp: 900 } }));
      p = stepPlayOut(p, step({ eval: { kind: "cp", cp: 900 } }));
    }
    expect(p.state).toBe("won");
  });

  it("holds a draw to the move limit or by a drawn ending", () => {
    let p = startPlayOut("draw", "black", 1);
    expect(stepPlayOut(p, step({ ending: "stalemate", toMove: "white" })).state).toBe("won");
    expect(stepPlayOut(p, step({ eval: { kind: "cp", cp: 300 }, toMove: "white" })).state).toBe("lost");
    for (let i = 0; i < p.limit; i++) p = stepPlayOut(p, step({ eval: { kind: "cp", cp: 0 } }));
    expect(p.state).toBe("won");
  });

  it("follows the tablebase verdict", () => {
    const p = startPlayOut("win", "white", 6);
    // Black to move and drawing: the win is gone.
    expect(stepPlayOut(p, step({ verdict: "draw", eval: { kind: "cp", cp: 600 } })).state).toBe("lost");
  });
});

describe("tablebase", () => {
  it("reads categories and caches answers", async () => {
    expect(verdictOf("cursed-win")).toBe("draw");
    expect(verdictOf("win")).toBe("win");
    const db = memoryDb();
    const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ category: "win" })));
    const fen = "8/8/8/3k4/8/8/3PK3/8 w - - 0 1";
    expect(await probe(fen, db, fetcher)).toBe("win");
    expect(await probe(fen, db, fetcher)).toBe("win");
    expect(fetcher).toHaveBeenCalledOnce();
    // Too many pieces: not asked.
    expect(await probe("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", db, fetcher)).toBeNull();
    expect(fetcher).toHaveBeenCalledOnce();
  });
});

describe("concept cards", () => {
  it("only lists legal answers", () => {
    for (const card of CARDS) {
      for (const p of card.positions) {
        expect(p.solutions.length).toBeGreaterThan(0);
        for (const uci of p.solutions) expect(playMove(p.fen, uci.slice(0, 2), uci.slice(2, 4)), `${card.id} ${uci}`).not.toBeNull();
      }
    }
  });
});
