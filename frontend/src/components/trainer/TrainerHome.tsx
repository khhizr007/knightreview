import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ThemeInfo } from "../../api/generated/ThemeInfo";
import type { ThemeFamily } from "../../api/generated/ThemeFamily";
import { CARDS, type ConceptCard } from "../../lib/trainer/concepts";
import { exportAll, importAll, trainerDb, type StoredItem, type TrainerDb } from "../../lib/trainer/db";
import { buildSession, dueCount, type SessionFilter } from "../../lib/trainer/session";
import { panelStats } from "../../lib/trainer/stats";
import { loadState, STREAK_DRILLS, type TrainerState } from "../../lib/trainer/store";
import { setTablebaseEnabled, tablebaseEnabled, TABLEBASE_SOURCE } from "../../lib/trainer/tablebase";
import { loadThemes, themeLabel } from "../../lib/trainer/themes";
import { BulkBuild } from "./BulkBuild";
import { DrillScreen, type DrillResult } from "./DrillScreen";
import { ConceptCardView, OpeningLab, SessionSummary } from "./Labs";
import { EndgameStats, PhaseBars, ThemeGrid } from "./Panels";

type View =
  | { kind: "home" }
  | { kind: "session"; items: StoredItem[]; filter: SessionFilter }
  | { kind: "summary"; results: DrillResult[]; filter: SessionFilter }
  | { kind: "card"; card: ConceptCard };

/** Attempts after which the trainer reminds the player to export (browser data can be cleared). */
const EXPORT_REMINDER = 50;

function download(name: string, text: string) {
  const url = URL.createObjectURL(new Blob([text], { type: "application/json" }));
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  URL.revokeObjectURL(url);
}

