// Theme names for the trainer screens. The server's table (GET /api/train/themes) is the source;
// until it arrives, a name is made from the theme key.
import { api } from "../../api/client";
import type { ThemeInfo } from "../../api/generated/ThemeInfo";
import type { TrainKind } from "../../api/generated/TrainKind";

let table: Promise<ThemeInfo[]> | null = null;

export function loadThemes(): Promise<ThemeInfo[]> {
  table ??= api.trainThemes().catch(() => {
    table = null;
    return [];
  });
  return table;
}

/** "free_piece" → "Free piece". */
export function themeLabel(theme: string, infos: ThemeInfo[] = []): string {
  const known = infos.find((t) => t.theme === theme);
  if (known) return known.name;
  const words = theme.replace(/^endgame_/, "").replace(/_/g, " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

export const KIND_LABELS: Record<TrainKind, string> = {
  opening: "Opening",
  strategy: "Middlegame",
  endgame: "Endgame",
  miss: "Missed chance",
  blunder: "Blunder",
};

export const FAMILY_LABELS = {
  endgame: "Endgames",
  opening: "Openings",
  strategy: "Middlegame plans",
  tactics: "Tactics",
} as const;
