//! What Handy knows about each meeting beyond its recording: a title, a
//! topic, whether it was online or on site, who took part and notes. Kept
//! in `history.db` next to History, in tables of their own, so a released
//! Handy opening the same database never sees them.

use anyhow::{anyhow, Result};
use log::{info, warn};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS meeting_topics (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL UNIQUE COLLATE NOCASE
    );
    CREATE TABLE IF NOT EXISTS meetings (
        id TEXT PRIMARY KEY,
        file_name TEXT NOT NULL UNIQUE,
        title TEXT NOT NULL DEFAULT '',
        topic_id INTEGER,
        kind TEXT NOT NULL DEFAULT 'online',
        started_at_ms INTEGER NOT NULL,
        duration_ms INTEGER NOT NULL DEFAULT 0,
        participants TEXT NOT NULL DEFAULT '',
        notes TEXT NOT NULL DEFAULT '',
        export_path TEXT
    );
    CREATE TABLE IF NOT EXISTS meeting_settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
";

const MEETING_COLUMNS: &str = "m.id, m.file_name, m.title, m.topic_id, m.kind, m.started_at_ms, \
    m.duration_ms, m.participants, m.notes, h.id AS history_id, h.transcription_text";

const EXPORT_DIR_KEY: &str = "export_dir";
const LAST_KIND_KEY: &str = "last_kind";
const LAST_TOPIC_KEY: &str = "last_topic_id";

const MS_PER_SECOND: i64 = 1000;

/// Where a meeting took place, which decides what is recorded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MeetingKind {
    /// A call: the microphone and the system audio.
    #[default]
    Online,
    /// Everyone in one room: only the microphone.
    OnSite,
}

impl MeetingKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::OnSite => "on_site",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "on_site" => Self::OnSite,
            _ => Self::Online,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct MeetingTopic {
    pub id: i64,
    pub name: String,
}

/// A meeting as the Meeting page lists it.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct Meeting {
    pub id: String,
    /// Its recording, as History knows it.
    pub file_name: String,
    /// Its History entry; `None` once that is deleted.
    pub history_id: Option<i64>,
    pub title: String,
    pub topic_id: Option<i64>,
    pub kind: MeetingKind,
    pub started_at_ms: i64,
    pub duration_ms: i64,
    pub participants: String,
    pub notes: String,
    /// The transcript History shows; empty until it is transcribed.
    pub transcript: String,
}

/// What the user can change about a meeting.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
pub struct MeetingDetails {
    pub title: String,
    pub topic_id: Option<i64>,
    pub kind: MeetingKind,
    pub participants: String,
    pub notes: String,
}

/// How the next meeting starts: from the Meeting page, or the shortcut and
/// tray, which reuse what the page last started with.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct MeetingStartOptions {
    pub kind: MeetingKind,
    pub topic_id: Option<i64>,
    pub title: String,
}

pub struct MeetingRecords {
    db_path: PathBuf,
}

