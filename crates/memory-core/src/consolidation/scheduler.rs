use crate::consolidation::ConsolidationEngine;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

/// Background decay scheduler that runs Ebbinghaus decay on a timer.
/// Spawned as a Tokio task, runs every `interval` (default 24 hours).
pub struct DecayScheduler {
    engine: Arc<ConsolidationEngine>,
    interval: Duration,
}

impl DecayScheduler {
    pub fn new(engine: Arc<ConsolidationEngine>, interval: Duration) -> Self {
        Self { engine, interval }
    }

    /// Start the background decay loop. This never returns (runs forever).
    pub async fn run(self) {
        loop {
            tokio::time::sleep(self.interval).await;

            info!("DecayScheduler: starting batch consolidation");
            match self.engine.batch_consolidate(None, None).await {
                Ok(_) => info!("DecayScheduler: batch consolidation completed"),
                Err(e) => error!("DecayScheduler: batch consolidation failed: {e}"),
            }
        }
    }
}