/** The trainer: the weakness map, the labs, and the sessions. */
export function TrainerHome({ onBack }: { onBack: () => void }) {
  const [db, setDb] = useState<TrainerDb | null>(null);
  const [state, setState] = useState<TrainerState | null>(null);
  const [infos, setInfos] = useState<ThemeInfo[]>([]);
  const [view, setView] = useState<View>({ kind: "home" });
  const [tablebase, setTablebase] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);

  const reload = useCallback(async (d: TrainerDb) => {
    setState(await loadState(d));
  }, []);

  useEffect(() => {
    void (async () => {
      const d = await trainerDb();
      setDb(d);
      setTablebase(await tablebaseEnabled(d));
      await reload(d);
    })();
    void loadThemes().then(setInfos);
  }, [reload]);

  const now = Date.now();
  const stats = useMemo(() => (state ? panelStats(state.games) : null), [state]);

  const start = (filter: SessionFilter) => {
    if (!state) return;
    const items = buildSession({ items: state.items, schedule: state.schedule, themes: state.themes, now: Date.now(), filter });
    if (items.length === 0) {
      setMessage("No drills to practise here yet.");
      return;
    }
    setMessage(null);
    setView({ kind: "session", items, filter });
  };

  if (!db || !state || !stats) {
    return (
      <section className="card progress-card">
        <p className="muted">Opening your trainer…</p>
      </section>
    );
  }

  if (view.kind === "session") {
    return (
      <DrillScreen
        items={view.items}
        db={db}
        themes={infos}
        onExit={() => {
          void reload(db);
          setView({ kind: "home" });
        }}
        onFinish={(results) => {
          void reload(db);
          setView({ kind: "summary", results, filter: view.filter });
        }}
      />
    );
  }
  if (view.kind === "summary") {
    return <SessionSummary results={view.results} infos={infos} onHome={() => setView({ kind: "home" })} onAgain={() => start(view.filter)} />;
  }
  if (view.kind === "card") return <ConceptCardView card={view.card} onClose={() => setView({ kind: "home" })} />;

  const due = dueCount(state.items, state.schedule, now);
  const fresh = state.items.filter((it) => !state.schedule.has(it.id)).length;
  const lastGame = [...state.games].sort((a, b) => b.added_at - a.added_at)[0];
  const family = (f: ThemeFamily) => state.items.filter((it) => it.kind === f).length;
  const attempts = state.attempts.length;

  return (
    <div className="trainer">
      <header className="trainer__head">
        <div>
          <h1 className="trainer__title">Trainer</h1>
          <p className="muted">Practise the moments you got wrong in your own games.</p>
        </div>
        <button type="button" className="button button--ghost" onClick={onBack}>
          ← Review a game
        </button>
      </header>

      <section className="trainer__hero tcard">
        <div className="trainer__numbers">
          <div className="tstat">
            <span className="tstat__num">{due}</span>
            <span className="tstat__label">due</span>
          </div>
          <div className="tstat">
            <span className="tstat__num">{fresh}</span>
            <span className="tstat__label">new</span>
          </div>
          <div className="tstat">
            <span className="tstat__num">{state.items.length}</span>
            <span className="tstat__label">drills</span>
          </div>
          <div className="tstat" title={`Days in a row with ${STREAK_DRILLS} or more drills`}>
            <span className="tstat__num">{state.streak.days}</span>
            <span className="tstat__label">day streak</span>
          </div>
        </div>
        <div className="drill__actions">
          <button type="button" className="button button--primary button--large" disabled={state.items.length === 0} onClick={() => start({ kind: "today" })}>
            Start today's training
          </button>
          <button type="button" className="button" disabled={family("endgame") === 0} onClick={() => start({ kind: "family", family: "endgame" })}>
            Endgames only
          </button>
          <button type="button" className="button" disabled={family("opening") === 0} onClick={() => start({ kind: "family", family: "opening" })}>
            Openings only
          </button>
          <button type="button" className="button" disabled={!lastGame} onClick={() => lastGame && start({ kind: "game", gameKey: lastGame.game_key })}>
            Retry my last game
          </button>
        </div>
        {message && <p className="muted">{message}</p>}
        {state.items.length === 0 && (
          <p className="muted">
            Drills come from your reviews. Review a game where you chose your side, or build drills from your recent online games below.
          </p>
        )}
        {attempts >= EXPORT_REMINDER && attempts % EXPORT_REMINDER < 10 && (
          <p className="muted small">Your progress lives only in this browser. Export it now and then to keep it safe.</p>
        )}
      </section>

      <div className="trainer__grid">
        {stats.games > 0 && <PhaseBars stats={stats} />}
        {stats.games > 0 && <EndgameStats stats={stats} />}
        <ThemeGrid items={state.items} themes={state.themes} infos={infos} onTheme={(theme) => start({ kind: "theme", theme })} />

        <div className="tcard">
          <h3 className="tcard__title">Endgame Lab</h3>
          <p className="muted small">Short lessons with positions to solve.</p>
          <ul className="tlist">
            {CARDS.map((c) => (
              <li key={c.id}>
                <span>
                  <strong>{c.title}</strong> <span className="muted">· {c.positions.length} positions</span>
                </span>
                <button type="button" className="button button--small" onClick={() => setView({ kind: "card", card: c })}>
                  Open
                </button>
              </li>
            ))}
          </ul>
          <label className="bulk__speed">
            <input
              type="checkbox"
              checked={tablebase}
              onChange={(e) => {
                setTablebase(e.target.checked);
                void setTablebaseEnabled(db, e.target.checked);
              }}
            />
            Exact endgame verdicts (asks the {TABLEBASE_SOURCE} about positions with 7 pieces or fewer)
          </label>
        </div>

        <OpeningLab items={state.items} games={state.games} infos={infos} onTheme={(theme) => start({ kind: "theme", theme })} />

        <BulkBuild db={db} onAdded={() => void reload(db)} />

        <div className="tcard">
          <h3 className="tcard__title">Your data</h3>
          <p className="muted small">Everything stays in this browser. No account, nothing on our server.</p>
          <div className="drill__actions">
            <button
              type="button"
              className="button"
              onClick={async () => download("knightreview-trainer.json", JSON.stringify(await exportAll(db)))}
            >
              Export
            </button>
            <button type="button" className="button" onClick={() => fileInput.current?.click()}>
              Import
            </button>
            <input
              ref={fileInput}
              type="file"
              accept="application/json,.json"
              hidden
              onChange={async (e) => {
                const file = e.target.files?.[0];
                e.target.value = "";
                if (!file) return;
                try {
                  const n = await importAll(db, JSON.parse(await file.text()));
                  setMessage(`Imported ${n} records.`);
                  await reload(db);
                } catch (err) {
                  setMessage(err instanceof Error ? err.message : String(err));
                }
              }}
            />
          </div>
          {state.items.length > 0 && (
            <p className="muted small">
              Most practised theme:{" "}
              {themeLabel(
                [...state.themes.values()].sort((a, b) => b.count - a.count)[0]?.theme ?? state.items[0]!.theme,
                infos,
              )}
              .
            </p>
          )}
        </div>
      </div>
    </div>
  );
}
