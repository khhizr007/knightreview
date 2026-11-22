//! The trainer's sentences (`text/en.toml`): goals before the move, hints, and replies after it.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::grade::TrainGrade;
use super::{EndgameConcept, EndgameFamily, TrainBucket, TrainItem, TrainKind, TrainTheme};

#[derive(Debug, Deserialize)]
pub struct Corpus {
    pub goal: HashMap<String, Vec<String>>,
    pub hint: HashMap<String, String>,
    pub concept: HashMap<String, String>,
    pub reply: Replies,
    pub correct_theme: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct Replies {
    pub correct: Vec<String>,
    pub not_incorrect: Vec<String>,
    pub incorrect: Vec<String>,
    pub same_as_game: Vec<String>,
    pub fallback: String,
}

pub fn corpus() -> &'static Corpus {
    static CORPUS: OnceLock<Corpus> = OnceLock::new();
    CORPUS
        .get_or_init(|| toml::from_str(include_str!("text/en.toml")).expect("trainer text parses"))
}

/// The serialised name of a value (`TrainTheme::FreePiece` → `free_piece`).
pub fn key<T: Serialize>(value: T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(s)) => s,
        _ => String::new(),
    }
}

fn pick(options: &[String], seed: u64) -> String {
    if options.is_empty() {
        return String::new();
    }
    options[(seed % options.len() as u64) as usize].clone()
}

/// A stable number from a drill id, to pick among wordings.
pub fn seed(id: &str) -> u64 {
    id.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn family_key(family: EndgameFamily) -> String {
    format!("endgame_{}", key(family))
}

/// The goal said before the drill's move.
pub fn goal(
    kind: TrainKind,
    bucket: Option<TrainBucket>,
    theme: TrainTheme,
    family: Option<EndgameFamily>,
    seed: u64,
) -> String {
    let c = corpus();
    let wanted = match kind {
        TrainKind::Opening => vec!["opening".to_owned()],
        TrainKind::Strategy => vec!["strategy".to_owned()],
        TrainKind::Endgame => family
            .map(family_key)
            .into_iter()
            .chain(["endgame_other".to_owned()])
            .collect(),
        TrainKind::Miss => match bucket {
            Some(TrainBucket::Brilliant) => vec!["miss_brilliant".to_owned(), "miss".to_owned()],
            Some(TrainBucket::FreePieces) => vec!["miss_free_pieces".to_owned(), "miss".to_owned()],
            _ => vec!["miss".to_owned()],
        },
        TrainKind::Blunder if theme == TrainTheme::Mate => {
            vec!["blunder_mate".to_owned(), "blunder".to_owned()]
        }
        TrainKind::Blunder => vec!["blunder".to_owned()],
    };
    wanted
        .iter()
        .find_map(|k| c.goal.get(k))
        .map(|options| pick(options, seed))
        .unwrap_or_default()
}

/// The idea in one sentence: the endgame concept when there is one, else the theme.
pub fn hint(theme: TrainTheme, concept: Option<EndgameConcept>) -> String {
    let c = corpus();
    concept
        .and_then(|k| c.concept.get(&key(k)))
        .or_else(|| c.hint.get(&key(theme)))
        .cloned()
        .unwrap_or_default()
}

/// What the coach says after the move, by grade. `on_theme` says the move carried the drill's
/// own idea (a "correct" reply then names it).
pub fn reply(item: &TrainItem, grade: TrainGrade, on_theme: bool, engine: bool) -> Vec<String> {
    let c = corpus();
    let s = seed(&item.id);
    let mut out = vec![match grade {
        TrainGrade::Correct => pick(&c.reply.correct, s),
        TrainGrade::NotIncorrect => pick(&c.reply.not_incorrect, s),
        TrainGrade::Incorrect => pick(&c.reply.incorrect, s),
        TrainGrade::SameAsGame => pick(&c.reply.same_as_game, s),
    }];
    if grade == TrainGrade::Correct
        && on_theme
        && let Some(t) = c.correct_theme.get(&key(item.theme))
    {
        out.push(t.clone());
    }
    if !engine {
        out.push(c.reply.fallback.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::train::themes;

    #[test]
    fn every_theme_has_a_hint_and_every_kind_a_goal() {
        let c = corpus();
        for t in themes::all() {
            assert!(
                c.hint.contains_key(&key(t.theme)),
                "no hint for {:?}",
                t.theme
            );
        }
        for kind in [
            TrainKind::Opening,
            TrainKind::Strategy,
            TrainKind::Endgame,
            TrainKind::Miss,
            TrainKind::Blunder,
        ] {
            assert!(!goal(kind, None, TrainTheme::Fork, None, 0).is_empty());
        }
        for f in [
            EndgameFamily::Queen,
            EndgameFamily::RookAndPawn,
            EndgameFamily::KingAndPawn,
            EndgameFamily::Other,
        ] {
            assert!(c.goal.contains_key(&family_key(f)));
        }
        for k in [
            EndgameConcept::Opposition,
            EndgameConcept::KingActivity,
            EndgameConcept::PassedPawnPush,
            EndgameConcept::RookBehindPasser,
            EndgameConcept::Blockade,
            EndgameConcept::TradeIntoPawnEnding,
            EndgameConcept::MatingTechnique,
            EndgameConcept::Promotion,
        ] {
            assert!(c.concept.contains_key(&key(k)), "no text for {k:?}");
        }
    }

    #[test]
    fn goals_never_name_a_square() {
        let c = corpus();
        let square = |s: &str| {
            s.as_bytes()
                .windows(2)
                .any(|w| (b'a'..=b'h').contains(&w[0]) && (b'1'..=b'8').contains(&w[1]))
        };
        for options in c.goal.values() {
            for g in options {
                assert!(!square(g), "goal names a square: {g}");
            }
        }
    }
}
