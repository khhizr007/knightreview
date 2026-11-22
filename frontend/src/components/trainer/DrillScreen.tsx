import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { api } from "../../api/client";
import type { Side } from "../../api/generated/Side";
import type { TrainGradeResponse } from "../../api/generated/TrainGradeResponse";
import type { ThemeInfo } from "../../api/generated/ThemeInfo";
import { canPlay, ending, legalTargets, lineMoves, needsPromotion, playMove, type PlayedMove, type PromotionPiece } from "../../lib/chess";
import { EngineController, browserWorker, type Analysis } from "../../lib/engine/controller";
import { moveSound, playSounds } from "../../lib/sound";
import type { StoredItem, TrainerDb } from "../../lib/trainer/db";
import { controllerEngine, gradeDrillMove } from "../../lib/trainer/grade";
import { startPlayOut, stepPlayOut, type PlayOut } from "../../lib/trainer/playout";
import type { Outcome } from "../../lib/trainer/schedule";
import { recordAttempt } from "../../lib/trainer/store";
import { probe, tablebaseEnabled, TABLEBASE_SOURCE } from "../../lib/trainer/tablebase";
import { KIND_LABELS, themeLabel } from "../../lib/trainer/themes";
import { BoardView, type Arrow, type BoardPromotion } from "../BoardView";
import { CoachBubble } from "../Coach";
import type { CoachMood } from "../../lib/coach";

/** A finished drill, for the session summary. */
export interface DrillResult {
  item: StoredItem;
  grade: TrainGradeResponse["grade"];
  hint: Outcome["hint"];
  retried: boolean;
  nextDue: number;
  playOut: PlayOut["state"] | null;
}

const PITCH_DELAY_MS = 500;
const LINE_STEP_MS = 700;
const ENGINE_REPLY_DEPTH = 16;
const HINT_SQUARE: CSSProperties = { background: "rgba(77, 195, 234, 0.45)" };
const ARROW_BEST = "rgba(129, 182, 76, 0.85)";
const ARROW_GAME = "rgba(250, 65, 45, 0.75)";
const ARROW_HINT = "rgba(77, 195, 234, 0.85)";

function soundOn(): boolean {
  try {
    return localStorage.getItem("review.sound") !== "false";
  } catch {
    return true;
  }
}

function reducedMotion(): boolean {
  return typeof window !== "undefined" && !!window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
}

function squares(uci: string): { from: string; to: string } {
  return { from: uci.slice(0, 2), to: uci.slice(2, 4) };
}

const GRADE_TEXT: Record<TrainGradeResponse["grade"], { title: string; mood: CoachMood; tone: string }> = {
  correct: { title: "Correct", mood: "happy", tone: "good" },
  not_incorrect: { title: "Playable, but not the idea", mood: "neutral", tone: "ok" },
  incorrect: { title: "Not this time", mood: "concerned", tone: "bad" },
  same_as_game: { title: "Same as your game", mood: "concerned", tone: "bad" },
};

type Stage =
  | { kind: "intro" }
  | { kind: "move" }
  | { kind: "grading"; move: PlayedMove }
  | { kind: "graded"; move: PlayedMove; result: TrainGradeResponse }
  | { kind: "playout"; moves: PlayedMove[]; state: PlayOut; thinking: boolean };

interface DrillProps {
  item: StoredItem;
  db: TrainerDb;
  themes: ThemeInfo[];
  engine: () => EngineController;
  onDone: (result: DrillResult) => void;
  position: { index: number; total: number };
}

