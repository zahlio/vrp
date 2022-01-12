use crate::heuristics::evolution::*;
use crate::heuristics::termination::*;
use std::hash::Hash;
use std::sync::Arc;

/// A configuration which controls evolution execution.
pub struct EvolutionConfig<C, O, S>
where
    C: HeuristicContext<Objective = O, Solution = S>,
    O: HeuristicObjective<Solution = S>,
    S: HeuristicSolution,
{
    /// An initial solution config.
    pub initial: InitialConfig<C, O, S>,

    /// A hyper heuristic.
    pub heuristic: Box<dyn HyperHeuristic<Context = C, Solution = S>>,

    /// Evolution strategy.
    pub strategy: Box<dyn EvolutionStrategy<Context = C, Objective = O, Solution = S>>,

    /// Population algorithm.
    pub population: Box<dyn HeuristicPopulation<Objective = O, Individual = S>>,

    /// A termination defines when evolution should stop.
    pub termination: Box<dyn Termination<Context = C, Objective = O>>,

    /// An environmental context.
    pub environment: Arc<Environment>,

    /// A telemetry to be used.
    pub telemetry: Telemetry<C, O, S>,
}

/// An initial solutions configuration.
pub struct InitialConfig<C, O, S>
where
    C: HeuristicContext<Objective = O, Solution = S>,
    O: HeuristicObjective<Solution = S>,
    S: HeuristicSolution,
{
    /// Create methods to produce initial individuals.
    pub methods: Vec<(Arc<dyn HeuristicOperator<Context = C, Solution = S> + Send + Sync>, usize)>,
    /// Initial size of population to be generated.
    pub max_size: usize,
    /// Quota for initial solution generation.
    pub quota: f64,
    /// Initial individuals in population.
    pub individuals: Vec<S>,
}

/// Provides configurable way to build evolution configuration using fluent interface style.
pub struct Builder<C, O, S, K>
where
    C: HeuristicContext<Objective = O, Solution = S> + Stateful<Key = K>,
    O: HeuristicObjective<Solution = S>,
    S: HeuristicSolution,
    K: Hash + Eq + Clone + Send + Sync,
{
    /// A max amount generations in evolution.
    max_generations: Option<usize>,
    /// A max seconds to run evolution.
    max_time: Option<usize>,
    /// A variation coefficient parameters for termination criteria.
    min_cv: Option<(String, usize, f64, bool, K)>,
    /// An evolution configuration.
    config: EvolutionConfig<C, O, S>,
}

