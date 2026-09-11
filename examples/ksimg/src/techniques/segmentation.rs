use crate::domain::{Artifact, DataType};

use super::registry::{Technique, TechniqueMetadata};
use super::{TechniqueExecutor, TechniqueId};

/// Image -> Mask. Deterministic mock, in the same spirit as
/// `kstest-core`'s `MessageProcessor`: an input whose description names it
/// "unstable" fails on the first attempt and succeeds from the second
/// attempt on, without any hidden state of its own. This is what drives
/// the failure/recovery demo in `main.rs`.
pub struct Segmentation;

impl TechniqueExecutor for Segmentation {
    fn execute(&self, inputs: &[Artifact], attempt: u32) -> Result<Artifact, String> {
        let image = inputs
            .iter()
            .find(|artifact| artifact.data_type == DataType::Image)
            .ok_or_else(|| "segmentation requires an Image input".to_string())?;
        if image.description.contains("unstable") && attempt == 0 {
            return Err(format!("segmentation quality too low for {}", image.id));
        }
        Ok(Artifact::new(
            format!("{}-mask", image.id),
            DataType::Mask,
            format!("segmentation mask for {}", image.id),
        ))
    }
}

pub fn contract() -> Technique {
    Technique {
        id: TechniqueId::new("segmentation"),
        input_types: vec![DataType::Image],
        output_types: vec![DataType::Mask],
        parameters: vec![],
        metadata: TechniqueMetadata {
            cost: 2,
            latency_ms: 80,
            capabilities: vec!["foreground-detection".into()],
            constraints: vec!["requires-a-foreground-subject".into()],
        },
    }
}
