use crate::solver::evolution::*;
use crate::solver::RefinementContext;
use crate::utils::Timer;

/// A simple evolution algorithm which maintains single population.
pub struct RunStraight {}

impl Default for RunStraight {
    fn default() -> Self {
        Self {}
    }
}

impl EvolutionStrategy for RunStraight {
    fn run(&self, refinement_ctx: RefinementContext, config: EvolutionConfig) -> EvolutionResult {
        let mut refinement_ctx = refinement_ctx;
        let mut config = config;

        while !should_stop(&mut refinement_ctx, config.termination.as_ref()) {
            let generation_time = Timer::start();

            let parents = config.selection.select_parents(&refinement_ctx);

            let offspring = config.mutation.mutate_all(&refinement_ctx, parents);

            let is_improved =
                if should_add_solution(&refinement_ctx) { refinement_ctx.population.add_all(offspring) } else { false };

            config.telemetry.on_generation(&mut refinement_ctx, generation_time, is_improved);
        }

        config.telemetry.on_result(&refinement_ctx);

        Ok((refinement_ctx.population, config.telemetry.get_metrics()))
    }
}
