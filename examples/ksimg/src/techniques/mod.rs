pub mod background_removal;
pub mod img2img;
pub mod img2txt;
pub mod registry;
pub mod segmentation;
pub mod txt2img;
pub mod txt2txt;
pub mod validate;

use crate::domain::Artifact;

pub use registry::{
    ParameterSchema, ParameterSpec, Technique, TechniqueId, TechniqueMetadata, TechniqueRegistry,
};

/// The only thing a technique executor is allowed to do: turn inputs into
/// one output artifact, or fail. No global state, no retries, no calls
/// into agents or the runtime — see CLAUDE.md section 22 ("Techniques
/// dürfen nicht ...").
pub trait TechniqueExecutor: Send + Sync {
    fn execute(&self, inputs: &[Artifact], attempt: u32) -> Result<Artifact, String>;
}

/// Assembles the registry this example ships with. A real system would
/// load this from a plugin directory instead of a fixed list.
pub fn default_registry() -> TechniqueRegistry {
    let mut registry = TechniqueRegistry::new();
    registry.register(txt2txt::contract(), Box::new(txt2txt::Txt2Txt));
    registry.register(txt2img::contract(), Box::new(txt2img::Txt2Img));
    registry.register(img2txt::contract(), Box::new(img2txt::Img2Txt));
    registry.register(img2img::contract(), Box::new(img2img::Img2Img));
    registry.register(
        segmentation::contract(),
        Box::new(segmentation::Segmentation),
    );
    registry.register(
        background_removal::contract(),
        Box::new(background_removal::BackgroundRemoval),
    );
    registry.register(validate::contract(), Box::new(validate::Validate));
    registry
}
