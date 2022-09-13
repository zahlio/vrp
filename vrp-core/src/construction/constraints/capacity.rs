#[cfg(test)]
#[path = "../../../tests/unit/construction/constraints/capacity_test.rs"]
mod capacity_test;

use crate::construction::constraints::*;
use crate::construction::extensions::{MultiTrip, NoMultiTrip};
use crate::construction::heuristics::*;
use crate::models::common::*;
use crate::models::problem::{Job, Single};
use crate::models::solution::Activity;
use std::iter::once;
use std::slice::Iter;
use std::sync::Arc;

/// A module which ensures vehicle capacity limitation while serving customer's demand.
pub struct CapacityConstraintModule<T: LoadOps> {
    code: i32,
    state_keys: Vec<i32>,
    conditional: ConditionalJobModule,
    constraints: Vec<ConstraintVariant>,
    multi_trip: Arc<dyn MultiTrip<Constraint = T> + Send + Sync>,
}

impl<T: LoadOps + 'static> CapacityConstraintModule<T> {
    /// Creates a new instance of `CapacityConstraintModule` without multi trip (reload) functionality
    pub fn new(code: i32) -> Self {
        Self::new_with_multi_trip(code, Arc::new(NoMultiTrip::default()))
    }

    /// Creates a new instance of `CapacityConstraintModule` with multi trip (reload) functionality
    pub fn new_with_multi_trip(code: i32, multi_trip: Arc<dyn MultiTrip<Constraint = T> + Send + Sync>) -> Self {
        Self {
            code,
            state_keys: vec![CURRENT_CAPACITY_KEY, MAX_FUTURE_CAPACITY_KEY, MAX_PAST_CAPACITY_KEY],
            conditional: ConditionalJobModule::new(Box::new(ConcreteJobContextTransition {
                remove_required: {
                    let multi_trip = multi_trip.clone();
                    move |_, _, job| multi_trip.is_marker_job(job)
                },
                promote_required: |_, _, _| false,
                remove_locked: |_, _, _| false,
                promote_locked: {
                    let multi_trip = multi_trip.clone();
                    move |_, _, job| multi_trip.is_marker_job(job)
                },
            })),
            constraints: vec![
                ConstraintVariant::SoftRoute(Arc::new(CapacitySoftRouteConstraint { multi_trip: multi_trip.clone() })),
                ConstraintVariant::HardRoute(Arc::new(CapacityHardRouteConstraint::<T> {
                    code,
                    multi_trip: multi_trip.clone(),
                })),
                ConstraintVariant::HardActivity(Arc::new(CapacityHardActivityConstraint::<T> {
                    code,
                    multi_trip: multi_trip.clone(),
                })),
            ],
            multi_trip,
        }
    }

    fn recalculate_states(&self, route_ctx: &mut RouteContext) {
        self.multi_trip.accept_route_state(route_ctx);
        let reload_intervals = self
            .multi_trip
            .get_marker_intervals(route_ctx)
            .cloned()
            .unwrap_or_else(|| vec![(0, route_ctx.route.tour.total() - 1)]);

        let (_, max_load) =
            reload_intervals.into_iter().fold((T::default(), T::default()), |(acc, max), (start_idx, end_idx)| {
                let (route, state) = route_ctx.as_mut();

                // determine static deliveries loaded at the begin and static pickups brought to the end
                let (start_delivery, end_pickup) = route.tour.activities_slice(start_idx, end_idx).iter().fold(
                    (acc, T::default()),
                    |acc, activity| {
                        Self::get_demand(activity)
                            .map(|demand| (acc.0 + demand.delivery.0, acc.1 + demand.pickup.0))
                            .unwrap_or_else(|| acc)
                    },
                );

                // determine actual load at each activity and max discovered in the past
                let (current, _) = route.tour.activities_slice(start_idx, end_idx).iter().fold(
                    (start_delivery, T::default()),
                    |(current, max), activity| {
                        let change =
                            Self::get_demand(activity).map(|demand| demand.change()).unwrap_or_else(T::default);

                        let current = current + change;
                        let max = max.max_load(current);

                        state.put_activity_state(CURRENT_CAPACITY_KEY, activity, current);
                        state.put_activity_state(MAX_PAST_CAPACITY_KEY, activity, max);

                        (current, max)
                    },
                );

                let current_max =
                    route.tour.activities_slice(start_idx, end_idx).iter().rev().fold(current, |max, activity| {
                        let max = max.max_load(*state.get_activity_state(CURRENT_CAPACITY_KEY, activity).unwrap());
                        state.put_activity_state(MAX_FUTURE_CAPACITY_KEY, activity, max);
                        max
                    });

                (current - end_pickup, current_max.max_load(max))
            });

        if let Some(capacity) = route_ctx.route.actor.clone().vehicle.dimens.get_capacity() {
            route_ctx.state_mut().put_route_state(MAX_LOAD_KEY, max_load.ratio(capacity));
        }
    }

    fn has_demand_violation(
        state: &RouteState,
        pivot: &Activity,
        capacity: Option<&T>,
        demand: Option<&Demand<T>>,
        stopped: bool,
    ) -> Option<bool> {
        if let Some(demand) = demand {
            if let Some(&capacity) = capacity {
                let default = T::default();

                // check how static delivery affect past max load
                if demand.delivery.0.is_not_empty() {
                    let past = *state.get_activity_state(MAX_PAST_CAPACITY_KEY, pivot).unwrap_or(&default);
                    if !capacity.can_fit(&(past + demand.delivery.0)) {
                        return Some(stopped);
                    }
                }

                // check how static pickup affect future max load
                if demand.pickup.0.is_not_empty() {
                    let future = *state.get_activity_state(MAX_FUTURE_CAPACITY_KEY, pivot).unwrap_or(&default);
                    if !capacity.can_fit(&(future + demand.pickup.0)) {
                        return Some(stopped);
                    }
                }

                // check dynamic load change
                let change = demand.change();
                if change.is_not_empty() {
                    let future = *state.get_activity_state(MAX_FUTURE_CAPACITY_KEY, pivot).unwrap_or(&default);
                    if !capacity.can_fit(&(future + change)) {
                        return Some(stopped);
                    }

                    let current = *state.get_activity_state(CURRENT_CAPACITY_KEY, pivot).unwrap_or(&default);
                    if !capacity.can_fit(&(current + change)) {
                        return Some(false);
                    }
                }

                None
            } else {
                Some(stopped)
            }
        } else {
            None
        }
    }

    fn can_handle_demand_on_intervals(
        ctx: &RouteContext,
        multi_trip: &(dyn MultiTrip<Constraint = T> + Send + Sync),
        demand: Option<&Demand<T>>,
        insert_idx: Option<usize>,
    ) -> bool {
        let has_demand_violation = |activity: &Activity| {
            CapacityConstraintModule::<T>::has_demand_violation(
                &ctx.state,
                activity,
                ctx.route.actor.vehicle.dimens.get_capacity(),
                demand,
                true,
            )
        };

        multi_trip
            .get_marker_intervals(ctx)
            .map(|intervals| {
                if let Some(insert_idx) = insert_idx {
                    intervals.iter().filter(|(_, end_idx)| insert_idx <= *end_idx).all(|interval| {
                        has_demand_violation(ctx.route.tour.get(insert_idx.max(interval.0)).unwrap()).is_none()
                    })
                } else {
                    intervals
                        .iter()
                        .any(|(start_idx, _)| has_demand_violation(ctx.route.tour.get(*start_idx).unwrap()).is_none())
                }
            })
            .unwrap_or_else(|| has_demand_violation(ctx.route.tour.get(insert_idx.unwrap_or(0)).unwrap()).is_none())
    }

    fn get_demand(activity: &Activity) -> Option<&Demand<T>> {
        activity.job.as_ref().and_then(|job| job.dimens.get_demand())
    }
}