/** One drill: the pitch move, the player's move, the grade, a retry, the line, and a play-out. */
export function Drill({ item, db, themes, engine, onDone, position }: DrillProps) {
  const side: Side = item.user_side;
  const [flipped, setFlipped] = useState(side === "black");
  const [stage, setStage] = useState<Stage>({ kind: "intro" });
  const [hint, setHint] = useState<0 | 1 | 2 | 3>(0);
  const [attempts, setAttempts] = useState(0);
  const [firstGrade, setFirstGrade] = useState<TrainGradeResponse["grade"] | null>(null);
  const [nextDue, setNextDue] = useState(0);
  const [line, setLine] = useState<{ moves: PlayedMove[]; at: number; playing: boolean } | null>(null);
  const [compare, setCompare] = useState<"none" | "game" | "best">("none");
  const [promotion, setPromotion] = useState<{ from: string; to: string } | null>(null);
  const [tablebase, setTablebase] = useState(false);
  const startedAt = useRef(Date.now());
  const sound = useMemo(soundOn, []);

  useEffect(() => {
    void tablebaseEnabled(db).then(setTablebase);
  }, [db]);

  // The pitch move: the position before it, then the opponent's move with its sound.
  useEffect(() => {
    const wait = reducedMotion() ? 0 : PITCH_DELAY_MS;
    const t = setTimeout(() => {
      setStage({ kind: "move" });
      startedAt.current = Date.now();
      const pitch = lineMoves(item.pre_pitch_fen, [item.pitch_move.uci], 1)[0];
      if (sound && pitch) playSounds([moveSound(pitch.flags)]);
    }, wait);
    return () => clearTimeout(t);
  }, [item, sound]);

  // Replaying a line, one move at a time.
  useEffect(() => {
    if (!line?.playing) return;
    if (line.at >= line.moves.length) {
      setLine({ ...line, playing: false });
      return;
    }
    const t = setTimeout(() => {
      const m = line.moves[line.at];
      if (sound && m) playSounds([moveSound(m.flags)]);
      setLine({ ...line, at: line.at + 1 });
    }, reducedMotion() ? 0 : LINE_STEP_MS);
    return () => clearTimeout(t);
  }, [line, sound]);

  const showLine = useCallback(
    (uci: string[]) => {
      const moves = lineMoves(item.start_fen, uci, 12);
      setCompare("none");
      setLine({ moves, at: 0, playing: true });
    },
    [item.start_fen],
  );

  const finishAttempt = useCallback(
    async (result: TrainGradeResponse) => {
      const retry = attempts > 0;
      const entry = await recordAttempt(db, item, { grade: result.grade, hint, retry, ms: Date.now() - startedAt.current });
      setNextDue(entry.due_at);
      if (!retry) setFirstGrade(result.grade);
      setAttempts((n) => n + 1);
    },
    [attempts, db, hint, item],
  );

  const submit = useCallback(
    async (move: PlayedMove) => {
      setStage({ kind: "grading", move });
      if (sound) playSounds([moveSound(move.flags)]);
      const result = await gradeDrillMove(item, move.uci, {
        engine: controllerEngine(engine()),
        grade: api.gradeDrill,
      });
      if (sound) playSounds([result.grade === "correct" ? "correct" : "incorrect"]);
      setStage({ kind: "graded", move, result });
      void finishAttempt(result);
      // A wrong move shows at once what is wrong with it.
      if (result.grade !== "correct" && result.show_line.length > 1) showLine(result.show_line);
    },
    [engine, finishAttempt, item, showLine, sound],
  );

  const playFrom = (fen: string, from: string, to: string, piece?: PromotionPiece): PlayedMove | null => playMove(fen, from, to, piece);

  /** The engine answers in a play-out, then the play-out is checked. */
  const engineReply = useCallback(
    async (moves: PlayedMove[], state: PlayOut) => {
      const fen = moves.at(-1)?.fen ?? item.start_fen;
      setStage({ kind: "playout", moves, state, thinking: true });
      const c = engine();
      const analysis = await new Promise<Analysis | null>((resolve) => {
        const t = setTimeout(() => resolve(null), 8000);
        c.analyze(fen, ENGINE_REPLY_DEPTH, (a) => {
          if (a.fen === fen && a.done) {
            clearTimeout(t);
            resolve(a);
          }
        }, 1);
      });
      const best = analysis?.lines[0];
      const verdict = tablebase ? await probe(fen, db) : null;
      let next = stepPlayOut(state, {
        eval: best?.eval ?? null,
        ending: ending(fen),
        toMove: fen.split(" ")[1] === "b" ? "black" : "white",
        promoted: !!moves.at(-1)?.flags.promotion,
        verdict,
      });
      if (next.state !== "playing" || !best?.pv[0]) {
        setStage({ kind: "playout", moves, state: next.state === "playing" ? { ...next, state: "lost" } : next, thinking: false });
        return;
      }
      const reply = lineMoves(fen, [best.pv[0]], 1)[0];
      if (!reply) return;
      if (sound) playSounds([moveSound(reply.flags)]);
      const after = [...moves, reply];
      next = stepPlayOut(next, {
        eval: best.eval,
        ending: ending(reply.fen),
        toMove: reply.fen.split(" ")[1] === "b" ? "black" : "white",
        promoted: false,
        verdict: null,
      });
      setStage({ kind: "playout", moves: after, state: next, thinking: false });
    },
    [db, engine, item.start_fen, sound, tablebase],
  );

  const startPlay = () => {
    if (stage.kind !== "graded" || !item.play_out) return;
    setLine(null);
    setCompare("none");
    const state = startPlayOut(item.play_out, side, item.best_line.length);
    void engineReply([stage.move], state);
  };

  const fen = (() => {
    if (stage.kind === "intro") return item.pre_pitch_fen;
    if (line) return line.at === 0 ? item.start_fen : line.moves[line.at - 1]!.fen;
    if (stage.kind === "grading" || stage.kind === "graded") return stage.move.fen;
    if (stage.kind === "playout") return stage.moves.at(-1)?.fen ?? item.start_fen;
    return item.start_fen;
  })();

  const canMove =
    !line &&
    (stage.kind === "move" || (stage.kind === "playout" && stage.state.state === "playing" && !stage.thinking && fen.split(" ")[1] === (side === "white" ? "w" : "b")));

  const onDrop = (from: string, to: string, piece?: PromotionPiece): boolean => {
    if (!canMove) return false;
    if (!piece && needsPromotion(fen, from, to)) {
      setPromotion({ from, to });
      return false;
    }
    const move = playFrom(fen, from, to, piece);
    if (!move) return false;
    if (stage.kind === "playout") {
      if (sound) playSounds([moveSound(move.flags)]);
      void engineReply([...stage.moves, move], stage.state);
    } else {
      void submit(move);
    }
    return true;
  };

  const retry = () => {
    setLine(null);
    setCompare("none");
    setStage({ kind: "move" });
    startedAt.current = Date.now();
  };

  const done = () =>
    onDone({
      item,
      grade: firstGrade ?? "incorrect",
      hint,
      retried: attempts > 1,
      nextDue,
      playOut: stage.kind === "playout" ? stage.state.state : null,
    });

  const graded = stage.kind === "graded" ? stage.result : null;
  const canRetry = !!graded && graded.grade !== "correct" && attempts < 2;

  // Keyboard: h hint, r retry, n next, f flip, arrows step the line.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.ctrlKey || e.metaKey || e.altKey) return;
      if (e.key === "h" && stage.kind === "move") setHint((h) => (h < 3 ? ((h + 1) as 1 | 2 | 3) : h));
      else if (e.key === "r" && canRetry) retry();
      else if (e.key === "n" && (graded || stage.kind === "playout")) done();
      else if (e.key === "f") setFlipped((f) => !f);
      else if (line && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
        const at = Math.max(0, Math.min(line.moves.length, line.at + (e.key === "ArrowRight" ? 1 : -1)));
        setLine({ ...line, at, playing: false });
      } else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const arrows: Arrow[] = [];
  const styles: Record<string, CSSProperties> = {};
  if (stage.kind === "move" && hint >= 1) styles[squares(item.correct_move.uci).from] = HINT_SQUARE;
  if (stage.kind === "move" && hint >= 3) {
    const s = squares(item.correct_move.uci);
    arrows.push({ startSquare: s.from, endSquare: s.to, color: ARROW_HINT });
  }
  if (compare !== "none" && !line) {
    const s = squares(compare === "game" ? item.played_move.uci : item.correct_move.uci);
    arrows.push({ startSquare: s.from, endSquare: s.to, color: compare === "game" ? ARROW_GAME : ARROW_BEST });
  }
  if (stage.kind === "intro") {
    const s = squares(item.pitch_move.uci);
    styles[s.from] = { background: "rgba(255, 255, 255, 0.12)" };
  }

  const board = (
    <BoardView
      fen={compare !== "none" && !line ? item.start_fen : fen}
      flipped={flipped}
      arrows={arrows}
      squareStyles={styles}
      moves={
        canMove
          ? {
              legal: (from, to) => canPlay(fen, from, to),
              onDrop: (from, to) => onDrop(from, to),
              targets: (sq) => legalTargets(fen, sq),
            }
          : null
      }
      promotion={
        promotion
          ? ({
              square: promotion.to,
              side,
              onPick: (piece) => {
                const p = promotion;
                setPromotion(null);
                if (piece) onDrop(p.from, p.to, piece);
              },
            } satisfies BoardPromotion)
          : null
      }
    />
  );

  const opponent = item.source.opponent ?? "your opponent";
  const moveLabel = `move ${item.source.move_number}`;
  const tone = graded ? GRADE_TEXT[graded.grade] : null;

  return (
    <div className="drill">
      <div className="drill__board">{board}</div>
      <aside className="drill__panel panel" aria-live="polite">
        <header className="drill__head">
          <span className="drill__count">
            {position.index + 1} / {position.total}
          </span>
          <span className={`drill__kind drill__kind--${item.kind}`}>{KIND_LABELS[item.kind]}</span>
          <span className="drill__theme">{themeLabel(item.theme, themes)}</span>
        </header>
        <p className="drill__context muted">
          From your game against {opponent}, {moveLabel}
          {item.source.date ? ` (${item.source.date})` : ""}. You play {side}.
          {item.endgame_type ? ` ${item.endgame_type} ending.` : ""}
        </p>

        <CoachBubble mood={tone?.mood ?? "neutral"}>
          <div className="drill__speech">
            {!graded && stage.kind !== "playout" && (
              <>
                <p>{stage.kind === "intro" ? `Your opponent plays ${item.pitch_move.san}.` : item.goal}</p>
                {hint >= 2 && <p className="drill__hint">{item.hint}</p>}
              </>
            )}
            {graded && (
              <>
                <p className={`drill__grade drill__grade--${tone!.tone}`}>
                  {tone!.title}: {graded.user_move.san}
                </p>
                {graded.reply.map((r) => (
                  <p key={r}>{r}</p>
                ))}
                {graded.coach.sentences.slice(0, 1).map((s, i) => (
                  <p key={i} className="muted">
                    {s.fragments.map((f) => f.text).join("")}
                  </p>
                ))}
              </>
            )}
            {stage.kind === "playout" && (
              <p>
                {stage.state.state === "playing"
                  ? stage.thinking
                    ? "Thinking…"
                    : item.play_out === "win"
                      ? "Now win it. The engine defends."
                      : "Now hold the draw. The engine attacks."
                  : stage.state.state === "won"
                    ? item.play_out === "win"
                      ? "Converted. Well played."
                      : "Held. Well defended."
                    : "It slipped away this time. Try the line again later."}
              </p>
            )}
          </div>
        </CoachBubble>

        <div className="drill__actions">
          {stage.kind === "move" && (
            <button type="button" className="button" onClick={() => setHint((h) => (h < 3 ? ((h + 1) as 1 | 2 | 3) : h))} disabled={hint >= 3}>
              {hint === 0 ? "Hint: the piece" : hint === 1 ? "Hint: the idea" : hint === 2 ? "Show the move" : "Move shown"}
            </button>
          )}
          {stage.kind === "grading" && <span className="muted">Checking your move…</span>}
          {graded && (
            <>
              {canRetry && (
                <button type="button" className="button" onClick={retry}>
                  Try again
                </button>
              )}
              <button type="button" className="button" onClick={() => showLine(graded.grade === "correct" ? graded.show_line : item.best_line)}>
                {graded.grade === "correct" ? "Show the line" : "Show the best line"}
              </button>
              {graded.grade !== "correct" && graded.show_line.length > 1 && (
                <button type="button" className="button" onClick={() => showLine(graded.show_line)}>
                  Why not my move
                </button>
              )}
              <button
                type="button"
                className="button"
                onClick={() => {
                  setLine(null);
                  setCompare((c) => (c === "game" ? "best" : "game"));
                }}
              >
                {compare === "game" ? "Show the best move" : "Compare with your game"}
              </button>
              {graded.grade === "correct" && item.play_out && (
                <button type="button" className="button" onClick={startPlay}>
                  {item.play_out === "win" ? "Play it out: win it" : "Play it out: hold it"}
                </button>
              )}
              <button type="button" className="button button--primary" onClick={done}>
                Next
              </button>
            </>
          )}
          {stage.kind === "playout" && stage.state.state !== "playing" && (
            <button type="button" className="button button--primary" onClick={done}>
              Next
            </button>
          )}
          <button type="button" className="button button--ghost" onClick={() => setFlipped((f) => !f)} aria-label="Flip the board">
            Flip
          </button>
        </div>
        {line && (
          <ol className="drill__line" aria-label="Line">
            {line.moves.map((m, i) => (
              <li key={i} className={i < line.at ? "drill__line-move drill__line-move--played" : "drill__line-move"}>
                <button type="button" onClick={() => setLine({ ...line, at: i + 1, playing: false })}>
                  {m.san}
                </button>
              </li>
            ))}
          </ol>
        )}
        {graded?.fallback && <p className="muted small">Graded without the engine.</p>}
        {tablebase && item.kind === "endgame" && <p className="muted small">Exact verdicts: {TABLEBASE_SOURCE}.</p>}
        <p className="muted small drill__keys">Keys: h hint · r retry · n next · f flip · ← → line</p>
      </aside>
    </div>
  );
}

