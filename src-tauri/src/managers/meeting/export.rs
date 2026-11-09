//! A meeting as a Markdown note, for a folder such as an Obsidian vault:
//! details in the frontmatter, then the notes and the transcript. One
//! subfolder per topic.

use chrono::{DateTime, Local};
use std::path::{Path, PathBuf};

use super::records::{Meeting, MeetingKind};
use super::transcript::{format_time, Speaker, Turn};

/// Longest file or folder name the export writes, in characters.
const MAX_NAME_CHARS: usize = 120;

/// The words a note uses, in the app's language.
pub struct ExportLabels<'a> {
    pub you: &'a str,
    pub others: &'a str,
    /// The title of a meeting that has none.
    pub untitled: &'a str,
    /// The folder of meetings without a topic.
    pub no_topic: &'a str,
    pub notes: &'a str,
    pub transcript: &'a str,
}

fn local_start(meeting: &Meeting) -> Option<DateTime<Local>> {
    DateTime::from_timestamp_millis(meeting.started_at_ms).map(|utc| utc.with_timezone(&Local))
}

/// The meeting's title, or the placeholder.
pub fn display_title<'a>(meeting: &'a Meeting, labels: &'a ExportLabels) -> &'a str {
    let title = meeting.title.trim();
    if title.is_empty() {
        labels.untitled
    } else {
        title
    }
}

/// `<dir>/<topic>/<YYYY-MM-DD HH-MM> <title>.md`.
pub fn note_path(
    dir: &Path,
    meeting: &Meeting,
    topic: Option<&str>,
    labels: &ExportLabels,
) -> PathBuf {
    let folder = safe_name(topic.unwrap_or(labels.no_topic));
    let started = local_start(meeting)
        .map(|t| t.format("%Y-%m-%d %H-%M").to_string())
        .unwrap_or_else(|| meeting.id.clone());
    let name = safe_name(&format!("{started} {}", display_title(meeting, labels)));
    dir.join(folder).join(format!("{name}.md"))
}

/// A name every file system accepts: no separators or reserved characters,
/// no leading dots, not too long.
fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#' | '^' | '[' | ']' => '-',
            c if c.is_control() => ' ',
            c => c,
        })
        .take(MAX_NAME_CHARS)
        .collect();
    let trimmed = cleaned.trim().trim_start_matches('.').trim();
    if trimmed.is_empty() {
        "-".to_string()
    } else {
        trimmed.to_string()
    }
}

/// A YAML string that survives any text.
fn yaml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