impl MeetingRecords {
    /// The tables in `history.db` at `db_path`, created on first use.
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }

    fn connection(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(conn)
    }

    /// Adds the meeting when it starts, so a crash still leaves its details.
    pub fn insert(
        &self,
        id: &str,
        file_name: &str,
        started_at_ms: i64,
        options: &MeetingStartOptions,
    ) -> Result<()> {
        let conn = self.connection()?;
        insert_with(&conn, id, file_name, started_at_ms, options)
    }

    /// Records how long the meeting ran, adding it if it was never added
    /// (a meeting recorded before meetings had details).
    pub fn finish(&self, id: &str, file_name: &str, duration_ms: i64) -> Result<()> {
        let conn = self.connection()?;
        let started_at_ms = id.parse::<i64>().unwrap_or_default();
        conn.execute(
            "INSERT OR IGNORE INTO meetings (id, file_name, started_at_ms) VALUES (?1, ?2, ?3)",
            params![id, file_name, started_at_ms],
        )?;
        conn.execute(
            "UPDATE meetings SET duration_ms = ?1 WHERE id = ?2",
            params![duration_ms, id],
        )?;
        Ok(())
    }

    /// Gives every meeting in History that has no details yet a row, timed
    /// from its recording. Runs at startup; existing rows are left alone.
    pub fn adopt_existing(&self, recordings_dir: &Path, prefix: &str) -> Result<()> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            "SELECT h.file_name FROM transcription_history h
             LEFT JOIN meetings m ON m.file_name = h.file_name
             WHERE h.file_name LIKE ?1 AND m.id IS NULL",
        )?;
        let orphans: Vec<String> = stmt
            .query_map([format!("{prefix}%")], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for file_name in orphans {
            let Some(id) = file_name
                .strip_prefix(prefix)
                .and_then(|rest| rest.strip_suffix(".wav"))
            else {
                continue;
            };
            let duration_ms = wav_duration_ms(&recordings_dir.join(&file_name)).unwrap_or(0);
            let started_at_ms = id.parse::<i64>().unwrap_or_default();
            conn.execute(
                "INSERT OR IGNORE INTO meetings (id, file_name, started_at_ms, duration_ms)
                 VALUES (?1, ?2, ?3, ?4)",
                params![id, file_name, started_at_ms, duration_ms],
            )?;
            info!("Added details for the earlier meeting {id}");
        }
        Ok(())
    }

    /// Every meeting, newest first.
    pub fn list(&self) -> Result<Vec<Meeting>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {MEETING_COLUMNS} FROM meetings m
             LEFT JOIN transcription_history h ON h.file_name = m.file_name
             ORDER BY m.started_at_ms DESC"
        ))?;
        let meetings = stmt
            .query_map([], map_meeting)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(meetings)
    }

    pub fn get(&self, id: &str) -> Result<Option<Meeting>> {
        let conn = self.connection()?;
        get_with(&conn, id)
    }

    pub fn by_file_name(&self, file_name: &str) -> Result<Option<Meeting>> {
        let conn = self.connection()?;
        let id: Option<String> = conn
            .query_row(
                "SELECT id FROM meetings WHERE file_name = ?1",
                [file_name],
                |row| row.get(0),
            )
            .optional()?;
        match id {
            Some(id) => get_with(&conn, &id),
            None => Ok(None),
        }
    }

    pub fn update(&self, id: &str, details: &MeetingDetails) -> Result<Meeting> {
        let conn = self.connection()?;
        let changed = conn.execute(
            "UPDATE meetings SET title = ?1, topic_id = ?2, kind = ?3, participants = ?4, notes = ?5
             WHERE id = ?6",
            params![
                details.title.trim(),
                details.topic_id,
                details.kind.as_str(),
                details.participants.trim(),
                details.notes,
                id
            ],
        )?;
        if changed == 0 {
            return Err(anyhow!("Meeting {id} not found"));
        }
        get_with(&conn, id)?.ok_or_else(|| anyhow!("Meeting {id} not found"))
    }

    pub fn remove_by_file_name(&self, file_name: &str) -> Result<()> {
        self.connection()?
            .execute("DELETE FROM meetings WHERE file_name = ?1", [file_name])?;
        Ok(())
    }

    pub fn export_path(&self, id: &str) -> Result<Option<PathBuf>> {
        let path: Option<String> = self
            .connection()?
            .query_row(
                "SELECT export_path FROM meetings WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        Ok(path.map(PathBuf::from))
    }

    pub fn set_export_path(&self, id: &str, path: &Path) -> Result<()> {
        self.connection()?.execute(
            "UPDATE meetings SET export_path = ?1 WHERE id = ?2",
            params![path.to_string_lossy(), id],
        )?;
        Ok(())
    }

    pub fn topics(&self) -> Result<Vec<MeetingTopic>> {
        let conn = self.connection()?;
        topics_with(&conn)
    }

    pub fn create_topic(&self, name: &str) -> Result<MeetingTopic> {
        let name = topic_name(name)?;
        let conn = self.connection()?;
        conn.execute("INSERT INTO meeting_topics (name) VALUES (?1)", [&name])
            .map_err(|e| duplicate_or(e, &name))?;
        Ok(MeetingTopic {
            id: conn.last_insert_rowid(),
            name,
        })
    }

    pub fn rename_topic(&self, id: i64, name: &str) -> Result<MeetingTopic> {
        let name = topic_name(name)?;
        let changed = self
            .connection()?
            .execute(
                "UPDATE meeting_topics SET name = ?1 WHERE id = ?2",
                params![&name, id],
            )
            .map_err(|e| duplicate_or(e, &name))?;
        if changed == 0 {
            return Err(anyhow!("Topic {id} not found"));
        }
        Ok(MeetingTopic { id, name })
    }

    /// Deletes the topic; its meetings keep everything else.
    pub fn delete_topic(&self, id: i64) -> Result<()> {
        let mut conn = self.connection()?;
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE meetings SET topic_id = NULL WHERE topic_id = ?1",
            [id],
        )?;
        tx.execute("DELETE FROM meeting_topics WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn topic_name(&self, id: Option<i64>) -> Result<Option<String>> {
        let Some(id) = id else {
            return Ok(None);
        };
        Ok(self
            .connection()?
            .query_row(
                "SELECT name FROM meeting_topics WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn export_dir(&self) -> Result<Option<PathBuf>> {
        Ok(self.setting(EXPORT_DIR_KEY)?.map(PathBuf::from))
    }

    pub fn set_export_dir(&self, dir: Option<&Path>) -> Result<()> {
        self.set_setting(
            EXPORT_DIR_KEY,
            dir.map(|d| d.to_string_lossy().into_owned()),
        )
    }

    /// What the last meeting started from the Meeting page used.
    pub fn last_start_options(&self) -> Result<MeetingStartOptions> {
        let conn = self.connection()?;
        let kind = setting_with(&conn, LAST_KIND_KEY)?
            .map(|k| MeetingKind::parse(&k))
            .unwrap_or_default();
        let topic_id = setting_with(&conn, LAST_TOPIC_KEY)?.and_then(|t| t.parse().ok());
        // A topic deleted since is no topic.
        let topic_id =
            topic_id.filter(|id| topics_with(&conn).is_ok_and(|t| t.iter().any(|t| t.id == *id)));
        Ok(MeetingStartOptions {
            kind,
            topic_id,
            title: String::new(),
        })
    }

    pub fn remember_start_options(&self, options: &MeetingStartOptions) -> Result<()> {
        self.set_setting(LAST_KIND_KEY, Some(options.kind.as_str().to_string()))?;
        self.set_setting(LAST_TOPIC_KEY, options.topic_id.map(|id| id.to_string()))
    }

    fn setting(&self, key: &str) -> Result<Option<String>> {
        setting_with(&self.connection()?, key)
    }

    fn set_setting(&self, key: &str, value: Option<String>) -> Result<()> {
        let conn = self.connection()?;
        match value {
            Some(value) => conn.execute(
                "INSERT INTO meeting_settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?,
            None => conn.execute("DELETE FROM meeting_settings WHERE key = ?1", [key])?,
        };
        Ok(())
    }
}

fn insert_with(
    conn: &Connection,
    id: &str,
    file_name: &str,
    started_at_ms: i64,
    options: &MeetingStartOptions,
) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO meetings (id, file_name, title, topic_id, kind, started_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            id,
            file_name,
            options.title.trim(),
            options.topic_id,
            options.kind.as_str(),
            started_at_ms
        ],
    )?;
    Ok(())
}

fn get_with(conn: &Connection, id: &str) -> Result<Option<Meeting>> {
    Ok(conn
        .query_row(
            &format!(
                "SELECT {MEETING_COLUMNS} FROM meetings m
                 LEFT JOIN transcription_history h ON h.file_name = m.file_name
                 WHERE m.id = ?1"
            ),
            [id],
            map_meeting,
        )
        .optional()?)
}

fn topics_with(conn: &Connection) -> Result<Vec<MeetingTopic>> {
    let mut stmt =
        conn.prepare("SELECT id, name FROM meeting_topics ORDER BY name COLLATE NOCASE")?;
    let topics = stmt
        .query_map([], |row| {
            Ok(MeetingTopic {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(topics)
}

fn setting_with(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT value FROM meeting_settings WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .optional()?)
}

fn map_meeting(row: &rusqlite::Row<'_>) -> rusqlite::Result<Meeting> {
    Ok(Meeting {
        id: row.get("id")?,
        file_name: row.get("file_name")?,
        history_id: row.get("history_id")?,
        title: row.get("title")?,
        topic_id: row.get("topic_id")?,
        kind: MeetingKind::parse(&row.get::<_, String>("kind")?),
        started_at_ms: row.get("started_at_ms")?,
        duration_ms: row.get("duration_ms")?,
        participants: row.get("participants")?,
        notes: row.get("notes")?,
        transcript: row
            .get::<_, Option<String>>("transcription_text")?
            .unwrap_or_default(),
    })
}

fn topic_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(anyhow!("A topic needs a name"));
    }
    Ok(name.to_string())
}

fn duplicate_or(error: rusqlite::Error, name: &str) -> anyhow::Error {
    match error {
        rusqlite::Error::SqliteFailure(e, _)
            if e.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            anyhow!("A topic named \"{name}\" already exists")
        }
        other => other.into(),
    }
}

fn wav_duration_ms(path: &Path) -> Option<i64> {
    match hound::WavReader::open(path) {
        Ok(reader) => {
            let rate = i64::from(reader.spec().sample_rate.max(1));
            Some(i64::from(reader.duration()) * MS_PER_SECOND / rate)
        }
        Err(e) => {
            warn!("Failed to read the length of {}: {e}", path.display());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PREFIX: &str = "handy-meeting-";

    /// A database with History's table, as `HistoryManager` creates it.
    fn records() -> (tempfile::TempDir, MeetingRecords) {
        let dir = tempfile::tempdir().expect("temp dir");
        let db = dir.path().join("history.db");
        Connection::open(&db)
            .expect("open db")
            .execute_batch(
                "CREATE TABLE transcription_history (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    file_name TEXT NOT NULL,
                    timestamp INTEGER NOT NULL,
                    saved BOOLEAN NOT NULL DEFAULT 0,
                    title TEXT NOT NULL,
                    transcription_text TEXT NOT NULL
                );",
            )
            .expect("create history");
        let records = MeetingRecords::new(db);
        (dir, records)
    }

    fn add_history(records: &MeetingRecords, file_name: &str, text: &str) {
        records
            .connection()
            .unwrap()
            .execute(
                "INSERT INTO transcription_history (file_name, timestamp, title, transcription_text)
                 VALUES (?1, 0, '', ?2)",
                params![file_name, text],
            )
            .unwrap();
    }

    #[test]
    fn earlier_meetings_are_adopted_once_and_keep_their_transcript() {
        let (dir, records) = records();
        add_history(
            &records,
            "handy-meeting-1730740000000.wav",
            "[00:00:01] Du: Hallo",
        );
        add_history(&records, "handy-1730740000.wav", "a dictation");

        records.adopt_existing(dir.path(), PREFIX).unwrap();
        records.adopt_existing(dir.path(), PREFIX).unwrap();

        let meetings = records.list().unwrap();
        assert_eq!(meetings.len(), 1);
        let meeting = &meetings[0];
        assert_eq!(meeting.id, "1730740000000");
        assert_eq!(meeting.started_at_ms, 1_730_740_000_000);
        assert_eq!(meeting.kind, MeetingKind::Online);
        assert_eq!(meeting.transcript, "[00:00:01] Du: Hallo");
        assert!(meeting.history_id.is_some());
    }

    #[test]
    fn a_meeting_keeps_what_it_started_with_and_what_was_changed() {
        let (_dir, records) = records();
        let topic = records.create_topic(" Kunde A ").unwrap();
        assert_eq!(topic.name, "Kunde A");
        let options = MeetingStartOptions {
            kind: MeetingKind::OnSite,
            topic_id: Some(topic.id),
            title: "Kickoff".into(),
        };
        records
            .insert("1", "handy-meeting-1.wav", 1, &options)
            .unwrap();
        records.finish("1", "handy-meeting-1.wav", 90_000).unwrap();

        let meeting = records.get("1").unwrap().unwrap();
        assert_eq!(
            (
                meeting.title.as_str(),
                meeting.topic_id,
                meeting.kind,
                meeting.duration_ms
            ),
            ("Kickoff", Some(topic.id), MeetingKind::OnSite, 90_000)
        );
        assert_eq!(meeting.history_id, None);

        let updated = records
            .update(
                "1",
                &MeetingDetails {
                    title: "Kickoff Projekt X".into(),
                    topic_id: None,
                    kind: MeetingKind::Online,
                    participants: "Anna, Ben".into(),
                    notes: "Folgetermin".into(),
                },
            )
            .unwrap();
        assert_eq!(updated.title, "Kickoff Projekt X");
        assert_eq!(updated.participants, "Anna, Ben");
        assert_eq!(updated.topic_id, None);
    }

    #[test]
    fn topics_are_unique_and_deleting_one_keeps_its_meetings() {
        let (_dir, records) = records();
        let topic = records.create_topic("Intern").unwrap();
        assert!(records.create_topic("intern").is_err());
        assert!(records.create_topic("  ").is_err());
        let options = MeetingStartOptions {
            topic_id: Some(topic.id),
            ..Default::default()
        };
        records
            .insert("2", "handy-meeting-2.wav", 2, &options)
            .unwrap();
        records.remember_start_options(&options).unwrap();

        records.delete_topic(topic.id).unwrap();

        assert!(records.topics().unwrap().is_empty());
        assert_eq!(records.get("2").unwrap().unwrap().topic_id, None);
        assert_eq!(records.last_start_options().unwrap().topic_id, None);
    }

    #[test]
    fn the_export_folder_can_be_set_and_cleared() {
        let (_dir, records) = records();
        assert_eq!(records.export_dir().unwrap(), None);
        records
            .set_export_dir(Some(Path::new("/vault/Meetings")))
            .unwrap();
        assert_eq!(
            records.export_dir().unwrap(),
            Some(PathBuf::from("/vault/Meetings"))
        );
        records.set_export_dir(None).unwrap();
        assert_eq!(records.export_dir().unwrap(), None);
    }
}
