// The trainer's local store (docs/trainer.md §11): drills, attempts, the review schedule, theme
// progress and per-game stats live in the browser (IndexedDB), never on the server. A memory store
// stands in where IndexedDB is unavailable (private windows, tests).
import type { TrainGrade } from "../../api/generated/TrainGrade";
import type { TrainItem } from "../../api/generated/TrainItem";
import type { TrainTheme } from "../../api/generated/TrainTheme";
import type { GameStats } from "./stats";

export const DB_NAME = "knightreview-trainer";
export const DB_VERSION = 1;

export interface StoredItem extends TrainItem {
  added_at: number;
  /** The game the drill comes from (see `gameKey`). */
  game_key: string;
}

export interface Attempt {
  id?: number;
  item_id: string;
  at: number;
  grade: TrainGrade;
  /** Hints used before the move: 0 none, 1 the piece, 2 the idea, 3 the move. */
  hint: 0 | 1 | 2 | 3;
  retry: boolean;
  ms: number;
}

export interface ScheduleEntry {
  item_id: string;
  /** Leitner box 0–5. */
  box: number;
  due_at: number;
  retired: boolean;
}

export interface ThemeStat {
  theme: TrainTheme;
  /** Exponentially weighted success rate (0–1). */
  ewma: number;
  count: number;
  last_at: number;
}

export interface StoredGame extends GameStats {
  game_key: string;
  added_at: number;
}

export interface MetaEntry {
  key: string;
  value: unknown;
}

/** Each store and its key field. */
export const STORES = {
  items: "id",
  attempts: "id",
  schedule: "item_id",
  themes: "theme",
  games: "game_key",
  meta: "key",
  tablebase: "fen",
} as const;

export type StoreName = keyof typeof STORES;

export interface TablebaseEntry {
  fen: string;
  category: string;
  at: number;
}

interface Rows {
  items: StoredItem;
  attempts: Attempt;
  schedule: ScheduleEntry;
  themes: ThemeStat;
  games: StoredGame;
  meta: MetaEntry;
  tablebase: TablebaseEntry;
}

/** The store's operations: the same over IndexedDB and memory. */
export interface TrainerDb {
  all<S extends StoreName>(store: S): Promise<Rows[S][]>;
  get<S extends StoreName>(store: S, key: string): Promise<Rows[S] | undefined>;
  put<S extends StoreName>(store: S, rows: Rows[S][]): Promise<void>;
  clear(): Promise<void>;
}

function keyOf<S extends StoreName>(store: S, row: Rows[S]): string | number | undefined {
  return (row as unknown as Record<string, string | number | undefined>)[STORES[store]];
}

/** A store held in memory: for tests, and where IndexedDB is missing. */
export function memoryDb(): TrainerDb {
  const data = new Map<StoreName, Map<string, unknown>>();
  let nextAttempt = 1;
  const table = (s: StoreName) => {
    let t = data.get(s);
    if (!t) data.set(s, (t = new Map()));
    return t;
  };
  return {
    async all(store) {
      return [...table(store).values()].map((v) => structuredClone(v)) as never;
    },
    async get(store, key) {
      const v = table(store).get(String(key));
      return (v === undefined ? undefined : structuredClone(v)) as never;
    },
    async put(store, rows) {
      for (const row of rows) {
        const copy = structuredClone(row) as unknown as Record<string, unknown>;
        if (store === "attempts" && copy.id === undefined) copy.id = nextAttempt++;
        table(store).set(String(keyOf(store, copy as never)), copy);
      }
    },
    async clear() {
      data.clear();
    },
  };
}

function request<T>(r: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => reject(r.error);
  });
}

/** Opens the IndexedDB store; rejects where IndexedDB is not available. */
export async function indexedDb(factory: IDBFactory = globalThis.indexedDB): Promise<TrainerDb> {
  if (!factory) throw new Error("IndexedDB is not available");
  const open = factory.open(DB_NAME, DB_VERSION);
  open.onupgradeneeded = () => {
    const db = open.result;
    for (const [name, key] of Object.entries(STORES)) {
      if (db.objectStoreNames.contains(name)) continue;
      const os = db.createObjectStore(name, { keyPath: key, autoIncrement: name === "attempts" });
      if (name === "attempts") {
        os.createIndex("item_id", "item_id");
        os.createIndex("at", "at");
      }
    }
  };
  const db = await request(open);
  const tx = (store: StoreName, mode: IDBTransactionMode) => db.transaction(store, mode).objectStore(store);
  return {
    all: (store) => request(tx(store, "readonly").getAll()) as never,
    get: (store, key) => request(tx(store, "readonly").get(key)) as never,
    async put(store, rows) {
      if (rows.length === 0) return;
      const t = db.transaction(store, "readwrite");
      const os = t.objectStore(store);
      for (const row of rows) os.put(row);
      await new Promise<void>((resolve, reject) => {
        t.oncomplete = () => resolve();
        t.onerror = () => reject(t.error);
        t.onabort = () => reject(t.error);
      });
    },
    async clear() {
      for (const name of Object.keys(STORES) as StoreName[]) await request(tx(name, "readwrite").clear());
    },
  };
}

let shared: Promise<TrainerDb> | null = null;

/** The page's store: IndexedDB when it opens, else memory (progress is then lost on reload). */
export function trainerDb(): Promise<TrainerDb> {
  shared ??= indexedDb().catch(() => memoryDb());
  return shared;
}

/** Replaces the page's store (tests). */
export function setTrainerDb(db: TrainerDb | null): void {
  shared = db ? Promise.resolve(db) : null;
}

/** The export file: every store's rows. */
export interface TrainerExport {
  format: "knightreview-trainer";
  version: number;
  exported_at: number;
  stores: Partial<Record<StoreName, unknown[]>>;
}

export async function exportAll(db: TrainerDb, now = Date.now()): Promise<TrainerExport> {
  const stores: TrainerExport["stores"] = {};
  for (const name of Object.keys(STORES) as StoreName[]) {
    if (name === "tablebase") continue;
    stores[name] = await db.all(name);
  }
  return { format: "knightreview-trainer", version: DB_VERSION, exported_at: now, stores };
}

/** Merges an export into the store by key (rows already present are replaced). */
export async function importAll(db: TrainerDb, data: unknown): Promise<number> {
  const file = data as Partial<TrainerExport> | null;
  if (!file || file.format !== "knightreview-trainer" || typeof file.stores !== "object" || !file.stores) {
    throw new Error("This is not a trainer export file.");
  }
  let n = 0;
  for (const name of Object.keys(STORES) as StoreName[]) {
    const rows = file.stores[name];
    if (!Array.isArray(rows)) continue;
    const valid = rows.filter((r) => r && typeof r === "object" && keyOf(name, r as never) !== undefined);
    // Attempts keep their history but get fresh ids, so imports never overwrite local attempts.
    const put =
      name === "attempts"
        ? valid.map((r) => {
            const copy = { ...(r as Attempt) };
            delete copy.id;
            return copy;
          })
        : valid;
    await db.put(name, put as never);
    n += put.length;
  }
  return n;
}
