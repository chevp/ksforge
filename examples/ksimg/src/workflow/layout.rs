use super::ir::Workflow;

/// Presentation-only metadata: where a workflow's steps would sit on a
/// canvas. This is UI concern, not workflow logic — see CLAUDE.md
/// section 17. Nothing in `builder`, `validator`, or `runtime` reads a
/// `WorkflowLayout`.
#[derive(Debug, Clone)]
pub struct LayoutNode {
    pub step: String,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone)]
pub struct WorkflowLayout {
    pub nodes: Vec<LayoutNode>,
}

impl WorkflowLayout {
    /// A simple left-to-right arrangement, purely for display.
    pub fn left_to_right(workflow: &Workflow) -> Self {
        let nodes = workflow
            .steps
            .iter()
            .enumerate()
            .map(|(index, step)| LayoutNode {
                step: step.operation.to_string(),
                x: 100 + (index as i32) * 200,
                y: 100,
            })
            .collect();
        Self { nodes }
    }
}
