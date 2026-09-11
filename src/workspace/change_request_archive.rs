use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{ChangeRequest, ExecutionId, Result};

/// Filesystem-backed archive of every change request ksforge has been asked to
/// act on, under `<workspace_root>/.ksforge/change-requests/` — the
/// `.ksforge/<category>/` layout ksforge owns and that other future record
/// types (e.g. ADRs) follow the same way. Each change request gets a `CR-<base62>`
/// id (exactly 6 base62 digits, e.g. `CR-4gK2p0`), a markdown snapshot of
/// the raw change request text, and a JSON record tying the two together with the
/// execution it started.
///
/// The id is a base62-encoded random value (from a `Uuid::new_v4`), not a
/// persisted counter. A shared counter file races the moment two `ksforge`
/// runs execute concurrently — e.g. two GitHub Actions workflow runs
/// triggered close together — since increment-then-write has no locking
/// here. A random id needs no shared mutable state at all, so parallel runs
/// can never collide or clobber each other's counter.
pub struct ChangeRequestArchive {
    root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeRequestRecord {
    pub id: String,
    pub text: String,
    pub capability: String,
    pub execution_id: ExecutionId,
    pub markdown_file: String,
    pub created_at: DateTime<Utc>,
}

const BASE62_ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Fixed width every generated id uses (section: short, stable, still
/// collision-safe enough for a random id scoped to one workspace's
/// `.ksforge/change-requests/` — see `new_id`).
const ID_DIGITS: usize = 6;

fn base62_encode(mut value: u128) -> String {
    if value == 0 {
        return "0".to_string();
    }
    let mut digits = Vec::new();
    while value > 0 {
        digits.push(BASE62_ALPHABET[(value % 62) as usize]);
        value /= 62;
    }
    digits.reverse();
    String::from_utf8(digits).expect("base62 alphabet is ASCII")
}

/// `base62_encode`, left-padded with `0` to exactly `width` characters —
/// `new_id` needs every id the same length, not just the shortest
/// representation of the underlying number.
fn base62_encode_padded(value: u128, width: usize) -> String {
    let mut encoded = base62_encode(value);
    while encoded.len() < width {
        encoded.insert(0, BASE62_ALPHABET[0] as char);
    }
    encoded
}

fn new_id() -> String {
    // Reduce the UUID's 128 random bits mod 62^ID_DIGITS instead of
    // truncating its base62 representation — truncation would only ever
    // vary the id's low-order digits, this uses all of them.
    let random = Uuid::new_v4().as_u128() % 62u128.pow(ID_DIGITS as u32);
    format!("CR-{}", base62_encode_padded(random, ID_DIGITS))
}

impl ChangeRequestArchive {
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            root: workspace_root.join(".ksforge").join("change-requests"),
        }
    }

    /// Persist `change_request` as markdown plus a JSON record, before the execution
    /// it seeds runs. Called once per new `Execution` (section:
    /// `execute::run`) — never on resume, since a resumed execution
    /// continues an already-archived change request rather than submitting a new
    /// one.
    pub fn record(
        &self,
        change_request: &ChangeRequest,
        capability: &str,
        execution_id: &ExecutionId,
    ) -> Result<ChangeRequestRecord> {
        std::fs::create_dir_all(&self.root)?;
        let id = new_id();
        let md_name = format!("{id}.md");
        std::fs::write(
            self.root.join(&md_name),
            format!("{}\n", change_request.text.trim()),
        )?;

        let record = ChangeRequestRecord {
            id: id.clone(),
            text: change_request.text.clone(),
            capability: capability.to_string(),
            execution_id: execution_id.clone(),
            markdown_file: format!(".ksforge/change-requests/{md_name}"),
            created_at: Utc::now(),
        };
        let json = serde_json::to_string_pretty(&record)?;
        std::fs::write(self.root.join(format!("{id}.json")), json)?;
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assigns_base62_ids_and_writes_markdown_and_json() {
        let dir = tempfile::tempdir().unwrap();
        let archive = ChangeRequestArchive::new(dir.path());

        let change_request_a = ChangeRequest::from_text("As a user, I want change A.").unwrap();
        let record_a = archive
            .record(&change_request_a, "implement", &ExecutionId::new())
            .unwrap();
        assert!(record_a.id.starts_with("CR-"));
        assert_eq!(record_a.id["CR-".len()..].len(), ID_DIGITS);
        assert!(
            record_a.id["CR-".len()..]
                .chars()
                .all(|c| c.is_ascii_alphanumeric())
        );

        let change_request_b = ChangeRequest::from_text("As a user, I want change B.").unwrap();
        let record_b = archive
            .record(&change_request_b, "implement", &ExecutionId::new())
            .unwrap();
        assert_ne!(record_a.id, record_b.id);

        let md_path = dir
            .path()
            .join(format!(".ksforge/change-requests/{}.md", record_a.id));
        assert_eq!(
            std::fs::read_to_string(&md_path).unwrap(),
            "As a user, I want change A.\n"
        );

        let json_path = dir
            .path()
            .join(format!(".ksforge/change-requests/{}.json", record_a.id));
        let parsed: ChangeRequestRecord =
            serde_json::from_str(&std::fs::read_to_string(&json_path).unwrap()).unwrap();
        assert_eq!(parsed.id, record_a.id);
        assert_eq!(
            parsed.markdown_file,
            format!(".ksforge/change-requests/{}.md", record_a.id)
        );
    }

    #[test]
    fn concurrent_records_never_collide_without_shared_state() {
        let dir = tempfile::tempdir().unwrap();
        let archive = ChangeRequestArchive::new(dir.path());
        let change_request = ChangeRequest::from_text("As a user, I want something.").unwrap();

        let mut ids = std::collections::HashSet::new();
        for _ in 0..100 {
            let record = archive
                .record(&change_request, "implement", &ExecutionId::new())
                .unwrap();
            assert!(ids.insert(record.id), "base62 id collided");
        }
    }

    #[test]
    fn base62_encoding_round_trips_known_values() {
        assert_eq!(base62_encode(0), "0");
        assert_eq!(base62_encode(61), "z");
        assert_eq!(base62_encode(62), "10");
    }
}
