//! Problem domain models.

use crate::algorithms::nsga2::Objective;
use crate::construction::constraints::{ConstraintModule, JOB_ID_DIMEN_KEY, VEHICLE_ID_DIMEN_KEY};
use crate::construction::heuristics::InsertionContext;
use crate::models::common::{Dimensions, ValueDimension};
use std::sync::Arc;

mod costs;
pub use self::costs::*;

mod jobs;
pub use self::jobs::*;

mod fleet;
pub use self::fleet::*;

/// An actual objective on solution type.
pub type TargetObjective = Arc<dyn Objective<Solution = InsertionContext> + Send + Sync>;

/// An actual constraint.
pub type TargetConstraint = Arc<dyn ConstraintModule + Send + Sync>;

/// A trait to get or set job id.
pub trait JobIdDimension {
    /// Sets value as job id.
    fn set_job_id(&mut self, id: &str) -> &mut Self;
    /// Gets job id value if present.
    fn get_job_id(&self) -> Option<&String>;
}

impl JobIdDimension for Dimensions {
    fn set_job_id(&mut self, id: &str) -> &mut Self {
        self.set_value(JOB_ID_DIMEN_KEY, id.to_string());
        self
    }

    fn get_job_id(&self) -> Option<&String> {
        self.get_value(JOB_ID_DIMEN_KEY)
    }
}

/// A trait to get or set vehicle id.
pub trait VehicleIdDimension {
    /// Sets value as vehicle id.
    fn set_vehicle_id(&mut self, id: &str) -> &mut Self;
    /// Gets job vehicle value if present.
    fn get_vehicle_id(&self) -> Option<&String>;
}

impl VehicleIdDimension for Dimensions {
    fn set_vehicle_id(&mut self, id: &str) -> &mut Self {
        self.set_value(VEHICLE_ID_DIMEN_KEY, id.to_string());
        self
    }

    fn get_vehicle_id(&self) -> Option<&String> {
        self.get_value(VEHICLE_ID_DIMEN_KEY)
    }
}
