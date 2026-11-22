import { useMemo, useState } from "react";
import type { ThemeInfo } from "../../api/generated/ThemeInfo";
import { canPlay, legalTargets, needsPromotion, playMove } from "../../lib/chess";
import { moveSound, playSounds } from "../../lib/sound";
import type { ConceptCard } from "../../lib/trainer/concepts";
import type { StoredGame, StoredItem } from "../../lib/trainer/db";
import { DAY_MS } from "../../lib/trainer/schedule";
import { themeLabel } from "../../lib/trainer/themes";
import { BoardView } from "../BoardView";
import type { DrillResult } from "./DrillScreen";

/** A concept card: the idea, then its positions to solve one by one. */
export function ConceptCardView({ card, onClose }: { card: ConceptCard; onClose: () => void }) {
  const [index, setIndex] = useState(0);
  const [answer, setAnswer] = useState<{ fen: string; ok: boolean; san: string } | null>(null);
  const [solved, setSolved] = useState(0);
  const pos = card.positions[index]!;
  const side = pos.fen.split(" ")[1] === "b" ? "black" : "white";
  const fen = answer?.fen ?? pos.fen;

  const onDrop = (from: string, to: string): boolean => {
    if (answer?.ok) return false;
    const move = playMove(pos.fen, from, to, needsPromotion(pos.fen, from, to) ? "q" : undefined);
    if (!move) return false;
    const ok = pos.solutions.includes(move.uci);
    playSounds([moveSound(move.flags), ok ? "correct" : "incorrect"]);
    setAnswer({ fen: move.fen, ok, san: move.san });
    if (ok) setSolved((n) => n + 1);
    return true;
  };

  return (
    <section className="concept">
      <div className="drill-screen__bar">
        <button type="button" className="button button--ghost" onClick={onClose}>
          ← Endgame Lab
        </button>
      </div>
      <div className="drill">
        <div className="drill__board">
          <BoardView
            fen={fen}
            flipped={side === "black"}
            arrows={[]}
            squareStyles={{}}
            moves={
              answer?.ok
                ? null
                : {
                    legal: (f, t) => canPlay(pos.fen, f, t),
                    onDrop,
                    targets: (sq) => legalTargets(pos.fen, sq),
                  }
            }
          />
        </div>
        <aside className="drill__panel panel">
          <h2 className="concept__title">{card.title}</h2>
          <p className="concept__idea">{card.idea}</p>
          <p className="drill__count muted">
            Position {index + 1} of {card.positions.length} · solved {solved}
          </p>
          <p className="concept__task">{pos.task}</p>
          {answer && (
            <p className={`drill__grade drill__grade--${answer.ok ? "good" : "bad"}`}>
              {answer.ok ? `${answer.san}: correct. ${pos.why}` : `${answer.san} does not keep the result.`}
            </p>
          )}
          <div className="drill__actions">
            {answer && !answer.ok && (
              <button type="button" className="button" onClick={() => setAnswer(null)}>
                Try again
              </button>
            )}
            {answer?.ok && index + 1 < card.positions.length && (
              <button
                type="button"
                className="button button--primary"
                onClick={() => {
                  setIndex(index + 1);
                  setAnswer(null);
                }}
              >
                Next position
              </button>
            )}
            {answer?.ok && index + 1 >= card.positions.length && (
              <button type="button" className="button button--primary" onClick={onClose}>
                Done
              </button>
            )}
          </div>
          <p className="muted small">Positions composed for KnightReview; answers checked with the Lichess tablebase.</p>
        </aside>
      </div>
    </section>
  );
}

