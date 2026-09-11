/// The kinds of value that flow between techniques. Kept intentionally
/// small — this example is about the architecture around techniques, not
/// about modeling real image data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    Text,
    Image,
    Mask,
    ValidatedImage,
}

/// A named value flowing through a workflow. No real payload is stored —
/// `description` stands in for it, so the whole example stays inspectable
/// as plain strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub id: String,
    pub data_type: DataType,
    pub description: String,
}

impl Artifact {
    pub fn new(id: impl Into<String>, data_type: DataType, description: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            data_type,
            description: description.into(),
        }
    }
}

/// Limits the builder and validator must respect. Deliberately minimal —
/// only what the example scenarios exercise.
#[derive(Debug, Clone, Default)]
pub struct Constraints {
    pub max_steps: Option<usize>,
    pub denied_techniques: Vec<String>,
}