impl<C, O, S, K> Builder<C, O, S, K>
where
    C: HeuristicContext<Objective = O, Solution = S> + Stateful<Key = K>,
    O: HeuristicObjective<Solution = S>,
    S: HeuristicSolution,
    K: Hash + Eq + Clone + Send + Sync,
{
    /// Creates a new instance of `Builder` from mandatory arguments.
    pub fn new(
        heuristic: Box<dyn HyperHeuristic<Context = C, Solution = S>>,
        population: Box<dyn HeuristicPopulation<Objective = O, Individual = S> + Send + Sync>,
        strategy: Box<dyn EvolutionStrategy<Context = C, Objective = O, Solution = S> + Send + Sync>,
        termination: Box<dyn Termination<Context = C, Objective = O> + Send + Sync>,
        methods: Vec<(Arc<dyn HeuristicOperator<Context = C, Solution = S> + Send + Sync>, usize)>,
        environment: Arc<Environment>,
    ) -> Self {
        Self {
            max_generations: None,
            max_time: None,
            min_cv: None,
            config: EvolutionConfig {
                initial: InitialConfig { methods, max_size: 4, quota: 0.05, individuals: vec![] },
                heuristic,
                population,
                strategy,
                termination,
                environment,
                telemetry: Telemetry::new(TelemetryMode::None),
            },
        }
    }

    /// Sets telemetry. Default telemetry is set to do nothing.
    pub fn with_telemetry(mut self, telemetry: Telemetry<C, O, S>) -> Self {
        self.config.telemetry = telemetry;
        self
    }

    /// Sets max generations to be run by evolution. Default is 3000.
    pub fn with_max_generations(mut self, limit: Option<usize>) -> Self {
        self.max_generations = limit;
        self
    }

    /// Sets variation coefficient termination criteria. Default is None.
    pub fn with_min_cv(mut self, min_cv: Option<(String, usize, f64, bool)>, key: K) -> Self {
        self.min_cv = min_cv.map(|min_cv| (min_cv.0, min_cv.1, min_cv.2, min_cv.3, key));
        self
    }

    /// Sets max running time limit for evolution. Default is 300 seconds.
    pub fn with_max_time(mut self, limit: Option<usize>) -> Self {
        self.max_time = limit;
        self
    }

    /// Sets initial parameters used to construct initial population.
    pub fn with_initial(
        mut self,
        max_size: usize,
        quota: f64,
        methods: Vec<(Arc<dyn HeuristicOperator<Context = C, Solution = S> + Send + Sync>, usize)>,
    ) -> Self {
        self.config.telemetry.log("configured to use custom initial population parameters");

        self.config.initial.max_size = max_size;
        self.config.initial.quota = quota;
        self.config.initial.methods = methods;

        self
    }

    /// Sets initial solutions in population. Default is no solutions in population.
    pub fn with_init_solutions(mut self, solutions: Vec<S>, max_init_size: Option<usize>) -> Self {
        self.config.telemetry.log(
            format!(
                "provided {} initial solutions to start with, max init size: {}",
                solutions.len(),
                if let Some(max_init_size) = max_init_size { max_init_size.to_string() } else { "default".to_string() }
            )
            .as_str(),
        );

        if let Some(max_size) = max_init_size {
            self.config.initial.max_size = max_size;
        }
        self.config.initial.individuals = solutions;

        self
    }

    /// Sets termination algorithm. Default is max time and max generations.
    pub fn with_termination(mut self, termination: Box<dyn Termination<Context = C, Objective = O>>) -> Self {
        self.config.termination = termination;
        self
    }

    /// Builds an evolution config.
    pub fn build(self) -> Result<EvolutionConfig<C, O, S>, String> {
        let terminations: Vec<Box<dyn Termination<Context = C, Objective = O> + Send + Sync>> =
            match (self.max_generations, self.max_time, &self.min_cv) {
                (None, None, None) => {
                    self.config
                        .telemetry
                        .log("configured to use default max-generations (3000) and max-time (300secs)");
                    vec![Box::new(MaxGeneration::<C, O, S>::new(3000)), Box::new(MaxTime::<C, O, S>::new(300.))]
                }
                _ => {
                    let mut terminations: Vec<Box<dyn Termination<Context = C, Objective = O> + Send + Sync>> = vec![];

                    if let Some(limit) = self.max_generations {
                        self.config.telemetry.log(format!("configured to use max-generations: {}", limit).as_str());
                        terminations.push(Box::new(MaxGeneration::<C, O, S>::new(limit)))
                    }

                    if let Some((interval_type, value, threshold, is_global, key)) = self.min_cv {
                        self.config.telemetry.log(
                            format!(
                                "configured to use variation coefficient {} with sample: {}, threshold: {}",
                                interval_type, value, threshold
                            )
                            .as_str(),
                        );

                        let variation: Box<dyn Termination<Context = C, Objective = O> + Send + Sync> =
                            match interval_type.as_str() {
                                "sample" => Box::new(MinVariation::<C, O, S, K>::new_with_sample(
                                    value, threshold, is_global, key,
                                )),
                                "period" => Box::new(MinVariation::<C, O, S, K>::new_with_period(
                                    value, threshold, is_global, key,
                                )),
                                _ => return Err(format!("unknown variation interval type: {}", interval_type)),
                            };

                        terminations.push(variation)
                    }

                    terminations
                }
            };

        let mut config = self.config;
        config.termination = Box::new(CompositeTermination::new(terminations));

        Ok(config)
    }
}
