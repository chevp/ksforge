use ksimg::agents::{LogicalStep, LogicalWorkflow, MockPlanningAgent, PlanningAgent};
use ksimg::domain::{Constraints, DataType, ProcessingIntent};
use ksimg::runtime::{Event, MachineState, StateMachine};
use ksimg::techniques::default_registry;
use ksimg::workflow::ir::{InputSpec, OutputSpec, Step, StepId, WorkflowId};
use ksimg::workflow::{ValidationError, ValueRef, Workflow, WorkflowBuilder, WorkflowValidator};

fn logical(operations: &[&str]) -> LogicalWorkflow {
    LogicalWorkflow {
        steps: operations
            .iter()
            .map(|operation| LogicalStep {
                operation: operation.to_string(),
                purpose: String::new(),
            })
            .collect(),
    }
}

// Test 1: Text -> Image plans to txt2img -> validate.
#[test]
fn text_to_image_plans_txt2img_then_validate() {
    let registry = default_registry();
    let logical = logical(&["txt2img", "validate"]);

    let workflow =
        WorkflowBuilder::build(&logical, &registry, &Constraints::default(), DataType::Text)
            .expect("txt2img -> validate should build");

    let operations: Vec<String> = workflow
        .steps
        .iter()
        .map(|step| step.operation.to_string())
        .collect();
    assert_eq!(operations, vec!["txt2img", "validate"]);

    WorkflowValidator::validate(&workflow, &registry, &Constraints::default())
        .expect("workflow should validate");
}

// Test 2: Image -> background-removed image plans to
// segmentation -> background_removal -> validate.
#[test]
fn background_removal_plans_segmentation_removal_validate() {
    let registry = default_registry();
    let logical = logical(&["segmentation", "background_removal", "validate"]);

    let workflow = WorkflowBuilder::build(
        &logical,
        &registry,
        &Constraints::default(),
        DataType::Image,
    )
    .expect("segmentation -> background_removal -> validate should build");

    let operations: Vec<String> = workflow
        .steps
        .iter()
        .map(|step| step.operation.to_string())
        .collect();
    assert_eq!(
        operations,
        vec!["segmentation", "background_removal", "validate"]
    );

    WorkflowValidator::validate(&workflow, &registry, &Constraints::default())
        .expect("workflow should validate");
}

// Test 3: an Image fed straight into txt2txt is an invalid workflow and
// the validator must reject it.
#[test]
fn validator_rejects_a_technique_fed_the_wrong_input_type() {
    let registry = default_registry();
    let step_id = StepId::new("step-0-txt2txt");

    let workflow = Workflow {
        id: WorkflowId::new("bad-workflow"),
        input: InputSpec {
            data_type: DataType::Image,
        },
        steps: vec![Step {
            id: step_id.clone(),
            operation: "txt2txt".into(),
            inputs: vec![ValueRef::Input],
            output: ValueRef::StepOutput(step_id.clone()),
        }],
        output: OutputSpec {
            value: ValueRef::StepOutput(step_id),
            data_type: DataType::Text,
        },
    };

    let errors = WorkflowValidator::validate(&workflow, &registry, &Constraints::default())
        .expect_err("feeding an Image to txt2txt must be rejected");

    assert!(
        errors
            .iter()
            .any(|error| matches!(error, ValidationError::InvalidInputType { .. }))
    );
}

// Test 4: planning a text-to-image intent inserts a txt2txt
// prompt-improvement step ahead of txt2img.
#[test]
fn text_to_image_intent_plans_txt2txt_before_txt2img() {
    let planning_agent = MockPlanningAgent;
    let intent = ProcessingIntent {
        input_type: DataType::Text,
        output_type: DataType::Image,
        goal: "Generate image".into(),
        constraints: Constraints::default(),
    };

    let logical = planning_agent.plan(&intent);

    let operations: Vec<String> = logical
        .steps
        .iter()
        .map(|step| step.operation.clone())
        .collect();
    assert_eq!(operations, vec!["txt2txt", "txt2img", "validate"]);
}

// Test 4b: a standalone prompt-improvement intent plans to txt2txt alone.
#[test]
fn prompt_improvement_intent_plans_standalone_txt2txt() {
    let planning_agent = MockPlanningAgent;
    let intent = ProcessingIntent {
        input_type: DataType::Text,
        output_type: DataType::Text,
        goal: "Improve prompt for image generation".into(),
        constraints: Constraints::default(),
    };

    let logical = planning_agent.plan(&intent);

    let operations: Vec<String> = logical
        .steps
        .iter()
        .map(|step| step.operation.clone())
        .collect();
    assert_eq!(operations, vec!["txt2txt"]);
}

// Test 5: a failed step recovers instead of stopping the runtime.
#[test]
fn a_failed_step_transitions_to_recovering_not_stopped() {
    let mut machine = StateMachine::new();
    machine.handle(Event::Start);
    machine.handle(Event::RunStep);

    let transition = machine.handle(Event::StepFailed("segmentation quality too low".into()));

    assert_eq!(transition.to, MachineState::Recovering);
    assert_ne!(transition.to, MachineState::Stopped);
}

// Test 5: recovering via retry returns to processing, then ready.
#[test]
fn recovering_retries_and_returns_to_ready() {
    let mut machine = StateMachine::new();
    machine.handle(Event::Start);
    machine.handle(Event::RunStep);
    machine.handle(Event::StepFailed("segmentation quality too low".into()));

    let retried = machine.handle(Event::RetryRequested);
    assert_eq!(retried.to, MachineState::Processing);

    let succeeded = machine.handle(Event::StepSucceeded);
    assert_eq!(succeeded.to, MachineState::Ready);
}

// Test 6: an explicit shutdown, and only that, stops the runtime.
#[test]
fn explicit_shutdown_stops_the_runtime() {
    let mut machine = StateMachine::new();
    machine.handle(Event::Start);

    let stopping = machine.handle(Event::ShutdownRequested);
    assert_eq!(stopping.to, MachineState::Stopping);

    let stopped = machine.handle(Event::ShutdownCompleted);
    assert_eq!(stopped.to, MachineState::Stopped);
}
