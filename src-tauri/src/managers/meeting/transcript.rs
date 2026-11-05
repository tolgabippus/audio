//! A meeting's two tracks, transcribed apart, put back together in time:
//! the microphone is "you", the system audio "the others". Neither track
//! hears the other side's words over the top of its own, so overlapping
//! speech survives, which a mixed recording loses.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::managers::transcription::TimedText;

/// A microphone stretch that overlaps the system audio this much of its
/// length, saying much the same words, is the speakers heard through the
/// microphone, not the user.
const ECHO_MIN_OVERLAP: f64 = 0.5;
/// Share of words two stretches must have in common to count as the same.
const ECHO_MIN_SIMILARITY: f64 = 0.6;
/// The same speaker's stretches this close together read as one turn.
const MAX_TURN_GAP_MS: u64 = 15_000;

const MS_PER_SECOND: u64 = 1000;
const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_PER_HOUR: u64 = 3600;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Speaker {
    /// The microphone.
    You,
    /// The system audio: everyone else in the call.
    Others,
}

/// One speaker's words, from the start of their first stretch to the end of
/// their last.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    pub speaker: Speaker,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

/// What a transcript calls each speaker, in the app's language.
pub struct SpeakerLabels<'a> {
    pub you: &'a str,
    pub others: &'a str,
}

/// The microphone's stretches without those that only echo the system audio.
pub fn drop_echo(microphone: Vec<TimedText>, system: &[TimedText]) -> Vec<TimedText> {
    microphone
        .into_iter()
        .filter(|mic| !is_echo(mic, system))
        .collect()
}

fn is_echo(mic: &TimedText, system: &[TimedText]) -> bool {
    let length = mic.end_ms.saturating_sub(mic.start_ms).max(1);
    let overlapping: Vec<&TimedText> = system
        .iter()
        .filter(|s| s.start_ms < mic.end_ms && mic.start_ms < s.end_ms)
        .collect();
    let overlap: u64 = overlapping
        .iter()
        .map(|s| mic.end_ms.min(s.end_ms) - mic.start_ms.max(s.start_ms))
        .sum();
    if (overlap as f64) < ECHO_MIN_OVERLAP * length as f64 {
        return false;
    }
    let heard: String = overlapping
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    similarity(&mic.text, &heard) >= ECHO_MIN_SIMILARITY
}

/// The share of `a`'s words that `b` also has, ignoring case and punctuation.
fn similarity(a: &str, b: &str) -> f64 {
    let a = words(a);
    if a.is_empty() {
        return 0.0;
    }
    let b = words(b);
    a.iter().filter(|w| b.contains(*w)).count() as f64 / a.len() as f64
}

