//! Application layer: the shared execution pipeline plus one thin
//! `Capability` impl per verb. Orchestration mechanism lives in
//! `execute`/`resume`; policy (prompts, tool scope, constraints) lives in
//! `implement`/`review`/`fix`/`explain`.

pub mod coordinate;
pub mod execute;
pub mod explain;
pub mod fix;
pub mod img2img;
pub mod implement;
pub mod prompt;
pub mod resume;
pub mod review;
pub mod test;
pub mod txt2img;