fn participants(meeting: &Meeting) -> Vec<&str> {
    meeting
        .participants
        .split([',', ';', '\n'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect()
}

/// The note. `turns` is the transcript by speaker; without it (a meeting
/// transcribed before speakers were told apart) the plain transcript is used.
pub fn render_note(
    meeting: &Meeting,
    topic: Option<&str>,
    turns: Option<&[Turn]>,
    labels: &ExportLabels,
) -> String {
    let title = display_title(meeting, labels);
    let mut out = String::from("---\n");
    out.push_str(&format!("title: {}\n", yaml_string(title)));
    if let Some(start) = local_start(meeting) {
        out.push_str(&format!("date: {}\n", start.format("%Y-%m-%dT%H:%M")));
    }
    out.push_str(&format!(
        "duration_minutes: {}\n",
        (meeting.duration_ms + 30_000) / 60_000
    ));
    let kind = match meeting.kind {
        MeetingKind::Online => "online",
        MeetingKind::OnSite => "on_site",
    };
    out.push_str(&format!("type: {kind}\n"));
    if let Some(topic) = topic {
        out.push_str(&format!("topic: {}\n", yaml_string(topic)));
    }
    let people = participants(meeting);
    if !people.is_empty() {
        out.push_str("participants:\n");
        for person in people {
            out.push_str(&format!("  - {}\n", yaml_string(person)));
        }
    }
    out.push_str("tags:\n  - meeting\n");
    out.push_str(&format!("handy_id: {}\n", yaml_string(&meeting.id)));
    out.push_str("---\n\n");

    out.push_str(&format!("# {title}\n\n"));
    if !meeting.notes.trim().is_empty() {
        out.push_str(&format!(
            "## {}\n\n{}\n\n",
            labels.notes,
            meeting.notes.trim()
        ));
    }
    out.push_str(&format!("## {}\n\n", labels.transcript));
    match turns {
        Some(turns) if !turns.is_empty() => {
            for turn in turns {
                let speaker = match turn.speaker {
                    Speaker::You => labels.you,
                    Speaker::Others => labels.others,
                };
                out.push_str(&format!(
                    "**[{}] {}:** {}\n\n",
                    format_time(turn.start_ms),
                    speaker,
                    turn.text.trim()
                ));
            }
        }
        _ => {
            let text = meeting.transcript.trim();
            if !text.is_empty() {
                out.push_str(text);
                out.push_str("\n\n");
            }
        }
    }
    let trimmed_len = out.trim_end().len();
    out.truncate(trimmed_len);
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const LABELS: ExportLabels = ExportLabels {
        you: "Du",
        others: "Andere",
        untitled: "Meeting",
        no_topic: "Ohne Thema",
        notes: "Notizen",
        transcript: "Transkript",
    };

    fn meeting() -> Meeting {
        Meeting {
            id: "1730740000000".into(),
            file_name: "handy-meeting-1730740000000.wav".into(),
            history_id: Some(1),
            title: "Kickoff: Projekt \"X\"".into(),
            topic_id: Some(1),
            kind: MeetingKind::Online,
            started_at_ms: 1_730_740_000_000,
            duration_ms: 125_000,
            participants: "Anna, Ben;".into(),
            notes: "Angebot bis Freitag.".into(),
            transcript: "plain".into(),
        }
    }

    #[test]
    fn a_note_has_frontmatter_notes_and_speakers() {
        let turns = vec![
            Turn {
                speaker: Speaker::Others,
                start_ms: 3_000,
                end_ms: 5_000,
                text: "Hallo zusammen.".into(),
            },
            Turn {
                speaker: Speaker::You,
                start_ms: 72_000,
                end_ms: 75_000,
                text: "Passt.".into(),
            },
        ];
        let note = render_note(&meeting(), Some("Kunde A"), Some(&turns), &LABELS);

        assert!(note.starts_with("---\ntitle: \"Kickoff: Projekt \\\"X\\\"\"\n"));
        assert!(note.contains("duration_minutes: 2\n"));
        assert!(note.contains("type: online\n"));
        assert!(note.contains("topic: \"Kunde A\"\n"));
        assert!(note.contains("participants:\n  - \"Anna\"\n  - \"Ben\"\n"));
        assert!(note.contains("## Notizen\n\nAngebot bis Freitag.\n"));
        assert!(
            note.contains("**[00:00:03] Andere:** Hallo zusammen.\n\n**[00:01:12] Du:** Passt.\n")
        );
        assert!(!note.contains("plain"));
        assert!(note.ends_with("Passt.\n"));
    }

    #[test]
    fn without_turns_the_plain_transcript_is_used() {
        let mut meeting = meeting();
        meeting.notes = String::new();
        meeting.participants = String::new();
        let note = render_note(&meeting, None, None, &LABELS);
        assert!(note.ends_with("## Transkript\n\nplain\n"));
        assert!(!note.contains("topic:"));
        assert!(!note.contains("participants:"));
        assert!(!note.contains("## Notizen"));
    }

    #[test]
    fn notes_go_in_a_folder_per_topic_with_safe_names() {
        let path = note_path(
            Path::new("/vault"),
            &meeting(),
            Some("Kunden/Intern"),
            &LABELS,
        );
        let folder = path.parent().unwrap();
        assert_eq!(folder, Path::new("/vault/Kunden-Intern"));
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.ends_with(" Kickoff- Projekt -X-.md"), "{name}");
        assert!(name.starts_with("2024-11-0"), "{name}");

        let mut untitled = meeting();
        untitled.title = "  ".into();
        let path = note_path(Path::new("/vault"), &untitled, None, &LABELS);
        assert!(path.starts_with("/vault/Ohne Thema"));
        assert!(path.to_string_lossy().ends_with(" Meeting.md"));
    }

    #[test]
    fn unsafe_names_are_cleaned() {
        assert_eq!(safe_name("../../etc"), "-..-etc");
        assert_eq!(safe_name("  "), "-");
        assert_eq!(safe_name(&"a".repeat(500)).chars().count(), MAX_NAME_CHARS);
    }
}
