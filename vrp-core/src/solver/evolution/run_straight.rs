use crate::solver::evolution::*;
use crate::solver::{RefinementContext, Telemetry};
use crate::utils::Timer;

/// A simple evolution algorithm which maintains single population.
pub struct RunStraight {}

impl Default for RunStraight {
    fn default() -> Self {
        Self {}
    }
}

impl EvolutionStrategy for RunStraight {
    fn run(
        &self,
        refinement_ctx: RefinementContext,
        operators: OperatorConfig,
        telemetry: Telemetry,
    ) -> EvolutionResult {
        let mut refinement_ctx = refinement_ctx;
        let mut telemetry = telemetry;

        while !should_stop(&mut refinement_ctx, operators.termination.as_ref()) {
            let generation_time = Timer::start();

            let parents = operators.selection.select_parents(&refinement_ctx);

            let offspring = operators.mutation.mutate_all(&refinement_ctx, parents);

            let is_improved =
                if should_add_solution(&refinement_ctx) { refinement_ctx.population.add_all(offspring) } else { false };

            telemetry.on_generation(&mut refinement_ctx, generation_time, is_improved);
        }

        telemetry.on_result(&refinement_ctx);

        Ok((refinement_ctx.population, telemetry.get_metrics()))
    }
}
