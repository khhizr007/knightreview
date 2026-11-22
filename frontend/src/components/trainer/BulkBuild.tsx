import { useRef, useState } from "react";
import { api } from "../../api/client";
import type { ReviewResult } from "../../api/generated/ReviewResult";
import { fetchRecentGames, SITE_NAMES, type OnlineGame, type Site } from "../../lib/onlineGames";
import { myUsernames, rememberUsername } from "../../lib/players";
import type { TrainerDb } from "../../lib/trainer/db";
import { addReview } from "../../lib/trainer/store";

const BUILT_KEY = "built_urls";
const POLL_MS = 1000;
/** Reviews run at once (the server's default MAX_CONCURRENT_REVIEWS). */
const PARALLEL = 2;
const SPEEDS = ["bullet", "blitz", "rapid", "classical", "daily"] as const;

async function reviewOne(game: OnlineGame, quick: boolean, signal: AbortSignal): Promise<ReviewResult> {
  const status = await api.createReview({
    pgn: game.pgn,
    strength: quick ? "fast" : "standard",
    ...(game.userSide ? { player_side: game.userSide } : {}),
  });
  for (;;) {
    if (signal.aborted) {
      void api.cancelReview(status.id).catch(() => {});
      throw new DOMException("cancelled", "AbortError");
    }
    const s = await api.reviewStatus(status.id, signal);
    if (s.state === "completed") return api.reviewResult(status.id, signal);
    if (s.state === "failed" || s.state === "cancelled") throw new Error(s.error ?? `The review ${s.state}.`);
    await new Promise((r) => setTimeout(r, POLL_MS));
  }
}

/** "Train from my last games": review recent online games and keep their drills. */
export function BulkBuild({ db, onAdded }: { db: TrainerDb; onAdded: () => void }) {
  const [site, setSite] = useState<Site>("chesscom");
  const [username, setUsername] = useState(() => myUsernames()[0] ?? "");
  const [count, setCount] = useState(20);
  const [speeds, setSpeeds] = useState<Set<string>>(new Set(["blitz", "rapid", "classical"]));
  const [quick, setQuick] = useState(false);
  const [progress, setProgress] = useState<{ done: number; total: number; drills: number; failed: number } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const abort = useRef<AbortController | null>(null);

  const run = async () => {
    setError(null);
    setBusy(true);
    const controller = new AbortController();
    abort.current = controller;
    try {
      const games = (await fetchRecentGames(site, username.trim(), { max: 200, signal: controller.signal }))
        .filter((g) => !g.chess960 && speeds.has(g.speed))
        .slice(0, count);
      rememberUsername(username.trim());
      const built = new Set(((await db.get("meta", BUILT_KEY))?.value as string[] | undefined) ?? []);
      const todo = games.filter((g) => !built.has(g.url));
      const state = { done: 0, total: todo.length, drills: 0, failed: 0 };
      setProgress({ ...state });
      const queue = [...todo];
      const worker = async () => {
        for (let g = queue.shift(); g; g = queue.shift()) {
          if (controller.signal.aborted) return;
          try {
            const review = await reviewOne(g, quick, controller.signal);
            state.drills += await addReview(db, review, g.userSide);
            built.add(g.url);
            await db.put("meta", [{ key: BUILT_KEY, value: [...built] }]);
            onAdded();
          } catch (e) {
            if (controller.signal.aborted) return;
            state.failed++;
            if (!(e instanceof Error)) throw e;
          }
          state.done++;
          setProgress({ ...state });
        }
      };
      await Promise.all(Array.from({ length: PARALLEL }, worker));
    } catch (e) {
      if (!controller.signal.aborted) setError(e instanceof Error ? e.message : String(e));
    } finally {
      abort.current = null;
      setBusy(false);
    }
  };

  const running = busy;
  return (
    <div className="tcard bulk">
      <h3 className="tcard__title">Train from my last games</h3>
      <div className="bulk__form">
        <select value={site} onChange={(e) => setSite(e.target.value as Site)} aria-label="Site">
          {(Object.keys(SITE_NAMES) as Site[]).map((s) => (
            <option key={s} value={s}>
              {SITE_NAMES[s]}
            </option>
          ))}
        </select>
        <input value={username} onChange={(e) => setUsername(e.target.value)} placeholder="Username" aria-label="Username" />
        <select value={count} onChange={(e) => setCount(Number(e.target.value))} aria-label="Games">
          {[20, 50, 100].map((n) => (
            <option key={n} value={n}>
              Last {n}
            </option>
          ))}
        </select>
      </div>
      <div className="bulk__speeds">
        {SPEEDS.map((s) => (
          <label key={s} className="bulk__speed">
            <input
              type="checkbox"
              checked={speeds.has(s)}
              onChange={(e) => {
                const next = new Set(speeds);
                if (e.target.checked) next.add(s);
                else next.delete(s);
                setSpeeds(next);
              }}
            />
            {s}
          </label>
        ))}
        <label className="bulk__speed" title="A shallower search: faster, good enough to find drills.">
          <input type="checkbox" checked={quick} onChange={(e) => setQuick(e.target.checked)} />
          quick build
        </label>
      </div>
      <div className="drill__actions">
        {running ? (
          <button type="button" className="button" onClick={() => abort.current?.abort()}>
            Stop
          </button>
        ) : (
          <button type="button" className="button button--primary" disabled={!username.trim() || speeds.size === 0} onClick={() => void run()}>
            Review and build drills
          </button>
        )}
      </div>
      {progress && (
        <p className="muted small">
          {progress.total === 0
            ? "No new games to review."
            : `Reviewed ${progress.done} of ${progress.total} games · ${progress.drills} new drills${progress.failed ? ` · ${progress.failed} failed` : ""}.`}
        </p>
      )}
      {error && <div className="notice notice--error">{error}</div>}
      <p className="muted small">Each game takes the server about a minute at the standard strength. Games already built are skipped.</p>
    </div>
  );
}
