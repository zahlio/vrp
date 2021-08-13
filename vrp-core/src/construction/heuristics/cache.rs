//! Insertion cache logic.

use crate::construction::constraints::{
    ActivityConstraintViolation, ConstraintPipeline, RouteConstraintViolation, INSERTION_CACHE_KEY,
};
use crate::construction::heuristics::*;
use crate::models::common::Cost;
use crate::models::problem::{Actor, Job, Single};
use hashbrown::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Represents an entity to hold insertion cache.
pub struct InsertionCache<'a> {
    constraint: &'a ConstraintPipeline,
    solution: Option<&'a SolutionCache>,
    pub job: JobCache,
}

#[derive(Clone)]
pub struct SolutionCache {
    hard_route: HashMap<Arc<Actor>, HashMap<RouteCacheKey, Option<RouteConstraintViolation>>>,
    soft_route: HashMap<Arc<Actor>, HashMap<RouteCacheKey, Cost>>,
    hard_activity: HashMap<Arc<Actor>, HashMap<ActivityCacheKey, Option<ActivityConstraintViolation>>>,
    soft_activity: HashMap<Arc<Actor>, HashMap<ActivityCacheKey, Cost>>,
}

pub struct JobCache {
    hard_route: Option<HashMap<RouteCacheKey, Option<RouteConstraintViolation>>>,
    soft_route: Option<HashMap<RouteCacheKey, Cost>>,
    hard_activity: Option<HashMap<ActivityCacheKey, Option<ActivityConstraintViolation>>>,
    soft_activity: Option<HashMap<ActivityCacheKey, Cost>>,
}

impl Default for SolutionCache {
    fn default() -> Self {
        Self {
            hard_route: Default::default(),
            soft_route: Default::default(),
            hard_activity: Default::default(),
            soft_activity: Default::default(),
        }
    }
}

impl SolutionCache {
    pub fn clone_only_with(&self, actors: &HashSet<Arc<Actor>>) -> SolutionCache {
        Self {
            hard_route: clone_only_with(actors, &self.hard_route),
            soft_route: clone_only_with(actors, &self.soft_route),
            hard_activity: clone_only_with(actors, &self.hard_activity),
            soft_activity: clone_only_with(actors, &self.soft_activity),
        }
    }
}

impl Default for JobCache {
    fn default() -> Self {
        Self { hard_route: None, soft_route: None, hard_activity: None, soft_activity: None }
    }
}

impl<'a> InsertionCache<'a> {
    /// Creates insertion cache without underlying data.
    pub fn empty(constraint: &'a ConstraintPipeline) -> Self {
        Self { constraint, solution: None, job: JobCache::default() }
    }

    /// Creates insertion cache with underlying data if it exists.
    pub fn new(insertion_ctx: &'a InsertionContext) -> Self {
        Self {
            constraint: insertion_ctx.problem.constraint.as_ref(),
            solution: Some(&insertion_ctx.solution.cache),
            job: JobCache::default(),
        }
    }

    pub(crate) fn ensure_cache(insertion_ctx: &mut InsertionContext) {
        insertion_ctx.solution.state.entry(INSERTION_CACHE_KEY).or_insert_with(|| Arc::new(SolutionCache::default()));
    }

    pub(crate) fn synchronize(insertion_ctx: &mut InsertionContext, job: JobCache) {
        let solution = &mut insertion_ctx.solution.cache;

        sync_maps(&mut solution.hard_route, job.hard_route, &|key| key.0.clone());
        sync_maps(&mut solution.soft_route, job.soft_route, &|key| key.0.clone());
        sync_maps(&mut solution.hard_activity, job.hard_activity, &|key| key.0.clone());
        sync_maps(&mut solution.soft_activity, job.soft_activity, &|key| key.0.clone());
    }

