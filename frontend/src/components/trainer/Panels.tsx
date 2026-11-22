import type { ThemeInfo } from "../../api/generated/ThemeInfo";
import type { ThemeFamily } from "../../api/generated/ThemeFamily";
import { formatAccuracy, percent } from "../../lib/format";
import type { StoredItem, ThemeStat } from "../../lib/trainer/db";
import { FAMILIES, FAMILY_LABELS as ENDGAME_LABELS } from "../../lib/trainer/endgame";
import { MASTERY_LABELS, mastery } from "../../lib/trainer/schedule";
import { familyOfKind } from "../../lib/trainer/session";
import type { PanelStats } from "../../lib/trainer/stats";
import { FAMILY_LABELS, themeLabel } from "../../lib/trainer/themes";

function Bar({ label, value, average }: { label: string; value: number | null; average: number | null }) {
  return (
    <div className="tbar">
      <span className="tbar__label">{label}</span>
      <span className="tbar__track">
        <span className="tbar__fill" style={{ width: `${Math.max(0, Math.min(100, value ?? 0))}%` }} />
        {average !== null && <span className="tbar__mark" style={{ left: `${average}%` }} title="Your average" />}
      </span>
      <span className="tbar__value">{formatAccuracy(value)}</span>
    </div>
  );
}

/** Accuracy by phase, against the player's overall average. */
export function PhaseBars({ stats }: { stats: PanelStats }) {
  return (
    <div className="tcard">
      <h3 className="tcard__title">Accuracy by phase</h3>
      <Bar label="Opening" value={stats.phases.opening} average={stats.overall} />
      <Bar label="Middlegame" value={stats.phases.middlegame} average={stats.overall} />
      <Bar label="Endgame" value={stats.phases.endgame} average={stats.overall} />
      <p className="muted small">
        {stats.games} {stats.games === 1 ? "game" : "games"} · the mark is your overall accuracy ({formatAccuracy(stats.overall)})
      </p>
    </div>
  );
}

/** The endgame panel: accuracy by family, results by entry, endings reached. */
export function EndgameStats({ stats }: { stats: PanelStats }) {
  const entry = (k: "better" | "equal" | "worse", label: string) => {
    const e = stats.entries[k];
    return (
      <div className="tstat">
        <span className="tstat__num">{e.score === null ? "–" : percent(e.score)}</span>
        <span className="tstat__label">
          {label} <span className="muted">({e.games})</span>
        </span>
      </div>
    );
  };
  return (
    <div className="tcard">
      <h3 className="tcard__title">Endgames</h3>
      {FAMILIES.map((f) => (
        <Bar key={f} label={ENDGAME_LABELS[f]} value={stats.families[f].accuracy} average={stats.phases.endgame} />
      ))}
      <p className="muted small">Moves in decided positions are left out.</p>
      <h4 className="tcard__sub">Score after reaching the endgame</h4>
      <div className="tstats">
        {entry("better", "ahead")}
        {entry("equal", "level")}
        {entry("worse", "behind")}
      </div>
      {stats.types.length > 0 && (
        <>
          <h4 className="tcard__sub">Endings you reached</h4>
          <ul className="tendings">
            {stats.types.slice(0, 8).map((t) => (
              <li key={t.type}>
                <span className="tendings__name">{t.type}</span>
                <span className="muted">
                  {t.games}× · scored {t.score === null ? "–" : percent(t.score)}
                </span>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

const FAMILY_ORDER: ThemeFamily[] = ["endgame", "opening", "strategy", "tactics"];

/** Themes as tiles coloured by mastery, sized by how many drills they have. */
export function ThemeGrid({
  items,
  themes,
  infos,
  onTheme,
}: {
  items: StoredItem[];
  themes: Map<string, ThemeStat>;
  infos: ThemeInfo[];
  onTheme: (theme: string) => void;
}) {
  const rows = FAMILY_ORDER.map((family) => {
    const counts = new Map<string, number>();
    for (const it of items) if (familyOfKind(it.kind) === family) counts.set(it.theme, (counts.get(it.theme) ?? 0) + 1);
    return { family, tiles: [...counts.entries()].sort((a, b) => b[1] - a[1]) };
  }).filter((r) => r.tiles.length > 0);
  if (rows.length === 0) return null;
  const max = Math.max(...rows.flatMap((r) => r.tiles.map((t) => t[1])));
  return (
    <div className="tcard">
      <h3 className="tcard__title">Your themes</h3>
      {rows.map((r) => (
        <div key={r.family} className="tgrid__row">
          <span className="tgrid__family">{FAMILY_LABELS[r.family]}</span>
          <div className="tgrid__tiles">
            {r.tiles.map(([theme, n]) => {
              const m = mastery(themes.get(theme));
              return (
                <button
                  key={theme}
                  type="button"
                  className={`ttile ttile--${m}`}
                  style={{ flexGrow: 1 + (3 * n) / max }}
                  onClick={() => onTheme(theme)}
                  title={`${MASTERY_LABELS[m]} · ${n} ${n === 1 ? "drill" : "drills"}`}
                >
                  <span className="ttile__name">{themeLabel(theme, infos)}</span>
                  <span className="ttile__meta">
                    {n} · {MASTERY_LABELS[m]}
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      ))}
      <p className="tlegend small muted">
        {(["new", "shaky", "learning", "solid"] as const).map((m) => (
          <span key={m} className={`tlegend__dot ttile--${m}`}>
            {MASTERY_LABELS[m]}
          </span>
        ))}
      </p>
    </div>
  );
}