interface ScreenProps {
  items: StoredItem[];
  db: TrainerDb;
  themes: ThemeInfo[];
  onFinish: (results: DrillResult[]) => void;
  onExit: () => void;
}

/** A session of drills, one after another, then the summary. */
export function DrillScreen({ items, db, themes, onFinish, onExit }: ScreenProps) {
  const [index, setIndex] = useState(0);
  const results = useRef<DrillResult[]>([]);
  const controller = useRef<EngineController | null>(null);
  const engine = useCallback(() => {
    controller.current ??= new EngineController(browserWorker, { multipv: 1 });
    return controller.current;
  }, []);
  useEffect(() => () => controller.current?.dispose(), []);

  const item = items[index];
  if (!item) return null;
  return (
    <section className="drill-screen">
      <div className="drill-screen__bar">
        <button type="button" className="button button--ghost" onClick={onExit}>
          ← Trainer
        </button>
        <div className="drill-screen__progress" role="progressbar" aria-valuemin={0} aria-valuemax={items.length} aria-valuenow={index}>
          <span style={{ width: `${(index / items.length) * 100}%` }} />
        </div>
      </div>
      <Drill
        key={item.id}
        item={item}
        db={db}
        themes={themes}
        engine={engine}
        position={{ index, total: items.length }}
        onDone={(r) => {
          results.current = [...results.current, r];
          if (index + 1 >= items.length) onFinish(results.current);
          else setIndex(index + 1);
        }}
      />
    </section>
  );
}
