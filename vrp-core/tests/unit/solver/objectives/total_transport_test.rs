use crate::construction::heuristics::{InsertionContext, RouteContext, RouteState, SolutionContext, UnassignmentInfo};
use crate::helpers::construction::constraints::create_constraint_pipeline_with_transport;
use crate::helpers::models::domain::{create_empty_solution_context, create_registry_context};
use crate::helpers::models::problem::*;
use crate::helpers::models::solution::*;
use crate::models::common::Schedule;
use crate::models::problem::{Job, Jobs, ProblemObjective, SimpleActivityCost};
use crate::models::{Extras, Problem};
use crate::solver::objectives::TotalCost;
use hashbrown::HashMap;
use rosomaxa::prelude::Environment;
use std::sync::Arc;

#[test]
fn can_calculate_transport_cost() {
    let fleet = Arc::new(
        FleetBuilder::default()
            .add_driver(test_driver())
            .add_vehicle(VehicleBuilder::default().id("v1").costs(fixed_costs()).build())
            .add_vehicle(VehicleBuilder::default().id("v2").costs(fixed_costs()).build())
            .build(),
    );
    let route1 = RouteContext::new_with_state(
        Arc::new(create_route_with_start_end_activities(
            &fleet,
            "v1",
            test_activity_with_schedule(Schedule::new(0., 0.)),
            test_activity_with_schedule(Schedule::new(40., 40.)),
            vec![test_activity_with_location_and_duration(10, 5.), test_activity_with_location_and_duration(15, 5.)],
        )),
        Arc::new(RouteState::default()),
    );
    let route2 = RouteContext::new_with_state(
        Arc::new(create_route_with_start_end_activities(
            &fleet,
            "v2",
            test_activity_with_schedule(Schedule::new(0., 0.)),
            test_activity_with_schedule(Schedule::new(11., 11.)),
            vec![test_activity_with_location_and_duration(5, 1.)],
        )),
        Arc::new(RouteState::default()),
    );
    let activity = Arc::new(SimpleActivityCost::default());
    let transport = TestTransportCost::new_shared();
    let constraint = Arc::new(create_constraint_pipeline_with_transport());
    let mut unassigned = HashMap::new();
    unassigned.insert(Job::Single(Arc::new(test_single())), UnassignmentInfo::Simple(1));
    let problem = Arc::new(Problem {
        fleet: fleet.clone(),
        jobs: Arc::new(Jobs::new(&fleet, vec![], &transport)),
        locks: vec![],
        constraint: constraint.clone(),
        activity,
        transport,
        objective: Arc::new(ProblemObjective::default()),
        extras: Arc::new(Extras::default()),
    });
    let mut insertion_ctx = InsertionContext {
        problem,
        solution: SolutionContext {
            unassigned,
            routes: vec![route1, route2],
            registry: create_registry_context(&fleet),
            ..create_empty_solution_context()
        },
        environment: Arc::new(Environment::default()),
    };
    constraint.accept_solution_state(&mut insertion_ctx.solution);

    // vehicle + driver

    // route 1:
    // locations: 0 10 15 0
    // time: 40
    // driving: 30
    // fixed: 100

    // route 2:
    // locations: 0 5 0
    // time: 11
    // driving: 10
    // fixed: 100

    // total: (70 * 2 + 100) + (21 * 2 + 100) = 382

    let result = TotalCost::minimize().fitness(&insertion_ctx);

    assert_eq!(result.round(), 382.0);
}
