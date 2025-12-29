//! @acp:module "Constraints"
//! @acp:summary "AI behavioral guardrails and constraint system"
//! @acp:domain cli
//! @acp:layer model
//! @acp:stability stable
//!
//! This module provides types and logic for controlling AI behavior through:
//! - Style constraints (formatting rules)
//! - Mutation constraints (what can be changed)
//! - Experimental/hack tracking
//! - Debug session management
//! - Quality gates

mod enforcer;
mod guardrails;
mod types;

pub use types::{
    Approach, BehaviorModifier, ConstraintIndex, Constraints, DebugAttempt, DebugResult,
    DebugSession, DebugStatus, DeprecationAction, DeprecationInfo, HackMarker, HackType, LockLevel,
    ModifyPermission, MutationConstraint, PerformanceBudget, Priority, QualityGate, Reference,
    StyleConstraint,
};

pub use guardrails::{
    AIBehavior, AIGeneratedMarker, Attempt, AttemptStatus, Checkpoint, ComplexityMarker,
    FileGuardrails, FrameworkRequirement, GuardrailConstraints, GuardrailParser, HumanVerification,
    QualityMarkers, ReviewRequirements, StyleGuide, TechDebtItem, TemporaryKind, TemporaryMarker,
};

pub use enforcer::{
    GuardrailCheck, GuardrailEnforcer, RequiredAction, Severity, Violation, Warning,
};
