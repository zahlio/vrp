use crate::format::problem::*;
use crate::format::solution::*;
use crate::format_time;
use crate::helpers::*;

fn create_problem_with_dispatch(dispatch: Option<Vec<VehicleDispatch>>) -> Problem {
    Problem {
        plan: Plan {
            jobs: vec![create_delivery_job("job1", (3., 0.)), create_delivery_job("job2", (5., 0.))],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                shifts: vec![VehicleShift { dispatch, ..create_default_vehicle_shift() }],
                ..create_default_vehicle_type()
            }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    }
}

#[test]
fn can_assign_single_dispatch() {
    let problem = create_problem_with_dispatch(Some(vec![VehicleDispatch {
        location: (7., 0.).to_loc(),
        limits: vec![VehicleDispatchLimit { max: 1, start: format_time(10.), end: format_time(12.) }],
        tag: None,
    }]));
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert_eq!(
        solution,
        SolutionBuilder::default()
            .tour(
                TourBuilder::default()
                    .stops(vec![
                        StopBuilder::default()
                            .coordinate((0., 0.))
                            .schedule_stamp(0., 3.)
                            .load(vec![0])
                            .build_departure(),
                        StopBuilder::default()
                            .coordinate((7., 0.))
                            .schedule_stamp(10., 12.)
                            .load(vec![2])
                            .distance(7)
                            .build_single("dispatch", "dispatch"),
                        StopBuilder::default()
                            .coordinate((5., 0.))
                            .schedule_stamp(14., 15.)
                            .load(vec![1])
                            .distance(9)
                            .build_single("job2", "delivery"),
                        StopBuilder::default()
                            .coordinate((3., 0.))
                            .schedule_stamp(17., 18.)
                            .load(vec![0])
                            .distance(11)
                            .build_single("job1", "delivery"),
                        StopBuilder::default()
                            .coordinate((0., 0.))
                            .schedule_stamp(21., 21.)
                            .load(vec![0])
                            .distance(14)
                            .build_arrival(),
                    ])
                    .statistic(StatisticBuilder::default().driving(14).serving(4).build())
                    .build()
            )
            .build()
    );
}

#[test]
fn can_assign_dispatch_at_start() {
    let problem = create_problem_with_dispatch(Some(vec![VehicleDispatch {
        location: (0., 0.).to_loc(),
        limits: vec![VehicleDispatchLimit { max: 1, start: format_time(0.), end: format_time(2.) }],
        tag: None,
    }]));
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert!(solution.unassigned.is_none());
    assert_eq!(solution.tours.len(), 1);

    let first_stop = &solution.tours[0].stops[0];
    assert_eq!(first_stop.schedule().arrival, format_time(0.));
    assert_eq!(first_stop.schedule().departure, format_time(2.));
    assert_eq!(first_stop.activities().len(), 2);
    assert_eq!(first_stop.activities()[0].activity_type, "departure");
    assert_eq!(first_stop.activities()[1].activity_type, "dispatch");
}

#[test]
fn can_handle_unassignable_dispatch() {
    let problem = create_problem_with_dispatch(Some(vec![VehicleDispatch {
        location: (1001., 0.).to_loc(),
        limits: vec![VehicleDispatchLimit { max: 1, start: format_time(10.), end: format_time(12.) }],
        tag: None,
    }]));
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert!(solution.tours.is_empty());
    assert_eq!(solution.unassigned.map_or(0, |u| u.len()), 2);
}

parameterized_test! {can_handle_two_dispatch, (first_dispatch, second_dispatch, expected_location, expected_cost), {
    can_handle_two_dispatch_impl(first_dispatch, second_dispatch, expected_location, expected_cost);
}}

can_handle_two_dispatch! {
    case01: (((7., 0.), (7., 8.)), ((8., 0.), (8., 9.)), (7., 0.), 40.),
    case02: (((8., 0.), (8., 9.)), ((7., 0.), (7., 8.)), (7., 0.), 40.),
    case03: (((1001., 0.), (10., 11.)), ((8., 0.), (8., 9.)), (8., 0.), 44.),
}

fn can_handle_two_dispatch_impl(
    first_dispatch: ((f64, f64), (f64, f64)),
    second_dispatch: ((f64, f64), (f64, f64)),
    expected_location: (f64, f64),
    expected_cost: f64,
) {
    let problem = create_problem_with_dispatch(Some(vec![
        VehicleDispatch {
            location: first_dispatch.0.to_loc(),
            limits: vec![VehicleDispatchLimit {
                max: 1,
                start: format_time((first_dispatch.1).0),
                end: format_time((first_dispatch.1).0),
            }],
            tag: None,
        },
        VehicleDispatch {
            location: second_dispatch.0.to_loc(),
            limits: vec![VehicleDispatchLimit {
                max: 1,
                start: format_time((second_dispatch.1).0),
                end: format_time((second_dispatch.1).0),
            }],
            tag: None,
        },
    ]));
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert!(!solution.tours.is_empty());
    let second_stop = solution.tours[0].stops[1].as_point().unwrap();
    assert_eq!(second_stop.location, expected_location.to_loc());
    assert_eq!(second_stop.activities[0].activity_type, "dispatch");
    assert_eq!(solution.statistic.cost, expected_cost);
}

fn create_problem_with_dispatch_5jobs(vehicle_ids: Vec<&str>, dispatch: Option<Vec<VehicleDispatch>>) -> Problem {
    Problem {
        plan: Plan {
            jobs: vec![
                create_delivery_job("job1", (2., 0.)),
                create_delivery_job("job2", (2., 0.)),
                create_delivery_job("job3", (2., 0.)),
                create_delivery_job("job4", (2., 0.)),
                create_delivery_job("job5", (2., 0.)),
            ],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                vehicle_ids: vehicle_ids.iter().map(|id| id.to_string()).collect(),
                shifts: vec![VehicleShift { dispatch, ..create_default_vehicle_shift() }],
                capacity: vec![1],
                ..create_default_vehicle_type()
            }],
            ..create_default_fleet()
        },
        ..create_empty_problem()
    }
}

fn assert_tours(tours: &[Tour], values: (f64, f64, f64)) {
    tours.iter().for_each(|tour| {
        assert_eq!(tour.stops.len(), 4);

        let first_stop = tour.stops[0].as_point().unwrap();
        assert_eq!(first_stop.time.departure, format_time(values.0));
        assert_eq!(first_stop.activities.len(), 1);
        assert_eq!(first_stop.activities[0].activity_type, "departure");

        let second_stop = tour.stops[1].as_point().unwrap();
        assert_eq!(second_stop.activities.len(), 1);
        assert_eq!(second_stop.activities[0].activity_type, "dispatch");
        assert_eq!(second_stop.time.arrival, format_time(values.1));
        assert_eq!(second_stop.time.departure, format_time(values.2));
    });
}

#[test]
fn can_dispatch_multiple_vehicles_at_single_dispatch() {
    let problem = create_problem_with_dispatch_5jobs(
        vec!["v1", "v2", "v3", "v4", "v5"],
        Some(vec![VehicleDispatch {
            location: (1., 0.).to_loc(),
            limits: vec![
                VehicleDispatchLimit { max: 2, start: format_time(10.), end: format_time(12.) },
                VehicleDispatchLimit { max: 3, start: format_time(13.), end: format_time(16.) },
            ],
            tag: None,
        }]),
    );
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert!(solution.unassigned.is_none());
    assert_eq!(solution.tours.len(), 5);

    assert_tours(&solution.tours[0..2], (9., 10., 12.));
    assert_tours(&solution.tours[2..5], (12., 13., 16.));
}
