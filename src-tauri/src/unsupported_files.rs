use crate::platform::unavailable_reason;

#[tauri::command]
pub async fn file_edit_open(
    id: String,
    generation: u64,
    file_id: String,
    side: String,
) -> Result<serde_json::Value, String> {
    let _ = (id, generation, file_id, side);
    Err(unavailable_reason("edit"))
}

#[tauri::command]
pub async fn file_edit_close(ticket: String) -> Result<bool, String> {
    let _ = ticket;
    Err(unavailable_reason("edit"))
}

#[tauri::command]
pub async fn file_save(ticket: String, bytes: Vec<u8>) -> Result<serde_json::Value, String> {
    let _ = (ticket, bytes);
    Err(unavailable_reason("edit"))
}

#[tauri::command]
pub async fn copy_preview(
    id: String,
    generation: u64,
    file_id: String,
    side: String,
) -> Result<serde_json::Value, String> {
    let _ = (id, generation, file_id, side);
    Err(unavailable_reason("copy"))
}

#[tauri::command]
pub async fn copy_apply(id: String, confirmed: bool) -> Result<serde_json::Value, String> {
    let _ = (id, confirmed);
    Err(unavailable_reason("copy"))
}

#[tauri::command]
pub async fn copy_cancel(id: String) -> Result<bool, String> {
    let _ = id;
    Err(unavailable_reason("copy"))
}

#[tauri::command]
pub async fn recovery_list() -> Result<serde_json::Value, String> {
    Err(unavailable_reason("recovery"))
}

#[tauri::command]
pub async fn recovery_undo(id: String) -> Result<serde_json::Value, String> {
    let _ = id;
    Err(unavailable_reason("recovery"))
}

#[tauri::command]
pub async fn recovery_cleanup(ids: Vec<String>, confirmed: bool) -> Result<usize, String> {
    let _ = (ids, confirmed);
    Err(unavailable_reason("recovery"))
}

#[tauri::command]
pub async fn recovery_resolve(id: String, confirmed: bool) -> Result<serde_json::Value, String> {
    let _ = (id, confirmed);
    Err(unavailable_reason("recovery"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn direct_edit_copy_and_recovery_calls_return_capability_refusals() {
        let fixture = crate::platform::Fixture::new("refusals");
        let sentinel = fixture.0.join("sentinel");
        std::fs::write(&sentinel, b"retained").unwrap();
        let id = sentinel.to_str().unwrap().to_string();
        let edit = unavailable_reason("edit");
        let copy = unavailable_reason("copy");
        let recovery = unavailable_reason("recovery");
        assert_eq!(
            file_edit_open(id.clone(), 1, id.clone(), "left".into())
                .await
                .unwrap_err(),
            edit
        );
        assert_eq!(file_edit_close(id.clone()).await.unwrap_err(), edit);
        assert_eq!(
            file_save(id.clone(), b"changed".to_vec())
                .await
                .unwrap_err(),
            edit
        );
        assert_eq!(
            copy_preview(id.clone(), 1, id.clone(), "right".into())
                .await
                .unwrap_err(),
            copy
        );
        assert_eq!(copy_apply(id.clone(), true).await.unwrap_err(), copy);
        assert_eq!(copy_cancel(id.clone()).await.unwrap_err(), copy);
        assert_eq!(recovery_list().await.unwrap_err(), recovery);
        assert_eq!(recovery_undo(id.clone()).await.unwrap_err(), recovery);
        assert_eq!(
            recovery_cleanup(vec![id.clone()], true).await.unwrap_err(),
            recovery
        );
        assert_eq!(recovery_resolve(id, true).await.unwrap_err(), recovery);
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"retained");
        assert_eq!(std::fs::read_dir(&fixture.0).unwrap().count(), 2);
    }
}
