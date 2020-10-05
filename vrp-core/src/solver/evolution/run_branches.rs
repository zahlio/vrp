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
    use crate::solver::evolution::{should_add_solution, should_stop};
    use crate::solver::{DominancePopulation, Individual, Population, RefinementContext};
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

    fn get_best_individuals(refinement_ctx: &RefinementContext) -> Vec<Individual> {
        refinement_ctx
            .population
            .ranked()
            .filter_map(|(individual, rank)| if rank == 0 { Some(individual.deep_copy()) } else { None })
            .collect()
    }

    fn create_branch(
        refinement_ctx: &RefinementContext,
        operators: OperatorConfig,
        mut branch_sender: mpsc::Sender<Vec<Individual>>,
        is_terminated: Arc<AtomicBool>,
    ) {
        let mut population = DominancePopulation::new(refinement_ctx.problem.clone(), 4);
        population.add_all(get_best_individuals(refinement_ctx));

        let mut refinement_ctx = RefinementContext {
            problem: refinement_ctx.problem.clone(),
            population: Box::new(population),
            state: Default::default(),
            quota: Some(Arc::new(AtomicQuota { original: refinement_ctx.quota.clone(), is_terminated })),
            statistics: Default::default(),
        };

        tokio::spawn(async move {
            while !should_stop(&mut refinement_ctx, operators.termination.as_ref()) {
                let parents = operators.selection.select_parents(&refinement_ctx);
                let offspring = operators.mutation.mutate_all(&refinement_ctx, parents);

                if should_add_solution(&refinement_ctx) {
                    refinement_ctx.population.add_all(offspring);
                    let best_individuals = get_best_individuals(&refinement_ctx);

                    if let Err(_) = branch_sender.send(best_individuals).await {
                        return;
                    }
                }
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
