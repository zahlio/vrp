//! Contains implementation of extra constraints.

use std::sync::Arc;
use vrp_core::construction::heuristics::RouteContext;
use vrp_core::models::common::{Dimensions, ValueDimension};
use vrp_core::models::problem::{Single, VehicleIdDimension};
use vrp_core::models::solution::{Activity, Route};

// region state keys

/// A key which tracks job group state.
pub const GROUP_KEY: i32 = 1000;
pub const COMPATIBILITY_KEY: i32 = 1001;

// endregion

// region dimension keys

/// A key to track ids.
pub const JOB_TYPE_DIMEN_KEY: i32 = 1001;
pub const VEHICLE_SHIFT_INDEX_DIMEN_KEY: i32 = 1003;
pub const VEHICLE_TYPE_ID_DIMEN_KEY: i32 = 1004;
pub const BREAK_POLICY_DIMEN_KEY: i32 = 1005;
pub const SKILLS_DIMEN_KEY: i32 = 1006;
pub const GROUP_DIMEN_KEY: i32 = 1007;
pub const TAGS_DIMEN_KEY: i32 = 1008;
pub const COMPATIBILITY_DIMEN_KEY: i32 = 1009;
pub const TOUR_SIZE_DIMEN_KEY: i32 = 1010;

// endregion

/// A trait to get or set job type.
pub trait JobTypeDimension {
    /// Sets job type.
    fn set_job_type(&mut self, id: &str) -> &mut Self;
    /// Gets job type if present.
    fn get_job_type(&self) -> Option<&String>;
}

impl JobTypeDimension for Dimensions {
    fn set_job_type(&mut self, id: &str) -> &mut Self {
        self.set_value(JOB_TYPE_DIMEN_KEY, id.to_string());
        self
    }

    fn get_job_type(&self) -> Option<&String> {
        self.get_value(JOB_TYPE_DIMEN_KEY)
    }
}

fn as_single_job<F>(activity: &Activity, condition: F) -> Option<&Arc<Single>>
where
    F: Fn(&Arc<Single>) -> bool,
{
    activity.job.as_ref().and_then(|job| if condition(job) { Some(job) } else { None })
}

fn get_shift_index(dimens: &Dimensions) -> usize {
    *dimens.get_value::<usize>(VEHICLE_SHIFT_INDEX_DIMEN_KEY).unwrap()
}

fn get_vehicle_id_from_job(job: &Arc<Single>) -> Option<&String> {
    job.dimens.get_vehicle_id()
}

fn is_correct_vehicle(route: &Route, target_id: &str, target_shift: usize) -> bool {
    route.actor.vehicle.dimens.get_vehicle_id().unwrap() == target_id
        && get_shift_index(&route.actor.vehicle.dimens) == target_shift
}

fn is_single_belongs_to_route(ctx: &RouteContext, single: &Arc<Single>) -> bool {
    let vehicle_id = get_vehicle_id_from_job(single).unwrap();
    let shift_index = get_shift_index(&single.dimens);

    is_correct_vehicle(&ctx.route, vehicle_id, shift_index)
}

mod breaks;
pub use self::breaks::{BreakModule, BreakPolicy};

mod compatibility;
pub use self::compatibility::CompatibilityModule;

mod dispatch;
pub use self::dispatch::DispatchModule;

mod groups;
pub use self::groups::GroupModule;

mod reloads;
pub use self::reloads::ReloadMultiTrip;

mod reachable;
pub use self::reachable::ReachableModule;

mod skills;
pub use self::skills::JobSkills;
pub use self::skills::SkillsModule;
