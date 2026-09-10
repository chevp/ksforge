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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::UserStory;

    #[test]
    fn round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let store = ExecutionStore::new(dir.path());
        let execution =
            Execution::start(UserStory::from_text("As a user...").unwrap(), "implement");
        store.save(&execution).unwrap();

        let loaded = store.load(&execution.id).unwrap();
        assert_eq!(loaded.id, execution.id);
        assert_eq!(loaded.status, execution.status);
    }

    #[test]
    fn missing_execution_is_a_named_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = ExecutionStore::new(dir.path());
        let err = store.load(&ExecutionId::new()).unwrap_err();
        assert!(matches!(err, KsforgeError::ExecutionNotFound(_)));
    }
}