    pub(crate) fn remove(insertion_ctx: &mut InsertionContext, actor: &Arc<Actor>) {
        let solution = &mut insertion_ctx.solution.cache;

        solution.hard_route.remove(actor);
        solution.soft_route.remove(actor);
        solution.hard_activity.remove(actor);
        solution.soft_activity.remove(actor);
    }

    /// Merges caches into one.
    pub fn merge(left: Self, right: Self) -> Self {
        Self {
            constraint: left.constraint,
            solution: left.solution,
            job: JobCache {
                hard_route: merge_maps(left.job.hard_route, right.job.hard_route),
                soft_route: merge_maps(left.job.soft_route, right.job.soft_route),
                hard_activity: merge_maps(left.job.hard_activity, right.job.hard_activity),
                soft_activity: merge_maps(left.job.soft_activity, right.job.soft_activity),
            },
        }
    }

    /// Evaluates hard route constraint and memorizes its result.
    pub fn evaluate_hard_route(
        &mut self,
        solution_ctx: &SolutionContext,
        route_ctx: &RouteContext,
        job: &Job,
    ) -> Option<RouteConstraintViolation> {
        let actor = &route_ctx.route.actor;
        let key = self.get_route_cache_key(route_ctx, job);

        if let Some(result) =
            self.solution.and_then(|solution| solution.hard_route.get(actor).and_then(|cache| cache.get(&key)))
        {
            result.clone()
        } else {
            let result = self.constraint.evaluate_hard_route(solution_ctx, route_ctx, job);
            self.job.hard_route.get_or_insert_with(|| HashMap::with_capacity(16)).insert(key, result.clone());
            result
        }
    }

    /// Evaluates soft route constraint and memorizes its result.
    pub fn evaluate_soft_route(&mut self, solution_ctx: &SolutionContext, route_ctx: &RouteContext, job: &Job) -> Cost {
        let actor = &route_ctx.route.actor;
        let key = self.get_route_cache_key(route_ctx, job);

        if let Some(result) =
            self.solution.and_then(|solution| solution.soft_route.get(actor).and_then(|cache| cache.get(&key)))
        {
            *result
        } else {
            let result = self.constraint.evaluate_soft_route(solution_ctx, route_ctx, job);
            self.job.soft_route.get_or_insert_with(|| HashMap::with_capacity(16)).insert(key, result);
            result
        }
    }

    /// Evaluates hard activity constraint and memorizes its result.
    pub fn evaluate_hard_activity(
        &mut self,
        route_ctx: &RouteContext,
        activity_ctx: &ActivityContext,
    ) -> Option<ActivityConstraintViolation> {
        let result = self.get_activity_cache_key(route_ctx, activity_ctx).map(|key| {
            (
                self.solution.and_then(|solution| {
                    solution.hard_activity.get(&route_ctx.route.actor).and_then(|cache| cache.get(&key))
                }),
                key,
            )
        });

        match result {
            Some((Some(result), _)) => result.clone(),
            Some((None, key)) => {
                let result = self.constraint.evaluate_hard_activity(route_ctx, activity_ctx);
                self.job.hard_activity.get_or_insert_with(|| HashMap::with_capacity(16)).insert(key, result.clone());
                result
            }
            _ => self.constraint.evaluate_hard_activity(route_ctx, activity_ctx),
        }
    }

    /// Evaluates soft activity constraint and memorizes its result.
    pub fn evaluate_soft_activity(&mut self, route_ctx: &RouteContext, activity_ctx: &ActivityContext) -> Cost {
        let result = self.get_activity_cache_key(route_ctx, activity_ctx).map(|key| {
            (
                self.solution.and_then(|solution| {
                    solution.soft_activity.get(&route_ctx.route.actor).and_then(|cache| cache.get(&key))
                }),
                key,
            )
        });

        match result {
            Some((Some(result), _)) => result.clone(),
            Some((None, key)) => {
                let result = self.constraint.evaluate_soft_activity(route_ctx, activity_ctx);
                self.job.soft_activity.get_or_insert_with(|| HashMap::with_capacity(16)).insert(key, result);
                result
            }
            _ => self.constraint.evaluate_soft_activity(route_ctx, activity_ctx),
        }
    }

