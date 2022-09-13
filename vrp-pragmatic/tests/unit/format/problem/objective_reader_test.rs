use crate::constraints::{TOTAL_VALUE_KEY, TOUR_ORDER_KEY};
use crate::format::problem::reader::objective_reader::*;
use crate::format::problem::reader::ProblemProperties;
use crate::helpers::create_empty_problem;
use crate::helpers::{create_empty_insertion_context, create_single_with_type};
use std::sync::Arc;
use vrp_core::construction::constraints::ConstraintPipeline;
use vrp_core::construction::heuristics::InsertionContext;
use vrp_core::models::problem::Job;
use vrp_core::rosomaxa::prelude::MultiObjective;
use vrp_core::solver::objectives::OrderResult;

fn create_problem_props() -> ProblemProperties {
    ProblemProperties {
        has_multi_dimen_capacity: false,
        has_breaks: false,
        has_skills: false,
        has_unreachable_locations: false,
        has_dispatch: false,
        has_reloads: false,
        has_order: false,
        has_group: false,
        has_compatibility: false,
        has_tour_size_limits: false,
        has_tour_travel_limits: false,
        max_job_value: None,
        max_area_value: None,
    }
}

fn create_solution_with_state_value<T: Send + Sync + 'static>(state_key: i32, value: T) -> InsertionContext {
    let mut insertion_ctx = create_empty_insertion_context();
    insertion_ctx.solution.state.insert(state_key, Arc::new(value));

    insertion_ctx
}

#[test]
fn can_define_proper_place_for_value_objective_by_default() {
    let problem = create_empty_problem();
    let mut constraint = ConstraintPipeline::default();
    let props = ProblemProperties { max_job_value: Some(1.), ..create_problem_props() };

    let objective_cost = create_objective(&problem, &mut constraint, &props);
    let objectives = objective_cost.objectives().collect::<Vec<_>>();

    assert_eq!(objectives[0].fitness(&create_solution_with_state_value(TOTAL_VALUE_KEY, 1234.)), 1234.);
}

#[test]
fn can_define_proper_place_for_order_objective_by_default() {
    let problem = create_empty_problem();
    let mut constraint = ConstraintPipeline::default();
    let props = ProblemProperties { has_order: true, ..create_problem_props() };

    let objective_cost = create_objective(&problem, &mut constraint, &props);
    let objectives = objective_cost.objectives().collect::<Vec<_>>();

    assert_eq!(objectives[1].fitness(&create_solution_with_state_value(TOUR_ORDER_KEY, 1234_usize)), 1234.);
}

#[test]
fn can_define_proper_places_for_mixed_priority_and_order_objectives_by_default() {
    let problem = create_empty_problem();
    let mut constraint = ConstraintPipeline::default();
    let mut insertion_ctx = create_empty_insertion_context();
    insertion_ctx.solution.state.insert(TOTAL_VALUE_KEY, Arc::new(123.));
    insertion_ctx.solution.state.insert(TOUR_ORDER_KEY, Arc::new(321_usize));

    let props = ProblemProperties { max_job_value: Some(1.), has_order: true, ..create_problem_props() };

    let objective_cost = create_objective(&problem, &mut constraint, &props);
    let objectives = objective_cost.objectives().collect::<Vec<_>>();

    assert_eq!(objectives[0].fitness(&insertion_ctx), 123.);
    assert_eq!(objectives[2].fitness(&insertion_ctx), 321.);
}

#[test]
fn can_get_default_unassigned_job_estimate() {
    let estimate_for_type = |activity_type: &str| {
        get_unassigned_job_estimate(&Job::Single(create_single_with_type("job1", activity_type)), 100., 1.)
    };

    assert_eq!(estimate_for_type("break"), 100.);
    assert_eq!(estimate_for_type("reload"), 0.);
    assert_eq!(estimate_for_type("dispatch"), 1.);
    assert_eq!(estimate_for_type("pickup"), 1.);
    assert_eq!(estimate_for_type("delivery"), 1.);
    assert_eq!(estimate_for_type("service"), 1.);
}

#[test]
fn can_get_default_order() {
    let compare_for_type = |activity_type: &str, expected: OrderResult| {
        let order_result = get_default_order(&create_single_with_type("job1", activity_type));
        match (order_result, expected) {
            (OrderResult::Ignored, OrderResult::Ignored) | (OrderResult::Default, OrderResult::Default) => {}
            _ => unreachable!(),
        };
    };

    compare_for_type("break", OrderResult::Ignored);
    compare_for_type("reload", OrderResult::Ignored);
    compare_for_type("dispatch", OrderResult::Ignored);
    compare_for_type("pickup", OrderResult::Default);
    compare_for_type("delivery", OrderResult::Default);
    compare_for_type("service", OrderResult::Default);
}
