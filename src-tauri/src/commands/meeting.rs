use std::sync::Arc;
use tauri::State;

use crate::managers::history::HistoryEntry;
use crate::managers::meeting::{MeetingManager, MeetingState};

#[tauri::command]
#[specta::specta]
pub fn get_meeting_state(meetings: State<'_, Arc<MeetingManager>>) -> MeetingState {
    meetings.state()
}

/// Starts recording the microphone and system audio.
#[tauri::command]
#[specta::specta]
pub async fn start_meeting(
    meetings: State<'_, Arc<MeetingManager>>,
) -> Result<MeetingState, String> {
    let meetings = Arc::clone(&meetings);
    tauri::async_runtime::spawn_blocking(move || meetings.start())
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
