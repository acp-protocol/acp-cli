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

mod types;
mod guardrails;
mod enforcer;

pub use types::{
    Constraints, ConstraintIndex, ModifyPermission,
    StyleConstraint, MutationConstraint, LockLevel,
    BehaviorModifier, Approach, Priority,
    QualityGate, PerformanceBudget,
    DeprecationInfo, DeprecationAction, Reference,
    HackMarker, HackType,
    DebugSession, DebugAttempt, DebugStatus, DebugResult,
};

pub use guardrails::{
    FileGuardrails, GuardrailParser,
    GuardrailConstraints, StyleGuide, FrameworkRequirement,
    AIBehavior, TemporaryMarker, TemporaryKind,
    Attempt, AttemptStatus, Checkpoint,
    ReviewRequirements, AIGeneratedMarker, HumanVerification,
    QualityMarkers, TechDebtItem, ComplexityMarker,
};

pub use enforcer::{
    GuardrailEnforcer, GuardrailCheck,
    Violation, Warning, RequiredAction, Severity,
};
