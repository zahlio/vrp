//! Provides way to insert recharge stations in the tour to recharge (refuel) vehicle.

#[cfg(test)]
#[path = "../../../tests/unit/construction/features/recharge_test.rs"]
mod recharge_test;

use super::*;
use crate::construction::enablers::*;
use std::sync::Arc;
use vrp_core::construction::enablers::*;
use vrp_core::construction::features::*;

/// Specifies a distance limit function for recharge. It should return a fixed value for the same
/// actor all the time.
pub type RechargeDistanceLimitFn = Arc<dyn Fn(&Actor) -> Option<Distance> + Send + Sync>;

/// Creates a feature to insert charge stations along the route.
pub fn create_recharge_feature(
    name: &str,
    code: ViolationCode,
    distance_limit_fn: RechargeDistanceLimitFn,
    transport: Arc<dyn TransportCost + Send + Sync>,
) -> Result<Feature, GenericError> {
    create_multi_trip_feature(
        name,
        code,
        &[RECHARGE_DISTANCE_KEY, RECHARGE_INTERVALS_KEY],
        Arc::new(RechargeableMultiTrip {
            route_intervals: Arc::new(FixedReloadIntervals {
                is_marker_single_fn: Box::new(is_recharge_single),
                is_new_interval_needed_fn: Box::new({
                    let distance_limit_fn = distance_limit_fn.clone();
                    move |route_ctx| {
                        route_ctx
                            .route()
                            .tour
                            .end()
                            .map(|end| {
                                let current: Distance = route_ctx
                                    .state()
                                    .get_activity_state(RECHARGE_DISTANCE_KEY, end)
                                    .copied()
                                    .unwrap_or_default();

                                (distance_limit_fn)(route_ctx.route().actor.as_ref())
                                    .map_or(false, |threshold| current > threshold)
                            })
                            .unwrap_or(false)
                    }
                }),
                is_obsolete_interval_fn: Box::new({
                    let distance_limit_fn = distance_limit_fn.clone();
                    move |route_ctx, left, right| {
                        let intervals_distance = route_ctx
                            .route()
                            .tour
                            .get(left.end)
                            .iter()
                            .chain(route_ctx.route().tour.get(right.end).iter())
                            .flat_map(|activity| route_ctx.state().get_activity_state(RELOAD_RESOURCE_KEY, activity))
                            .sum::<Distance>();

                        (distance_limit_fn)(route_ctx.route().actor.as_ref())
                            .map_or(false, |threshold| intervals_distance < threshold)
                    }
                }),
                is_assignable_fn: Box::new(|route, job| {
                    job.as_single().map_or(false, |job| {
                        is_correct_vehicle(route, get_vehicle_id_from_job(job), get_shift_index(&job.dimens))
                    })
                }),
                intervals_key: RECHARGE_INTERVALS_KEY,
            }),
            transport,
            code,
            distance_state_key: RECHARGE_DISTANCE_KEY,
            distance_limit_fn,
        }),
    )
}

struct RechargeableMultiTrip {
    route_intervals: Arc<dyn RouteIntervals + Send + Sync>,
    transport: Arc<dyn TransportCost + Send + Sync>,
    code: ViolationCode,
    distance_state_key: StateKey,
    distance_limit_fn: RechargeDistanceLimitFn,
}

impl MultiTrip for RechargeableMultiTrip {
    fn get_route_intervals(&self) -> &(dyn RouteIntervals) {
        self.route_intervals.as_ref()
    }

    fn get_constraint(&self) -> &(dyn FeatureConstraint) {
        self
    }

    fn recalculate_states(&self, route_ctx: &mut RouteContext) {
        if (self.distance_limit_fn)(route_ctx.route().actor.as_ref()).is_none() {
            return;
        }

        let marker_intervals = self
            .route_intervals
            .get_marker_intervals(route_ctx)
            .cloned()
            .unwrap_or_else(|| vec![(0, route_ctx.route().tour.total() - 1)]);

        marker_intervals.into_iter().for_each(|(start_idx, end_idx)| {
            let (route, state) = route_ctx.as_mut();

            let _ = route
                .tour
                .activities_slice(start_idx, end_idx)
                .windows(2)
                .filter_map(|leg| match leg {
                    [prev, next] => Some((prev, next)),
                    _ => None,
                })
                .fold(Distance::default(), |acc, (prev, next)| {
                    let distance = self.transport.distance(
                        route,
                        prev.place.location,
                        next.place.location,
                        TravelTime::Departure(prev.schedule.departure),
                    );
                    let counter = acc + distance;

                    state.put_activity_state(self.distance_state_key, next, counter);

                    counter
                });
        });
    }
}

impl FeatureConstraint for RechargeableMultiTrip {
    fn evaluate(&self, move_ctx: &MoveContext<'_>) -> Option<ConstraintViolation> {
        match move_ctx {
            MoveContext::Route { route_ctx, job, .. } => self.evaluate_job(route_ctx, job),
            MoveContext::Activity { route_ctx, activity_ctx } => self.evaluate_activity(route_ctx, activity_ctx),
        }
    }

    fn merge(&self, source: Job, _: Job) -> Result<Job, ViolationCode> {
        Ok(source)
    }
}

impl RechargeableMultiTrip {
    fn evaluate_job(&self, _: &RouteContext, _: &Job) -> Option<ConstraintViolation> {
        ConstraintViolation::success()
    }

    fn evaluate_activity(
        &self,
        route_ctx: &RouteContext,
        activity_ctx: &ActivityContext,
    ) -> Option<ConstraintViolation> {
        let threshold = (self.distance_limit_fn)(route_ctx.route().actor.as_ref())?;

        let is_prev_recharge = activity_ctx.prev.job.as_ref().map_or(false, |job| is_recharge_single(job));
        let current_distance = if is_prev_recharge {
            // NOTE ignore current_distance for prev if prev is marker job as we store
            //      accumulated distance here to simplify obsolete intervals calculations
            Distance::default()
        } else {
            route_ctx
                .state()
                .get_activity_state::<Distance>(self.distance_state_key, activity_ctx.prev)
                .copied()
                .unwrap_or(Distance::default())
        };

        let (prev_to_next_distance, _) = calculate_travel(route_ctx, activity_ctx, self.transport.as_ref());

        if current_distance + prev_to_next_distance > threshold {
            ConstraintViolation::skip(self.code)
        } else {
            None
        }
    }
}

fn is_recharge_single(single: &Single) -> bool {
    single.dimens.get_job_type().map_or(false, |t| t == "recharge")
}