/** Opening mistakes by opening, where the player leaves the book, and their habits. */
export function OpeningLab({
  items,
  games,
  infos,
  onTheme,
}: {
  items: StoredItem[];
  games: StoredGame[];
  infos: ThemeInfo[];
  onTheme: (theme: string) => void;
}) {
  const groups = useMemo(() => {
    const m = new Map<string, { name: string; side: string; items: StoredItem[] }>();
    for (const it of items) {
      if (it.kind !== "opening") continue;
      const name = it.opening_name ?? "Unnamed opening";
      const key = `${name}|${it.user_side}`;
      const g = m.get(key) ?? { name, side: it.user_side, items: [] };
      g.items.push(it);
      m.set(key, g);
    }
    return [...m.values()].sort((a, b) => b.items.length - a.items.length);
  }, [items]);
  const exits = useMemo(() => {
    const m = new Map<string, { name: string; plies: number[]; lost: number[] }>();
    for (const g of games) {
      if (!g.opening || !g.book_exit) continue;
      const key = `${g.opening.name}|${g.side}`;
      const e = m.get(key) ?? { name: `${g.opening.name} as ${g.side}`, plies: [], lost: [] };
      e.plies.push(g.book_exit.ply);
      e.lost.push(g.book_exit.ep_lost);
      m.set(key, e);
    }
    return [...m.values()].sort((a, b) => b.plies.length - a.plies.length).slice(0, 8);
  }, [games]);
  const mean = (xs: number[]) => xs.reduce((a, b) => a + b, 0) / xs.length;
  return (
    <div className="tcard">
      <h3 className="tcard__title">Opening Lab</h3>
      {groups.length === 0 && <p className="muted">No opening drills yet. Review a few games first.</p>}
      <ul className="tlist">
        {groups.map((g) => {
          const themes = new Map<string, number>();
          for (const it of g.items) themes.set(it.theme, (themes.get(it.theme) ?? 0) + 1);
          const top = [...themes.entries()].sort((a, b) => b[1] - a[1])[0];
          return (
            <li key={`${g.name}|${g.side}`}>
              <span>
                <strong>{g.name}</strong> as {g.side}: {g.items.length} {g.items.length === 1 ? "mistake" : "mistakes"}
                {top && top[1] > 1 ? `, ${top[1]} about ${themeLabel(top[0], infos).toLowerCase()}` : ""}
              </span>
              {top && (
                <button type="button" className="button button--small" onClick={() => onTheme(top[0])}>
                  Practise
                </button>
              )}
            </li>
          );
        })}
      </ul>
      {exits.length > 0 && (
        <>
          <h4 className="tcard__sub">Where you leave the book</h4>
          <ul className="tlist">
            {exits.map((e) => (
              <li key={e.name}>
                <span>
                  {e.name}: around move {Math.ceil(mean(e.plies) / 2)} ({e.plies.length} {e.plies.length === 1 ? "game" : "games"})
                </span>
                <span className="muted">−{(mean(e.lost) * 100).toFixed(0)}% winning chances over the next 5 moves</span>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

/** After a session: grades, themes touched, when the drills come back. */
export function SessionSummary({
  results,
  infos,
  onHome,
  onAgain,
}: {
  results: DrillResult[];
  infos: ThemeInfo[];
  onHome: () => void;
  onAgain: () => void;
}) {
  const count = (g: DrillResult["grade"]) => results.filter((r) => r.grade === g).length;
  const themes = [...new Set(results.map((r) => r.item.theme))];
  const soonest = Math.min(...results.map((r) => r.nextDue));
  const days = Number.isFinite(soonest) ? Math.max(0, Math.round((soonest - Date.now()) / DAY_MS)) : null;
  return (
    <section className="card summary">
      <h1>Session done</h1>
      <div className="tstats">
        <div className="tstat">
          <span className="tstat__num tstat__num--good">{count("correct")}</span>
          <span className="tstat__label">correct</span>
        </div>
        <div className="tstat">
          <span className="tstat__num tstat__num--ok">{count("not_incorrect")}</span>
          <span className="tstat__label">playable</span>
        </div>
        <div className="tstat">
          <span className="tstat__num tstat__num--bad">{count("incorrect") + count("same_as_game")}</span>
          <span className="tstat__label">to review</span>
        </div>
      </div>
      <p className="muted">Themes: {themes.map((t) => themeLabel(t, infos)).join(", ")}.</p>
      {days !== null && <p className="muted">{days === 0 ? "Some drills come back today." : `Next drills come back in ${days} ${days === 1 ? "day" : "days"}.`}</p>}
      <div className="drill__actions">
        <button type="button" className="button button--primary" onClick={onAgain}>
          Another session
        </button>
        <button type="button" className="button" onClick={onHome}>
          Back to the trainer
        </button>
      </div>
    </section>
  );
}
