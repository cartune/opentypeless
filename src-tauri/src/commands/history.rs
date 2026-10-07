use crate::storage;

#[tauri::command]
pub async fn get_history(
    state: tauri::State<'_, storage::HistoryStore>,
    limit: u32,
    offset: u32,
) -> Result<Vec<storage::HistoryEntry>, String> {
    state.list(limit, offset).await.map_err(|e| e.to_string())
}

/// Aggregated BYOK usage since an ISO date prefix such as "2026-10-01".
#[tauri::command]
pub async fn get_usage_summary(
    state: tauri::State<'_, storage::HistoryStore>,
    since: String,
) -> Result<storage::UsageSummary, String> {
    let since = since.trim();
    if since.len() < 10 || !since.is_ascii() {
        return Err("since must be an ISO-8601 date".to_string());
    }
    state.usage_summary(since).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_history(state: tauri::State<'_, storage::HistoryStore>) -> Result<(), String> {
    state.clear().await.map_err(|e| e.to_string())
}
