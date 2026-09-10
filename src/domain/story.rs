use std::path::Path;

use serde::{Deserialize, Serialize};

use super::error::{KsforgeError, Result};

/// A software user story, the first-class input to every ksforge capability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserStory {
    pub text: String,
}

impl UserStory {
    pub fn from_text(text: impl Into<String>) -> Result<Self> {
        let text = text.into();
        if text.trim().is_empty() {
            return Err(KsforgeError::Usage(
                "user story text must not be empty".into(),
            ));
        }
        Ok(Self { text })
    }

    pub fn from_file(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|e| {
            KsforgeError::Usage(format!("failed to read story file {}: {e}", path.display()))
        })?;
        Self::from_text(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_story() {
        assert!(UserStory::from_text("   ").is_err());
    }

    #[test]
    fn accepts_nonempty_story() {
        let s = UserStory::from_text("As a user, I want...").unwrap();
        assert_eq!(s.text, "As a user, I want...");
    }
}
