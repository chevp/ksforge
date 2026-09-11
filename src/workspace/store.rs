use std::path::{Path, PathBuf};

use crate::domain::{Execution, ExecutionId, ImplementationRequest, KsforgeError, Result};

/// Filesystem-backed store for [`Execution`]s under
/// `<workspace_root>/.ksforge/executions/<id>/state.json`. No database, no
/// daemon — a durable `Execution` is just JSON on disk (section: HITL /
/// "do not build a database or external state store").
pub struct ExecutionStore {
    root: PathBuf,
}

impl ExecutionStore {
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            root: workspace_root.join(".ksforge").join("executions"),
        }
    }

    fn dir_for(&self, id: &ExecutionId) -> PathBuf {
        self.root.join(&id.0)
    }

    fn state_path(&self, id: &ExecutionId) -> PathBuf {
        self.dir_for(id).join("state.json")
    }

    fn request_path(&self, id: &ExecutionId) -> PathBuf {
        self.dir_for(id).join("request.json")
    }

    fn processed_event_path(&self, id: &ExecutionId, event_id: &str) -> PathBuf {
        self.dir_for(id).join("processed-events").join(event_id)
    }

    /// Record that `event_id` (a GitHub comment id) has already been acted
    /// on for this execution, so a duplicate webhook delivery is a no-op
    /// (§DlruVSP) instead of a second resume attempt. A marker file, same
    /// pattern as `request.json`/`state.json` — no database, no daemon.
    pub fn mark_event_processed(&self, id: &ExecutionId, event_id: &str) -> Result<()> {
        let path = self.processed_event_path(id, event_id);
        std::fs::create_dir_all(path.parent().expect("processed-events has a parent dir"))?;
        std::fs::write(path, b"")?;
        Ok(())
    }

    pub fn event_already_processed(&self, id: &ExecutionId, event_id: &str) -> bool {
        self.processed_event_path(id, event_id).exists()
    }

    /// Persist the `ImplementationRequest` a run started from, so `resume`
    /// can reconstruct workspace/constraints/validation policy without the
    /// caller having to re-supply them on the command line.
    pub fn save_request(&self, id: &ExecutionId, request: &ImplementationRequest) -> Result<()> {
        let dir = self.dir_for(id);
        std::fs::create_dir_all(&dir)?;
        let json = serde_json::to_string_pretty(request)?;
        std::fs::write(self.request_path(id), json)?;
        Ok(())
    }

    pub fn load_request(&self, id: &ExecutionId) -> Result<ImplementationRequest> {
        let json = std::fs::read_to_string(self.request_path(id)).map_err(|_| {
            KsforgeError::ExecutionNotFound(format!("no stored request for execution {id}"))
        })?;
        Ok(serde_json::from_str(&json)?)
    }

    pub fn save(&self, execution: &Execution) -> Result<()> {
        let dir = self.dir_for(&execution.id);
        std::fs::create_dir_all(&dir)?;
        let json = serde_json::to_string_pretty(execution)?;
        std::fs::write(self.state_path(&execution.id), json)?;
        Ok(())
    }

    pub fn load(&self, id: &ExecutionId) -> Result<Execution> {
        let path = self.state_path(id);
        let json = std::fs::read_to_string(&path).map_err(|_| {
            KsforgeError::ExecutionNotFound(format!(
                "no execution {id} found under {}",
                self.root.display()
            ))
        })?;
        Ok(serde_json::from_str(&json)?)
    }

    /// Every `Execution` currently on disk under this store, for the
    /// coordination agent (`application::coordinate`) to build a picture of
    /// concurrent work from — see `prompts/coordinator/system-prompt.md`
    /// §BTDXyaH/§mo8Nl7V/§hdJave0. A missing or unreadable `state.json` in one execution's
    /// directory is skipped rather than failing the whole scan (a
    /// partially written file from a concurrent run is expected, not
    /// corruption to surface as an error).
    pub fn list_all(&self) -> Result<Vec<Execution>> {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Ok(Vec::new());
        };
        let mut executions = Vec::new();
        for entry in entries.filter_map(|e| e.ok()) {
            let state_path = entry.path().join("state.json");
            let Ok(json) = std::fs::read_to_string(&state_path) else {
                continue;
            };
            if let Ok(execution) = serde_json::from_str(&json) {
                executions.push(execution);
            }
        }
        Ok(executions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ChangeRequest;

    #[test]
    fn round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let store = ExecutionStore::new(dir.path());
        let execution = Execution::start(
            ChangeRequest::from_text("As a user...").unwrap(),
            "implement",
        );
        store.save(&execution).unwrap();

        let loaded = store.load(&execution.id).unwrap();
        assert_eq!(loaded.id, execution.id);
        assert_eq!(loaded.status, execution.status);
    }

    #[test]
    fn list_all_returns_every_saved_execution() {
        let dir = tempfile::tempdir().unwrap();
        let store = ExecutionStore::new(dir.path());
        let a = Execution::start(
            ChangeRequest::from_text("As a user, X.").unwrap(),
            "implement",
        );
        let b = Execution::start(ChangeRequest::from_text("As a user, Y.").unwrap(), "fix");
        store.save(&a).unwrap();
        store.save(&b).unwrap();

        let ids: std::collections::HashSet<_> = store
            .list_all()
            .unwrap()
            .into_iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(ids, [a.id, b.id].into_iter().collect());
    }

    #[test]
    fn list_all_on_an_empty_store_is_an_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        let store = ExecutionStore::new(dir.path());
        assert!(store.list_all().unwrap().is_empty());
    }

    #[test]
    fn missing_execution_is_a_named_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = ExecutionStore::new(dir.path());
        let err = store.load(&ExecutionId::new()).unwrap_err();
        assert!(matches!(err, KsforgeError::ExecutionNotFound(_)));
    }

    #[test]
    fn processed_events_are_idempotent_markers() {
        let dir = tempfile::tempdir().unwrap();
        let store = ExecutionStore::new(dir.path());
        let id = ExecutionId::new();

        assert!(!store.event_already_processed(&id, "12345"));
        store.mark_event_processed(&id, "12345").unwrap();
        assert!(store.event_already_processed(&id, "12345"));
        assert!(!store.event_already_processed(&id, "67890"));
    }
}
