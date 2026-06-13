pub mod dedup;
pub mod entity;
pub mod decay;
pub mod engine;

pub use engine::ConsolidationEngine;
pub use decay::{calculate_retention, reinforce_stability, initial_stability};
