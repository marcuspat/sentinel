/// Capability definitions, traits, and result types.
pub mod capability;
/// Error types used throughout the workspace.
pub mod error;
/// Execution context threaded through every capability invocation.
pub mod execution_context;
/// Execution plan and step types.
pub mod plan;
/// Session lifecycle management.
pub mod session;
/// UTC timestamp and weekday types shared by the workspace.
pub mod timestamp;

// ── Top-level re-exports ──────────────────────────────────────────────────────

// capability
pub use capability::{
    Capability, CapabilityKind, CapabilityManifest, CapabilityResult, ResourceImpact, RiskTier,
};

// error
pub use error::CoreError;

// execution_context
pub use execution_context::{ExecutionContext, ResourceLimits};

// plan
pub use plan::{ApprovalDecision, Plan, PlanStep, StepStatus};

// session
pub use session::{Observation, Session, SessionCheckpoint, SessionPhase};

pub use timestamp::{ParseTimestampError, TimestampPrecision, UtcTimestamp, UtcWeekday};
