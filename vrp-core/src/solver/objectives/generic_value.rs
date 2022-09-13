use crate::construction::constraints::*;
use crate::construction::heuristics::{InsertionContext, RouteContext, SolutionContext};
use crate::models::problem::{Job, TargetConstraint, TargetObjective};
use rosomaxa::prelude::*;
use std::cmp::Ordering;
use std::ops::Deref;
use std::slice::Iter;
use std::sync::Arc;

/// Job merge function.
pub type JobMergeFn = Arc<dyn Fn(Job, Job) -> Result<Job, i32> + Send + Sync>;
/// Route value function.
pub type RouteValueFn = Arc<dyn Fn(&RouteContext) -> f64 + Send + Sync>;
/// Solution value function.
pub type SolutionValueFn = Arc<dyn Fn(&SolutionContext) -> f64 + Send + Sync>;
/// Estimate value function.
pub type EstimateValueFn = Arc<dyn Fn(&SolutionContext, &RouteContext, &Job, f64) -> f64 + Send + Sync>;

/// A helper for building objective functions.
pub struct GenericValue {}

impl GenericValue {
    /// Creates a new instance of constraint and related objective.
    pub fn new_constrained_objective(
        threshold: Option<f64>,
        job_merge_fn: JobMergeFn,
        route_value_fn: RouteValueFn,
        solution_value_fn: SolutionValueFn,
        estimate_value_fn: EstimateValueFn,
        state_key: i32,
    ) -> (TargetConstraint, TargetObjective) {
        let objective = GenericValueObjective {
            threshold,
            state_key,
            route_value_fn: route_value_fn.clone(),
            solution_value_fn: solution_value_fn.clone(),
            estimate_value_fn,
        };

        let constraint = GenericValueConstraint {
            constraints: vec![ConstraintVariant::SoftRoute(Arc::new(objective.clone()))],
            job_merge_fn,
            route_value_fn,
            state_key,
            keys: vec![state_key],
            solution_value_fn,
        };

        (Arc::new(constraint), Arc::new(objective))
    }
}

struct GenericValueConstraint {
    constraints: Vec<ConstraintVariant>,
    job_merge_fn: JobMergeFn,
    route_value_fn: RouteValueFn,
    solution_value_fn: SolutionValueFn,
    state_key: i32,
    keys: Vec<i32>,
}

impl ConstraintModule for GenericValueConstraint {
    fn accept_insertion(&self, solution_ctx: &mut SolutionContext, route_index: usize, _job: &Job) {
        self.accept_route_state(solution_ctx.routes.get_mut(route_index).unwrap());
    }

    fn accept_route_state(&self, ctx: &mut RouteContext) {
        let value = self.route_value_fn.deref()(ctx);

        ctx.state_mut().put_route_state(self.state_key, value);
    }

    fn accept_solution_state(&self, ctx: &mut SolutionContext) {
        let value = self.solution_value_fn.deref()(ctx);

        ctx.state.insert(self.state_key, Arc::new(value));
    }

    fn merge(&self, source: Job, candidate: Job) -> Result<Job, i32> {
        self.job_merge_fn.deref()(source, candidate)
    }

    fn state_keys(&self) -> Iter<i32> {
        self.keys.iter()
    }

    fn get_constraints(&self) -> Iter<ConstraintVariant> {
        self.constraints.iter()
    }
}

#[derive(Clone)]
struct GenericValueObjective {
    threshold: Option<f64>,
    state_key: i32,
    route_value_fn: RouteValueFn,
    solution_value_fn: SolutionValueFn,
    estimate_value_fn: EstimateValueFn,
}

impl SoftRouteConstraint for GenericValueObjective {
    fn estimate_job(&self, solution_ctx: &SolutionContext, route_ctx: &RouteContext, job: &Job) -> f64 {
        let value = route_ctx
            .state
            .get_route_state::<f64>(self.state_key)
            .cloned()
            .unwrap_or_else(|| self.route_value_fn.deref()(route_ctx));

        if value.is_finite() && self.threshold.map_or(true, |threshold| value > threshold) {
            self.estimate_value_fn.deref()(solution_ctx, route_ctx, job, value)
        } else {
            0.
        }
    }
}

impl Objective for GenericValueObjective {
    type Solution = InsertionContext;

    fn total_order(&self, a: &Self::Solution, b: &Self::Solution) -> Ordering {
        let fitness_a = self.fitness(a);
        let fitness_b = self.fitness(b);

        // TODO test it
        /*        if let Some(tolerance) = self.tolerance {
                    if (fitness_a - fitness_b).abs() < tolerance {
                        return Ordering::Equal;
                    }
                }
        */
        if let Some(threshold) = self.threshold {
            if fitness_a < threshold && fitness_b < threshold {
                return Ordering::Equal;
            }

            if fitness_a < threshold {
                return Ordering::Less;
            }

            if fitness_b < threshold {
                return Ordering::Greater;
            }
        }

        compare_floats(fitness_a, fitness_b)
    }

    fn fitness(&self, solution: &Self::Solution) -> f64 {
        solution
            .solution
            .state
            .get(&self.state_key)
            .and_then(|s| s.downcast_ref::<f64>())
            .cloned()
            .unwrap_or_else(|| self.solution_value_fn.deref()(&solution.solution))
    }
}
