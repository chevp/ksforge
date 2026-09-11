//! Runnable walkthrough for `ksimg`: wires up mock intent, planning, and
//! recovery agents with the technique registry, workflow builder,
//! validator, and state-machine-driven engine, then exercises four
//! scenarios (img2img, background removal, text-to-image, and a
//! failure/recovery case), printing each stage's output. See `README.md`
//! for the architecture this walkthrough demonstrates.

use ksimg::agents::{
    IntentAgent, MockIntentAgent, MockPlanningAgent, MockRecoveryAgent, PlanningAgent,
};
use ksimg::domain::{Artifact, DataType};
use ksimg::runtime::{ExecutionOutcome, WorkflowEngine};
use ksimg::techniques::default_registry;
use ksimg::workflow::{WorkflowBuilder, WorkflowValidator};

fn fmt_types(types: &[DataType]) -> String {
    types
        .iter()
        .map(|t| format!("{t:?}"))
        .collect::<Vec<_>>()
        .join(" + ")
}

fn run_scenario(name: &str, user_input: &str, input: Artifact) {
    println!("\n=== {name} ===\n");

    let intent_agent = MockIntentAgent;
    let planning_agent = MockPlanningAgent;
    let registry = default_registry();
    let recovery_agent = MockRecoveryAgent { max_retries: 1 };

    let intent = intent_agent.interpret(user_input);
    println!("Intent:");
    println!("  Input: {:?}", intent.input_type);
    println!("  Goal: {}", intent.goal);
    println!("  Output: {:?}", intent.output_type);

    let logical = planning_agent.plan(&intent);
    println!("\nPlanning:");
    for (position, step) in logical.steps.iter().enumerate() {
        println!("  {}. {}", position + 1, step.operation);
    }

    println!("\nBuilding workflow...");
    let workflow =
        match WorkflowBuilder::build(&logical, &registry, &intent.constraints, intent.input_type) {
            Ok(workflow) => {
                for step in &workflow.steps {
                    let technique = registry
                        .get(&step.operation)
                        .expect("builder only emits known techniques");
                    println!(
                        "  \u{2713} {}: {} -> {}",
                        step.operation,
                        fmt_types(&technique.input_types),
                        fmt_types(&technique.output_types)
                    );
                }
                workflow
            }
            Err(errors) => {
                println!("  \u{2717} build failed: {errors:?}");
                return;
            }
        };

    println!("\nValidation:");
    match WorkflowValidator::validate(&workflow, &registry, &intent.constraints) {
        Ok(()) => println!("  \u{2713} workflow valid"),
        Err(errors) => {
            println!("  \u{2717} workflow invalid: {errors:?}");
            return;
        }
    }

    println!("\nExecution:");
    let mut engine = WorkflowEngine::new(&registry, &recovery_agent);
    match engine.run(&workflow, input) {
        ExecutionOutcome::Completed(result) => {
            println!("\nResult:");
            println!("  {:?}", result.data_type);
        }
        ExecutionOutcome::Degraded { partial, reason } => {
            println!("\nResult (degraded):");
            println!("  reason: {reason}");
            if let Some(partial) = partial {
                println!(
                    "  best effort: {:?} ({})",
                    partial.data_type, partial.description
                );
            }
        }
    }
    engine.shutdown();
}

fn main() {
    println!("=== ksimg example ===");

    run_scenario(
        "Scenario A: img2img",
        "Transform this image into another image.",
        Artifact::new("image-a-001", DataType::Image, "source image"),
    );

    run_scenario(
        "Scenario B: background removal",
        "Create an image with the background removed.",
        Artifact::new("image-b-001", DataType::Image, "source image"),
    );

    run_scenario(
        "Scenario C: text to image",
        "Generate an image from this text.",
        Artifact::new("text-c-001", DataType::Text, "a mountain at sunset"),
    );

    run_scenario(
        "Scenario D: failure + recovery",
        "Create an image with the background removed.",
        Artifact::new("image-d-001", DataType::Image, "unstable source image"),
    );

    run_scenario(
        "Scenario E: prompt improvement",
        "Improve the prompt before generating an image.",
        Artifact::new("text-e-001", DataType::Text, "a cat wearing sunglasses"),
    );
}
