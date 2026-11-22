import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { TrainItem } from "../../api/generated/TrainItem";
import type { EngineController } from "../../lib/engine/controller";
import { memoryDb } from "../../lib/trainer/db";
import { Drill } from "./DrillScreen";

const start = "4k3/8/8/8/8/8/8/4K2R w K - 0 1";

const drill: TrainItem & { added_at: number; game_key: string } = {
  id: "a",
  kind: "endgame",
  theme: "endgame_rook_and_pawn",
  bucket: null,
  user_side: "white",
  pre_pitch_fen: "3k4/8/8/8/8/8/8/4K2R b K - 0 1",
  pitch_move: { san: "Ke8", uci: "d8e8", fen: start },
  start_fen: start,
  correct_move: { san: "Rh8+", uci: "h1h8", fen: start },
  played_move: { san: "Ke2", uci: "e1e2", fen: start },
  best_line: ["h1h8"],
  played_line: ["e1e2"],
  lines: [{ eval: { kind: "cp", cp: 500 }, depth: 12, pv: ["h1h8"] }],
  classification: "mistake",
  miss_kind: null,
  phase: "endgame",
  is_critical: false,
  position_type: null,
  eco: null,
  opening_name: null,
  endgame_family: "other",
  endgame_concept: null,
  endgame_type: "KRvK",
  play_out: null,
  ep_lost: 0.2,
  context: { rating: 1500, highest_elo: 1500, previous_ep_lost: 0, previous_erred: false },
  goal: "Endgame. Find the move that keeps your result.",
  hint: "Activate your king and use every piece.",
  source: {
    ply: 40,
    move_number: 20,
    white: "me",
    black: "Rival",
    opponent: "Rival",
    date: "2026.11.01",
    site: null,
    event: null,
    result: null,
    time_control: null,
  },
  generator_version: 1,
  added_at: 0,
  game_key: "g",
};

afterEach(() => vi.useRealTimers());

describe("Drill", () => {
  it("plays the pitch move, states the goal and gives hints in order", async () => {
    vi.useFakeTimers();
    const engine = () => ({ analyze: vi.fn(), stop: vi.fn() }) as unknown as EngineController;
    render(<Drill item={drill} db={memoryDb()} themes={[]} engine={engine} onDone={() => {}} position={{ index: 0, total: 3 }} />);
    expect(screen.getByText("Your opponent plays Ke8.")).toBeTruthy();
    expect(screen.getByText(/against Rival, move 20/)).toBeTruthy();
    await act(async () => {
      vi.advanceTimersByTime(600);
    });
    expect(screen.getByText(drill.goal)).toBeTruthy();
    expect(screen.queryByText(drill.hint)).toBeNull();
    fireEvent.click(screen.getByText("Hint: the piece"));
    fireEvent.click(screen.getByText("Hint: the idea"));
    expect(screen.getByText(drill.hint)).toBeTruthy();
    expect(screen.getByText("Show the move")).toBeTruthy();
    expect(screen.getByText("1 / 3")).toBeTruthy();
  });
});
