// Endgame concept cards (docs/trainer.md §8.3): short lessons, each with a few positions to solve.
// The positions were composed for this project; each answer list is every move that keeps the
// result (checked against the Lichess 7-piece tablebase), or every mate in one.
import type { EndgameConcept } from "../../api/generated/EndgameConcept";

export interface CardPosition {
  fen: string;
  /** What to do, without naming the move. */
  task: string;
  /** Every move (UCI) that solves it. */
  solutions: string[];
  /** Said after a solution. */
  why: string;
}

export interface ConceptCard {
  id: string;
  title: string;
  /** The idea in two or three sentences. */
  idea: string;
  /** The drill concepts and endings this card helps with. */
  concepts: EndgameConcept[];
  endings: string[];
  positions: CardPosition[];
}

export const CARDS: ConceptCard[] = [
  {
    id: "opposition",
    title: "The opposition",
    idea:
      "When the kings face each other with one square between them, the side that does not have to move holds the line. " +
      "In king and pawn endings, getting your king in front of the pawn with the opposition is usually the whole game.",
    concepts: ["opposition", "king_activity"],
    endings: ["KPvK", "KPPvK"],
    positions: [
      {
        fen: "8/8/8/3k4/8/8/3PK3/8 w - - 0 1",
        task: "White to move and win. Only one king move does it.",
        solutions: ["e2d3"],
        why: "Kd3 takes the opposition in front of the pawn: the black king has to give way.",
      },
      {
        fen: "8/8/4k3/8/8/4K3/4P3/8 w - - 0 1",
        task: "White to move and win. Walk the king up first.",
        solutions: ["e3e4", "e3d4", "e3f4"],
        why: "The king goes in front of its pawn. Pushing the pawn first, or stepping back, only draws.",
      },
      {
        fen: "4k3/8/4K3/4P3/8/8/8/8 w - - 0 1",
        task: "White to move and win. Keep the black king out.",
        solutions: ["e6d6", "e6f6"],
        why: "Stepping to the side keeps control of the squares in front of the pawn; the pawn then walks in.",
      },
    ],
  },
  {
    id: "square",
    title: "The square of the pawn",
    idea:
      "Draw a square from a passed pawn to its promotion rank. If the defending king can step into that square, it catches the pawn. " +
      "Count it out before you race.",
    concepts: ["passed_pawn_push", "king_activity"],
    endings: ["KPvK"],
    positions: [
      {
        fen: "8/8/8/p7/8/8/8/5K1k w - - 0 1",
        task: "White to move and draw. Can your king catch the pawn?",
        solutions: ["f1e1", "f1e2"],
        why: "Toward the pawn: the king steps into its square and catches it in time.",
      },
      {
        fen: "8/8/8/p7/8/8/8/4K2k w - - 0 1",
        task: "White to move and draw.",
        solutions: ["e1d1", "e1d2", "e1e2"],
        why: "Any step toward the pawn's path keeps the king inside the square.",
      },
    ],
  },
  {
    id: "basic-mates",
    title: "Basic mates",
    idea:
      "A queen or a rook mates a bare king on the edge of the board, with its own king close by. " +
      "Drive the king to the edge, bring your king up, then give the last check.",
    concepts: ["mating_technique"],
    endings: ["KQvK", "KRvK"],
    positions: [
      {
        fen: "7k/8/6K1/8/8/8/8/R7 w - - 0 1",
        task: "White to move: mate in one.",
        solutions: ["a1a8"],
        why: "The rook checks along the back rank and the white king covers every escape square.",
      },
      {
        fen: "6k1/8/6K1/8/8/8/8/1Q6 w - - 0 1",
        task: "White to move: mate in one.",
        solutions: ["b1b8"],
        why: "The queen takes the whole back rank; the white king guards the squares in front.",
      },
    ],
  },
];

/** The cards that help with a drill's concept or ending. */
export function cardsFor(concept: EndgameConcept | null, ending: string | null): ConceptCard[] {
  return CARDS.filter((c) => (concept && c.concepts.includes(concept)) || (ending && c.endings.includes(ending)));
}
