use super::{
    copy::{AsyncContent, Outcome, Preview},
    tickets::{EditFile, Request},
    Context, Environment, Record, Service,
};
use std::sync::{Arc, OnceLock};
use tauri::{AppHandle, Manager};

pub(super) fn environment(app: &AppHandle) -> Result<Environment, String> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let app = app.clone();
    Ok(Environment {
        app_data,
        settings: Arc::new(move || crate::settings::load_settings(app.clone())),
    })
}
async fn work<T: Send + 'static>(
    app: AppHandle,
    operation: impl FnOnce(AppHandle, Environment) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    static SLOTS: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
    let permit = SLOTS
        .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(4)))
        .clone()
        .try_acquire_owned()
        .map_err(|_| "Linux file operations are busy; retry after they finish")?;
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        let environment = environment(&app)?;
        operation(app, environment)
    })
    .await
    .map_err(|_| "Linux file operation could not finish; inspect recovery before retrying")?
}
#[tauri::command]
pub(crate) async fn file_edit_open(
    app: AppHandle,
    id: String,
    generation: u64,
    file_id: String,
    side: String,
) -> Result<EditFile, String> {
    work(app, move |app, environment| {
        tauri::async_runtime::block_on(app.state::<Service>().open(
            Context {
                comparisons: &app.state::<crate::compare::Service>(),
                environment: &environment,
            },
            Request {
                session: id,
                generation,
                file_id,
                side,
            },
        ))
    })
    .await
}
#[tauri::command]
pub(crate) async fn file_edit_close(
    service: tauri::State<'_, Service>,
    ticket: String,
) -> Result<bool, String> {
    service.close(&ticket)
}
#[tauri::command]
pub(crate) async fn file_save(
    app: AppHandle,
    ticket: String,
    bytes: Vec<u8>,
) -> Result<Record, String> {
    work(app, move |app, environment| {
        tauri::async_runtime::block_on(app.state::<Service>().save(
            Context {
                comparisons: &app.state::<crate::compare::Service>(),
                environment: &environment,
            },
            &ticket,
            bytes,
        ))
    })
    .await
}
struct Content {
    app: AppHandle,
    id: String,
    generation: u64,
}
impl AsyncContent for Content {
    async fn read(&mut self, file_id: &str, side: &str) -> Result<Vec<u8>, String> {
        crate::compare::comparison_content(
            self.app.clone(),
            self.app.state::<crate::compare::Service>(),
            self.id.clone(),
            self.generation,
            file_id.into(),
            side.into(),
        )
        .await
        .map(|content| content.bytes)
        .map_err(|problem| problem.message)
    }
}
#[tauri::command]
pub(crate) async fn copy_preview(
    app: AppHandle,
    id: String,
    generation: u64,
    file_id: String,
    side: String,
) -> Result<Preview, String> {
    work(app, move |app, environment| {
        let content = Content {
            app: app.clone(),
            id: id.clone(),
            generation,
        };
        tauri::async_runtime::block_on(app.state::<Service>().preview(
            Context {
                comparisons: &app.state::<crate::compare::Service>(),
                environment: &environment,
            },
            Request {
                session: id,
                generation,
                file_id,
                side,
            },
            content,
        ))
    })
    .await
}
#[tauri::command]
pub(crate) async fn copy_cancel(
    service: tauri::State<'_, Service>,
    id: String,
) -> Result<bool, String> {
    service.cancel(&id)
}
#[tauri::command]
pub(crate) async fn copy_apply(
    app: AppHandle,
    id: String,
    confirmed: bool,
) -> Result<Vec<Outcome>, String> {
    work(app, move |app, environment| {
        tauri::async_runtime::block_on(app.state::<Service>().apply(
            Context {
                comparisons: &app.state::<crate::compare::Service>(),
                environment: &environment,
            },
            &id,
            confirmed,
        ))
    })
    .await
}
#[tauri::command]
pub(crate) async fn recovery_list(app: AppHandle) -> Result<Vec<Record>, String> {
    work(app, |app, environment| {
        app.state::<Service>().list(&environment)
    })
    .await
}
#[tauri::command]
pub(crate) async fn recovery_undo(app: AppHandle, id: String) -> Result<Record, String> {
    work(app, move |app, environment| {
        tauri::async_runtime::block_on(app.state::<Service>().undo(&environment, &id))
    })
    .await
}
#[tauri::command]
pub(crate) async fn recovery_resolve(
    app: AppHandle,
    id: String,
    confirmed: bool,
) -> Result<Record, String> {
    work(app, move |app, environment| {
        app.state::<Service>().resolve(&environment, &id, confirmed)
    })
    .await
}
#[tauri::command]
pub(crate) async fn recovery_cleanup(
    app: AppHandle,
    ids: Vec<String>,
    confirmed: bool,
) -> Result<usize, String> {
    work(app, move |app, environment| {
        app.state::<Service>()
            .cleanup(&environment, &ids, confirmed)
    })
    .await
}