fn words(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Both tracks' stretches in time order, a speaker's neighbouring stretches
/// joined into one turn.
pub fn merge_turns(microphone: Vec<TimedText>, system: Vec<TimedText>) -> Vec<Turn> {
    let mut stretches: Vec<(Speaker, TimedText)> = microphone
        .into_iter()
        .map(|t| (Speaker::You, t))
        .chain(system.into_iter().map(|t| (Speaker::Others, t)))
        .collect();
    // Stable, so at the same moment the microphone goes first.
    stretches.sort_by_key(|(_, t)| t.start_ms);

    let mut turns: Vec<Turn> = Vec::new();
    for (speaker, stretch) in stretches {
        match turns.last_mut() {
            Some(turn)
                if turn.speaker == speaker && stretch.start_ms <= turn.end_ms + MAX_TURN_GAP_MS =>
            {
                turn.text.push(' ');
                turn.text.push_str(&stretch.text);
                turn.end_ms = turn.end_ms.max(stretch.end_ms);
            }
            _ => turns.push(Turn {
                speaker,
                start_ms: stretch.start_ms,
                end_ms: stretch.end_ms,
                text: stretch.text,
            }),
        }
    }
    turns
}

/// The transcript as History shows it: `[hh:mm:ss] Speaker: text`, a blank
/// line between turns.
pub fn format_transcript(turns: &[Turn], labels: &SpeakerLabels) -> String {
    turns
        .iter()
        .map(|turn| {
            let label = match turn.speaker {
                Speaker::You => labels.you,
                Speaker::Others => labels.others,
            };
            format!("[{}] {}: {}", format_time(turn.start_ms), label, turn.text)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn format_time(ms: u64) -> String {
    let seconds = ms / MS_PER_SECOND;
    format!(
        "{:02}:{:02}:{:02}",
        seconds / SECONDS_PER_HOUR,
        seconds % SECONDS_PER_HOUR / SECONDS_PER_MINUTE,
        seconds % SECONDS_PER_MINUTE
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(start_ms: u64, end_ms: u64, text: &str) -> TimedText {
        TimedText {
            start_ms,
            end_ms,
            text: text.to_string(),
        }
    }

    const LABELS: SpeakerLabels = SpeakerLabels {
        you: "Du",
        others: "Andere",
    };

    #[test]
    fn turns_alternate_in_time_and_a_speakers_stretches_join() {
        let microphone = vec![
            at(1_000, 3_000, "Hallo zusammen."),
            at(4_000, 6_000, "Können wir anfangen?"),
            at(20_000, 22_000, "Danke."),
        ];
        let system = vec![
            at(7_000, 12_000, "Ja, gerne."),
            at(13_000, 15_000, "Los geht's."),
        ];

        let turns = merge_turns(microphone, system);

        let summary: Vec<(Speaker, u64, &str)> = turns
            .iter()
            .map(|t| (t.speaker, t.start_ms, t.text.as_str()))
            .collect();
        assert_eq!(
            summary,
            vec![
                (Speaker::You, 1_000, "Hallo zusammen. Können wir anfangen?"),
                (Speaker::Others, 7_000, "Ja, gerne. Los geht's."),
                (Speaker::You, 20_000, "Danke."),
            ]
        );
        assert_eq!(turns[1].end_ms, 15_000);
    }

    #[test]
    fn a_long_pause_starts_a_new_turn_for_the_same_speaker() {
        let turns = merge_turns(
            vec![
                at(0, 1_000, "Erstens."),
                at(1_000 + MAX_TURN_GAP_MS + 1, 20_000, "Zweitens."),
            ],
            Vec::new(),
        );
        assert_eq!(turns.len(), 2);
    }

    #[test]
    fn talking_over_each_other_keeps_both_sides() {
        let turns = merge_turns(
            vec![at(5_000, 9_000, "Ich meine eigentlich etwas anderes.")],
            vec![at(4_000, 10_000, "Das Angebot geht bis Freitag raus.")],
        );
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].speaker, Speaker::Others);
        assert_eq!(turns[1].speaker, Speaker::You);
    }

    #[test]
    fn the_speakers_heard_through_the_microphone_are_dropped() {
        let system = vec![at(
            10_000,
            15_000,
            "Inference happens every single time someone sends a message.",
        )];
        let microphone = vec![
            // The same words, a little later and slightly misheard: echo.
            at(
                10_200,
                15_100,
                "inference happens every single time someone sends message",
            ),
            // Real speech while the others talk: kept.
            at(11_000, 14_000, "Moment, kurze Frage dazu."),
            // The same words but not at the same time: kept.
            at(
                40_000,
                44_000,
                "Inference happens every single time someone sends a message.",
            ),
        ];

        let kept = drop_echo(microphone, &system);

        let texts: Vec<&str> = kept.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(
            texts,
            vec![
                "Moment, kurze Frage dazu.",
                "Inference happens every single time someone sends a message."
            ]
        );
    }

    #[test]
    fn the_transcript_shows_time_and_speaker_for_each_turn() {
        let turns = vec![
            Turn {
                speaker: Speaker::You,
                start_ms: 72_500,
                end_ms: 75_000,
                text: "Passt das?".into(),
            },
            Turn {
                speaker: Speaker::Others,
                start_ms: 3_725_000,
                end_ms: 3_730_000,
                text: "Ja.".into(),
            },
        ];
        assert_eq!(
            format_transcript(&turns, &LABELS),
            "[00:01:12] Du: Passt das?\n\n[01:02:05] Andere: Ja."
        );
    }
}
