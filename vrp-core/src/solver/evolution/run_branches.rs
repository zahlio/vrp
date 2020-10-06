use crate::solver::evolution::{EvolutionResult, EvolutionStrategy, OperatorConfig};
use crate::solver::{RefinementContext, Telemetry};

/// An evolution algorithm which run multiple branches (islands) on each generations.
pub struct RunBranches {}

impl Default for RunBranches {
    fn default() -> Self {
        Self {}
    }
}

impl EvolutionStrategy for RunBranches {
    fn run(
        &self,
        refinement_ctx: RefinementContext,
        operators: OperatorConfig,
        telemetry: Telemetry,
    ) -> EvolutionResult {
        branches::run_evolution(refinement_ctx, operators, telemetry)
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod branches {
    use super::*;
    use crate::construction::Quota;
    use crate::solver::evolution::{should_add_solution, should_stop};
    use crate::solver::{DominancePopulation, Individual, Population, Statistics};
    use crate::utils::{Timer, get_cpus};
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
        is_terminated: Arc<AtomicBool>,
        mut branch_sender: mpsc::Sender<Option<Vec<Individual>>>,
    ) {
        let mut population = DominancePopulation::new(refinement_ctx.problem.clone(), 4);
        population.add_all(get_best_individuals(refinement_ctx));

        let mut refinement_ctx = RefinementContext {
            problem: refinement_ctx.problem.clone(),
            population: Box::new(population),
            state: Default::default(),
            quota: Some(Arc::new(AtomicQuota { original: refinement_ctx.quota.clone(), is_terminated })),
            statistics: Statistics { generation: 1, improvement_all_ratio: 1., improvement_1000_ratio: 1. },
        };

        tokio::spawn(async move {
            while !should_stop(&mut refinement_ctx, operators.termination.as_ref()) {
                let parents = operators.selection.select_parents(&refinement_ctx);
                let offspring = operators.mutation.mutate_all(&refinement_ctx, parents);

                if should_add_solution(&refinement_ctx) {
                    let is_improved = refinement_ctx.population.add_all(offspring);

                    let best_individuals = if is_improved {
                        Some(get_best_individuals(&refinement_ctx))
                    } else {
                        None
                    };

                    if let Err(_) = branch_sender.send(best_individuals).await {
                        return;
                    }
                }
            }
        });
    }

    async fn collect_individuals(
        refinement_ctx: &mut RefinementContext,
        mut root_receiver: mpsc::Receiver<Option<Vec<Individual>>>,
    ) -> bool {
        let mut chunks = 0;
        let mut is_improved = false;
        while let Some(individuals) = root_receiver.recv().await {
            if let Some(individuals) = individuals {
                //is_improved = refinement_ctx.population.add_all(individuals);
            }
            chunks += 1;

            if chunks == 200 {
                break;
            }
        }

        is_improved
    }

    pub fn run_evolution(
        mut refinement_ctx: RefinementContext,
        operators: OperatorConfig,
        mut telemetry: Telemetry,
    ) -> EvolutionResult {
        tokio::runtime::Runtime::new().expect("cannot create async runtime").block_on(async move {
            while !should_stop(&mut refinement_ctx, operators.termination.as_ref()) {
                let branches = 32;
                let (branch_sender, root_receiver) = mpsc::channel(32);
                let is_terminated = Arc::new(AtomicBool::new(false));
                let generation_time = Timer::start();

                (0..branches).for_each(|_| {
                    create_branch(&refinement_ctx, operators.clone(), is_terminated.clone(), branch_sender.clone());
                });

                drop(branch_sender);

                let is_improved = collect_individuals(&mut refinement_ctx, root_receiver).await;

                telemetry.on_generation(&mut refinement_ctx, generation_time, is_improved);
            }

            telemetry.on_result(&refinement_ctx);

            Ok((refinement_ctx.population, telemetry.get_metrics()))
        })
    }
}

#[cfg(target_arch = "wasm32")]
mod branches {
    use super::*;

    pub fn run_evolution(
        refinement_ctx: RefinementContext,
        operators: OperatorConfig,
        telemetry: Telemetry,
    ) -> EvolutionResult {
        RunStraight::default().run(refinement_ctx, operatorus, telemetry)
    }
}
