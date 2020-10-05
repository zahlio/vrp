use crate::solver::evolution::{EvolutionResult, EvolutionStrategy, OperatorConfig};
use crate::solver::{RefinementContext, Telemetry};

pub struct RunBranches {}

impl EvolutionStrategy for RunBranches {
    fn run(
        &self,
        refinement_ctx: RefinementContext,
        operators: OperatorConfig,
        _telemetry: Telemetry,
    ) -> EvolutionResult {
        branches::run_evolution(refinement_ctx, operators)
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod branches {
    use super::*;
    use crate::construction::Quota;
    use crate::solver::evolution::{should_stop, EvolutionStrategy, RunStraight};
    use crate::solver::{DominancePopulation, Population, RefinementContext, Telemetry, TelemetryMode};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tokio::sync::mpsc;

    struct AtomicQuota {
        original: Option<Arc<dyn Quota + Send + Sync>>,
        is_terminated: Arc<AtomicBool>,
    }

    impl Quota for AtomicQuota {
        fn is_reached(&self) -> bool {
            self.original.as_ref().map_or(false, |q| q.is_reached())
                || self.is_terminated.fetch_and(true, Ordering::SeqCst)
        }
    }

    fn create_branch(
        refinement_ctx: &RefinementContext,
        operators: OperatorConfig,
        mut branch_sender: mpsc::Sender<Box<dyn Population + Sync + Send>>,
        is_terminated: Arc<AtomicBool>,
    ) {
        let best_individuals = refinement_ctx
            .population
            .ranked()
            .filter_map(|(individual, rank)| if rank == 0 { Some(individual.deep_copy()) } else { None })
            .collect();

        let mut population = DominancePopulation::new(refinement_ctx.problem.clone(), 4);
        population.add_all(best_individuals);

        let quota: Option<Arc<dyn Quota + Send + Sync>> =
            Some(Arc::new(AtomicQuota { original: refinement_ctx.quota.clone(), is_terminated }));

        let refinement_ctx = RefinementContext {
            problem: refinement_ctx.problem.clone(),
            population: Box::new(population),
            state: Default::default(),
            quota: quota.clone(),
            statistics: Default::default(),
        };

        tokio::spawn(async move {
            let (population, _) = RunStraight::default()
                .run(refinement_ctx, operators, Telemetry::new(TelemetryMode::None))
                .expect("cannot find any solution");
            if let Err(_) = branch_sender.send(population).await {
                // receiver dropped
            }
        });
    }

    pub fn run_evolution(mut refinement_ctx: RefinementContext, operators: OperatorConfig) -> EvolutionResult {
        while !should_stop(&mut refinement_ctx, operators.termination.as_ref()) {
            // TODO determine from config
            let branches = 10_usize;
            let (branch_sender, mut _root_receiver) = mpsc::channel(1);
            let is_terminated = Arc::new(AtomicBool::new(false));

            (0..branches).for_each(|_| {
                create_branch(&refinement_ctx, operators.clone(), branch_sender.clone(), is_terminated.clone());
            });

            drop(branch_sender);
        }

        unimplemented!()
    }
}

#[cfg(target_arch = "wasm32")]
mod branches {}