impl<T: LoadOps> ConstraintModule for CapacityConstraintModule<T> {
    fn accept_insertion(&self, solution_ctx: &mut SolutionContext, route_index: usize, job: &Job) {
        self.accept_route_state(solution_ctx.routes.get_mut(route_index).unwrap());
        self.multi_trip.accept_insertion(solution_ctx, route_index, job, self.code);
    }

    fn accept_route_state(&self, ctx: &mut RouteContext) {
        self.recalculate_states(ctx);
    }

    fn accept_solution_state(&self, ctx: &mut SolutionContext) {
        self.conditional.accept_solution_state(ctx);
        ctx.routes.iter_mut().filter(|route_ctx| route_ctx.is_stale()).for_each(|route_ctx| {
            self.recalculate_states(route_ctx);
        });

        self.multi_trip.accept_solution_state(ctx);
    }

    fn merge(&self, source: Job, candidate: Job) -> Result<Job, i32> {
        if once(&source).chain(once(&candidate)).any(|job| self.multi_trip.is_marker_job(job)) {
            return Err(self.code);
        }

        match (&source, &candidate) {
            (Job::Single(s_source), Job::Single(s_candidate)) => {
                let source_demand: Option<&Demand<T>> = s_source.dimens.get_demand();
                let candidate_demand: Option<&Demand<T>> = s_candidate.dimens.get_demand();

                match (source_demand, candidate_demand) {
                    (None, None) | (Some(_), None) => Ok(source),
                    _ => {
                        let source_demand = source_demand.cloned().unwrap_or_default();
                        let candidate_demand = candidate_demand.cloned().unwrap_or_default();
                        let new_demand = source_demand + candidate_demand;

                        let mut dimens = s_source.dimens.clone();
                        dimens.set_demand(new_demand);

                        Ok(Job::Single(Arc::new(Single { places: s_source.places.clone(), dimens })))
                    }
                }
            }
            _ => Err(self.code),
        }
    }

