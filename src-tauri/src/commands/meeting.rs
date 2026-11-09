use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, State};

use crate::managers::history::HistoryEntry;
use crate::managers::meeting::{
    self, Meeting, MeetingDetails, MeetingManager, MeetingStartOptions, MeetingState, MeetingTopic,
    Turn,
};

fn text<T>(result: anyhow::Result<T>) -> Result<T, String> {
    result.map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn get_meeting_state(meetings: State<'_, Arc<MeetingManager>>) -> MeetingState {
    meetings.state()
}

/// How the last meeting started from the Meeting page; the page's defaults.
#[tauri::command]
#[specta::specta]
pub fn get_last_meeting_options(
    meetings: State<'_, Arc<MeetingManager>>,
) -> Result<MeetingStartOptions, String> {
    text(meetings.records().last_start_options())
}

/// Starts recording: the microphone, and online also the system audio.
#[tauri::command]
#[specta::specta]
pub async fn start_meeting(
    meetings: State<'_, Arc<MeetingManager>>,
    options: MeetingStartOptions,
) -> Result<MeetingState, String> {
    let meetings = Arc::clone(&meetings);
    tauri::async_runtime::spawn_blocking(move || meetings.start(options))
        .await
        .map_err(|e| format!("Failed to start the meeting: {}", e))?
        .map_err(|e| e.to_string())
}

/// Stops recording and queues the meeting's transcription in History.
#[tauri::command]
#[specta::specta]
pub async fn stop_meeting(
    meetings: State<'_, Arc<MeetingManager>>,
) -> Result<HistoryEntry, String> {
    let meetings = Arc::clone(&meetings);
    tauri::async_runtime::spawn_blocking(move || meetings.stop())
        .await
        .map_err(|e| format!("Failed to stop the meeting: {}", e))?
        .map_err(|e| e.to_string())
}

/// Every recorded meeting, newest first.
#[tauri::command]
#[specta::specta]
pub fn list_meetings(meetings: State<'_, Arc<MeetingManager>>) -> Result<Vec<Meeting>, String> {
    text(meetings.records().list())
}

/// The meeting's transcript by speaker; `None` if it has none yet, or was
/// transcribed before speakers were told apart.
#[tauri::command]
#[specta::specta]
pub fn get_meeting_turns(
    app: AppHandle,
    meetings: State<'_, Arc<MeetingManager>>,
    id: String,
) -> Result<Option<Vec<Turn>>, String> {
    let Some(found) = text(meetings.records().get(&id))? else {
        return Ok(None);
    };
    text(meeting::turns(&app, &found))
}

#[tauri::command]
#[specta::specta]
pub fn update_meeting(
    meetings: State<'_, Arc<MeetingManager>>,
    id: String,
    details: MeetingDetails,
) -> Result<Meeting, String> {
    text(meetings.update_details(&id, &details))
}

#[tauri::command]
#[specta::specta]
pub fn list_meeting_topics(
    meetings: State<'_, Arc<MeetingManager>>,
) -> Result<Vec<MeetingTopic>, String> {
    text(meetings.records().topics())
}

#[tauri::command]
#[specta::specta]
pub fn create_meeting_topic(
    meetings: State<'_, Arc<MeetingManager>>,
    name: String,
) -> Result<MeetingTopic, String> {
    text(meetings.records().create_topic(&name))
}

#[tauri::command]
#[specta::specta]
pub fn rename_meeting_topic(
    meetings: State<'_, Arc<MeetingManager>>,
    id: i64,
    name: String,
) -> Result<MeetingTopic, String> {
    text(meetings.records().rename_topic(id, &name))
}

/// Deletes the topic. Its meetings stay, without a topic. Exported notes
/// stay where they are until a meeting is exported again.
#[tauri::command]
#[specta::specta]
pub fn delete_meeting_topic(
    meetings: State<'_, Arc<MeetingManager>>,
    id: i64,
) -> Result<(), String> {
    text(meetings.records().delete_topic(id))
}

/// The folder meetings are exported to, such as an Obsidian vault.
#[tauri::command]
#[specta::specta]
pub fn get_meeting_export_dir(
    meetings: State<'_, Arc<MeetingManager>>,
) -> Result<Option<String>, String> {
    Ok(text(meetings.records().export_dir())?.map(|d| d.to_string_lossy().into_owned()))
}

/// Sets the export folder; `None` stops exporting.
#[tauri::command]
#[specta::specta]
pub fn set_meeting_export_dir(
    meetings: State<'_, Arc<MeetingManager>>,
    dir: Option<String>,
) -> Result<(), String> {
    let dir = dir.map(PathBuf::from);
    if let Some(dir) = dir.as_ref().filter(|d| !d.is_dir()) {
        return Err(format!("{} is not a folder", dir.display()));
    }
    text(meetings.records().set_export_dir(dir.as_deref()))
}

/// Writes the meeting's note again; the note's path.
#[tauri::command]
#[specta::specta]
pub async fn export_meeting(app: AppHandle, id: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || meeting::export(&app, &id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| "No export folder is set".to_string())
}

/// Writes every meeting's note again; how many were written.
#[tauri::command]
#[specta::specta]
pub async fn export_all_meetings(
    app: AppHandle,
    meetings: State<'_, Arc<MeetingManager>>,
) -> Result<u32, String> {
    let meetings = Arc::clone(&meetings);
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<u32> {
        let mut count = 0;
        for found in meetings.records().list()? {
            if meeting::export(&app, &found.id)?.is_some() {
                count += 1;
            }
        }
        Ok(count)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}
