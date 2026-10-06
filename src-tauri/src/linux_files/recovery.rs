use super::{filesystem, lock, Environment, Record, Service};
use crate::{linux_guard::Root, linux_journal::Journal};

pub(super) fn records(journal: &Journal) -> Result<Vec<Record>, String> {
    let mut records = journal
        .list()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(Record::from)
        .collect::<Vec<_>>();
    for parent in journal.list_parents().map_err(|error| error.to_string())? {
        let stage = serde_json::to_value(parent.stage).map_err(|error| error.to_string())?;
        let stage = stage.as_str().ok_or("Invalid parent recovery stage")?;
        records.push(Record { id: parent.id, root: parent.root.unwrap_or_default(), path: parent.destination.unwrap_or_default(),
            existed: false, stage: format!("parent{}{}", stage[..1].to_uppercase(), &stage[1..]), created_at: parent.created_at,
            root_volume: 0, root_index: 0, warning: Some(format!("Parent creation: {} recorded directories, uncertain index: {:?}. {} Destination directories are retained.", parent.created, parent.uncertain, parent.warning.unwrap_or(""))) });
    }
    Ok(records)
}
pub(super) fn get(journal: &Journal, id: &str) -> Result<Record, String> {
    if !id.starts_with("p-") {
        return journal
            .get(id)
            .map(Record::from)
            .map_err(|error| error.to_string());
    }
    records(journal)?
        .into_iter()
        .find(|record| record.id == id)
        .ok_or_else(|| "Unknown recovery record".into())
}
impl Service {
    pub(crate) fn list(&self, environment: &Environment) -> Result<Vec<Record>, String> {
        let _filesystem = crate::git::filesystem_gate()
            .try_read()
            .map_err(|_| "Another file operation is running; retry after it finishes")?;
        let Some(journal) =
            Journal::open_existing(&environment.app_data).map_err(|error| error.to_string())?
        else {
            return Ok(Vec::new());
        };
        journal.reconcile().map_err(|error| error.to_string())?;
        records(&journal)
    }
    pub(crate) async fn undo(&self, environment: &Environment, id: &str) -> Result<Record, String> {
        if id.starts_with("p-") {
            return Err(
                "Parent recovery retains destination directories; restore the linked file instead"
                    .into(),
            );
        }
        let record = {
            let journal = Journal::open_existing(&environment.app_data)
                .map_err(|error| error.to_string())?
                .ok_or("Recovery storage is absent")?;
            get(&journal, id)?
        };
        let settings = environment.load()?;
        let safe =
            crate::compare::registered_write_root(&settings, &record.root, &record.path).await?;
        let _filesystem = filesystem()?;
        environment.revalidate(&settings)?;
        let value = safe.linux_value()?;
        Root::reopen(&value)
            .and_then(|root| root.probe_write())
            .map_err(|error| error.to_string())?;
        let journal = Journal::open_guarded(&environment.app_data, &[value])
            .map_err(|error| error.to_string())?;
        journal.undo(id).map_err(|error| {
            format!(
                "{}; recovery record: {}; applied: {}",
                error, id, error.applied
            )
        })?;
        get(&journal, id)
    }
    pub(crate) fn resolve(
        &self,
        environment: &Environment,
        id: &str,
        confirmed: bool,
    ) -> Result<Record, String> {
        if !confirmed {
            return Err("Conflict resolution requires explicit confirmation".into());
        }
        let _filesystem = filesystem()?;
        let journal = Journal::open_existing(&environment.app_data)
            .map_err(|error| error.to_string())?
            .ok_or("Recovery storage is absent")?;
        journal.reconcile().map_err(|error| error.to_string())?;
        if id.starts_with("p-") {
            journal
                .acknowledge_parent(id, confirmed)
                .map_err(|error| error.to_string())?;
        } else {
            journal
                .resolve(id, confirmed)
                .map_err(|error| error.to_string())?;
        }
        get(&journal, id)
    }
    pub(crate) fn cleanup(
        &self,
        environment: &Environment,
        ids: &[String],
        confirmed: bool,
    ) -> Result<usize, String> {
        if !confirmed {
            return Err("Cleanup requires explicit confirmation".into());
        }
        if ids.len() > 1024 {
            return Err("Cleanup request exceeds limit".into());
        }
        let _filesystem = filesystem()?;
        let tickets = lock(&self.tickets)?;
        if ids.iter().any(|id| {
            tickets
                .values()
                .any(|ticket| ticket.undo_records.contains(id))
        }) {
            return Err("Close editors referencing these undo records before cleanup".into());
        }
        let Some(journal) =
            Journal::open_existing(&environment.app_data).map_err(|error| error.to_string())?
        else {
            return Ok(0);
        };
        let mut count = 0;
        for id in ids {
            let result = if id.starts_with("p-") {
                journal.cleanup_parent(id, confirmed)
            } else {
                journal.cleanup(id, confirmed)
            }
            .map_err(|error| format!("Cleanup stopped after {count} records: {error}"))?;
            if !result.complete {
                return Err(format!(
                    "Cleanup stopped after {count} records: {}",
                    result.warning.unwrap_or("Private artifacts retained")
                ));
            }
            if result.removed > 0 {
                count += 1;
            }
        }
        Ok(count)
    }
}