    /// Gets used constraint pipeline.
    pub fn get_constraint(&self) -> &'a ConstraintPipeline {
        self.constraint
    }

    fn get_route_cache_key(&self, route_ctx: &RouteContext, job: &Job) -> RouteCacheKey {
        RouteCacheKey(route_ctx.route.actor.clone(), job.clone())
    }

    fn get_activity_cache_key(
        &self,
        route_ctx: &RouteContext,
        activity_ctx: &ActivityContext,
    ) -> Option<ActivityCacheKey> {
        activity_ctx.target.retrieve_job().zip(activity_ctx.target.job.as_ref()).map(|(job, single)| {
            ActivityCacheKey(route_ctx.route.actor.clone(), job, single.clone(), activity_ctx.position.clone())
        })
    }
    /*
    fn retrieve_solution_cache(insertion_ctx: &mut InsertionContext) -> &mut SolutionCache {
        insertion_ctx
            .solution
            .state
            .get(&INSERTION_CACHE_KEY)
            .and_then(|s| s.downcast_ref::<SolutionCache>())
            .map(|s| unsafe { as_mut(s) })
            .expect("expect cache")
    }*/
}

/// Represents a named tuple: actor, job.
#[derive(Clone)]
struct RouteCacheKey(pub Arc<Actor>, pub Job);

/// Represents a named tuple: actor, job, its sub-job, po.
#[derive(Clone)]
struct ActivityCacheKey(pub Arc<Actor>, pub Job, pub Arc<Single>, ActivityPosition);

impl Eq for RouteCacheKey {}

impl PartialEq<RouteCacheKey> for RouteCacheKey {
    fn eq(&self, other: &RouteCacheKey) -> bool {
        self.0.eq(&other.0) && self.1.eq(&other.1)
    }
}

impl Hash for RouteCacheKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
        self.1.hash(state);
    }
}

impl Eq for ActivityCacheKey {}

impl PartialEq<ActivityCacheKey> for ActivityCacheKey {
    fn eq(&self, other: &ActivityCacheKey) -> bool {
        self.0.eq(&other.0)
            && self.1.eq(&other.1)
            && std::ptr::eq(self.2.as_ref() as *const Single, other.2.as_ref() as *const Single)
            && self.3 == other.3
    }
}

impl Hash for ActivityCacheKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
        self.1.hash(state);
        (self.2.as_ref() as *const Single).hash(state);
        self.3.hash(state);
    }
}

fn merge_maps<K, V>(left: Option<HashMap<K, V>>, right: Option<HashMap<K, V>>) -> Option<HashMap<K, V>>
where
    K: Eq + Hash,
{
    match (left, right) {
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (Some(mut left), Some(right)) => {
            left.extend(right);
            Some(left)
        }
        (None, None) => None,
    }
}

fn sync_maps<K, V>(
    destination: &mut HashMap<Arc<Actor>, HashMap<K, V>>,
    other: Option<HashMap<K, V>>,
    get_actor: &dyn Fn(&K) -> Arc<Actor>,
) where
    K: Eq + Hash,
{
    other.into_iter().flat_map(|other| other.into_iter()).for_each(|(key, value)| {
        destination.entry(get_actor(&key)).or_insert_with(|| HashMap::with_capacity(128)).insert(key, value);
    });
}

fn clone_only_with<K, V>(
    actors: &HashSet<Arc<Actor>>,
    data: &HashMap<Arc<Actor>, HashMap<K, V>>,
) -> HashMap<Arc<Actor>, HashMap<K, V>>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    data.iter().filter(|(key, _)| actors.contains(*key)).map(|(key, value)| (key.clone(), value.clone())).collect()
}
