// Grading a drill move (docs/trainer.md §6.4): the browser engine searches the position after the
// move when the drill's stored lines do not already cover it, then the server grades it. Without
// the engine or the server, only the drill's best move counts.
import type { TrainGradeRequest } from "../../api/generated/TrainGradeRequest";
import type { TrainGradeResponse } from "../../api/generated/TrainGradeResponse";
import type { TrainItem } from "../../api/generated/TrainItem";
import type { TrainLine } from "../../api/generated/TrainLine";
import { ending, lineMoves } from "../chess";
import type { Analysis, EngineController } from "../engine/controller";

/** Depth of the search after the player's move. */
export const AFTER_DEPTH = 14;
/** Longest wait for the engine and the server (the reference gives up after 5 s too). */
export const GRADE_TIMEOUT_MS = 5000;

/** The engine as grading needs it: the best line of a position. */
export interface DrillEngine {
  bestLine(fen: string, depth: number, timeoutMs: number): Promise<TrainLine | null>;
}

/** The browser engine (one search at a time), giving its deepest line when time runs out. */
export function controllerEngine(controller: EngineController): DrillEngine {
  return {
    bestLine(fen, depth, timeoutMs) {
      return new Promise((resolve) => {
        let latest: Analysis | null = null;
        const done = (a: Analysis | null) => {
          clearTimeout(timer);
          const line = a?.lines[0];
          resolve(line ? { eval: line.eval, depth: line.depth, pv: line.pv } : null);
        };
        const timer = setTimeout(() => {
          controller.stop();
          done(latest);
        }, timeoutMs);
        controller.analyze(
          fen,
          depth,
          (a) => {
            if (a.fen !== fen) return;
            latest = a;
            if (a.done) done(a);
          },
          1,
        );
      });
    },
  };
}

export interface GradeDeps {
  engine: DrillEngine | null;
  grade: (req: TrainGradeRequest, signal?: AbortSignal) => Promise<TrainGradeResponse>;
  timeoutMs?: number;
}

/** The grade when neither the engine nor the server can help: only the best move is correct. */
export function fallbackGrade(item: TrainItem, userMove: string): TrainGradeResponse {
  const move = lineMoves(item.start_fen, [userMove], 1)[0];
  const same = userMove === item.played_move.uci;
  const correct = userMove === item.correct_move.uci;
  return {
    grade: same ? "same_as_game" : correct ? "correct" : "incorrect",
    user_move: move ? { san: move.san, uci: move.uci, fen: move.fen } : { san: userMove, uci: userMove, fen: item.start_fen },
    classification: null,
    miss_kind: null,
    ep_lost: null,
    on_theme: false,
    fallback: true,
    reply: [
      same ? "That's the move you played in the game." : correct ? "That's it." : "Not this time.",
      "The engine was not available, so only the exact best move counts.",
    ],
    coach: { sentences: [], show_lines: [], events: [] },
    show_line: same ? item.played_line : correct ? item.best_line : [userMove],
  };
}

/** Grades the player's move in a drill. Never throws: failures fall back to `fallbackGrade`. */
export async function gradeDrillMove(item: TrainItem, userMove: string, deps: GradeDeps): Promise<TrainGradeResponse> {
  const timeoutMs = deps.timeoutMs ?? GRADE_TIMEOUT_MS;
  const inLines = item.lines.some((l) => l.pv[0] === userMove);
  const move = lineMoves(item.start_fen, [userMove], 1)[0];
  if (!move) return fallbackGrade(item, userMove);
  let after: TrainLine | undefined;
  if (!inLines && !ending(move.fen) && userMove !== item.played_move.uci) {
    const line = deps.engine ? await deps.engine.bestLine(move.fen, AFTER_DEPTH, timeoutMs).catch(() => null) : null;
    if (line) after = line;
  }
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await deps.grade({ item, user_move: userMove, ...(after ? { after } : {}) }, controller.signal);
  } catch {
    return fallbackGrade(item, userMove);
  } finally {
    clearTimeout(timer);
  }
}
