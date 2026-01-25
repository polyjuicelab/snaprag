//! Function-based hooks for in-process event handling

use clap::ValueEnum;
use serde::Deserialize;
use serde::Serialize;
use tracing::debug;
use tracing::info;

use crate::sync::hooks::EventData;

/// Supported function hook types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum FuncHookType {
    /// Log event payloads
    #[value(name = "LOG")]
    Log,
}

/// Trait for function-based hooks
pub trait FuncHook: Send + Sync {
    fn name(&self) -> &'static str;
    fn handle(&self, event: &EventData);
}

/// Basic log hook implementation
pub struct LogHook;

impl FuncHook for LogHook {
    fn name(&self) -> &'static str {
        "log"
    }

    fn handle(&self, event: &EventData) {
        info!(
            event_type = ?event.event_type,
            fid = event.fid,
            target_fid = event.target_fid,
            timestamp = event.timestamp,
            "FuncHook: log event"
        );
        debug!(data = ?event.data, "FuncHook: log payload");
    }
}

/// Build a function hook implementation from the enum
pub fn build_func_hook(kind: FuncHookType) -> Box<dyn FuncHook> {
    match kind {
        FuncHookType::Log => Box::new(LogHook),
    }
}