    fn state_keys(&self) -> Iter<i32> {
        self.state_keys.iter()
    }

    fn get_constraints(&self) -> Iter<ConstraintVariant> {
        self.constraints.iter()
    }
}

struct CapacitySoftRouteConstraint<T: LoadOps> {
    multi_trip: Arc<dyn MultiTrip<Constraint = T> + Send + Sync>,
}

impl<T: LoadOps> SoftRouteConstraint for CapacitySoftRouteConstraint<T> {
    fn estimate_job(&self, _: &SolutionContext, ctx: &RouteContext, job: &Job) -> f64 {
        if self.multi_trip.is_marker_job(job) {
            0. - ctx.route.actor.vehicle.costs.fixed.max(1000.)
        } else {
            0.
        }
    }
}

/// Locks reload jobs to specific vehicles
struct CapacityHardRouteConstraint<T: LoadOps> {
    code: i32,
    multi_trip: Arc<dyn MultiTrip<Constraint = T> + Send + Sync>,
}

impl<T: LoadOps> HardRouteConstraint for CapacityHardRouteConstraint<T> {
    fn evaluate_job(&self, _: &SolutionContext, ctx: &RouteContext, job: &Job) -> Option<RouteConstraintViolation> {
        if self.multi_trip.is_marker_job(job) {
            return if self.multi_trip.is_assignable(&ctx.route, job) {
                None
            } else {
                Some(RouteConstraintViolation { code: self.code })
            };
        };

        let can_handle = match job {
            Job::Single(job) => CapacityConstraintModule::<T>::can_handle_demand_on_intervals(
                ctx,
                self.multi_trip.as_ref(),
                job.dimens.get_demand(),
                None,
            ),
            Job::Multi(job) => job.jobs.iter().any(|job| {
                CapacityConstraintModule::<T>::can_handle_demand_on_intervals(
                    ctx,
                    self.multi_trip.as_ref(),
                    job.dimens.get_demand(),
                    None,
                )
            }),
        };

        if can_handle {
            None
        } else {
            Some(RouteConstraintViolation { code: self.code })
        }
    }
}

struct CapacityHardActivityConstraint<T: LoadOps> {
    code: i32,
    multi_trip: Arc<dyn MultiTrip<Constraint = T> + Send + Sync>,
}

impl<T: LoadOps> HardActivityConstraint for CapacityHardActivityConstraint<T> {
    fn evaluate_activity(
        &self,
        route_ctx: &RouteContext,
        activity_ctx: &ActivityContext,
    ) -> Option<ActivityConstraintViolation> {
        if activity_ctx
            .target
            .job
            .as_ref()
            .map_or(false, |job| self.multi_trip.is_marker_job(&Job::Single(job.clone())))
        {
            // NOTE insert reload job in route only as last
            let is_first = activity_ctx.prev.job.is_none();
            let is_not_last = activity_ctx.next.as_ref().and_then(|next| next.job.as_ref()).is_some();

            return if is_first || is_not_last {
                Some(ActivityConstraintViolation { code: self.code, stopped: false })
            } else {
                None
            };
        };

        let demand = CapacityConstraintModule::<T>::get_demand(activity_ctx.target);

        let violation = if activity_ctx.target.retrieve_job().map_or(false, |job| job.as_multi().is_some()) {
            // NOTE multi job has dynamic demand which can go in another interval
            if CapacityConstraintModule::<T>::can_handle_demand_on_intervals(
                route_ctx,
                self.multi_trip.as_ref(),
                demand,
                Some(activity_ctx.index),
            ) {
                None
            } else {
                Some(false)
            }
        } else {
            CapacityConstraintModule::<T>::has_demand_violation(
                &route_ctx.state,
                activity_ctx.prev,
                route_ctx.route.actor.vehicle.dimens.get_capacity(),
                demand,
                !self.multi_trip.has_markers(route_ctx),
            )
        };

        violation.map(|stopped| ActivityConstraintViolation { code: self.code, stopped })
    }
}
