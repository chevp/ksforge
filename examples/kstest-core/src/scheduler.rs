use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::thread;

use crate::event::Event;
use crate::intelligence::{self, AnalysisContext, Intelligence};

/// Which lane a piece of work runs in. Realtime work is executed inline by
/// the runtime loop; diagnostic work is handed to `Scheduler`, so a slow
/// analyzer can never delay realtime work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkClass {
    Realtime,
    Diagnostic,
}

/// Runs diagnostic work off the runtime's own thread when an
/// `Intelligence` is registered, so a slow analyzer can never delay
/// realtime work. With none registered, it answers immediately and
/// deterministically instead — intelligence is advisory, never required
/// (section 13: the runtime must not block on its absence).
pub struct Scheduler {
    intelligence: Option<Arc<dyn Intelligence>>,
    results: Sender<Event>,
}

impl Scheduler {
    pub fn new(intelligence: Option<Arc<dyn Intelligence>>, results: Sender<Event>) -> Self {
        Self {
            intelligence,
            results,
        }
    }

    pub fn schedule_diagnostic(&self, context: AnalysisContext) {
        match &self.intelligence {
            Some(provider) => {
                println!("[SLOW] diagnostic requested");
                let provider = Arc::clone(provider);
                let results = self.results.clone();
                thread::spawn(move || {
                    println!("[SLOW] analyzer started");
                    let result = provider.analyze(context);
                    println!("[SLOW] diagnosis available");
                    let _ = results.send(Event::DiagnosticAvailable(result));
                });
            }
            None => {
                let result = intelligence::deterministic_fallback(&context);
                let _ = self.results.send(Event::DiagnosticAvailable(result));
            }
        }
    }
}
