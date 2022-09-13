use heuristic_research::{draw_function_plots, solve_function, Axes};
use plotters::prelude::*;
use rosomaxa::utils::Environment;

fn main() {
    let function_name = "rosenbrock";

    let x = -2.;
    let z = -2.;
    let generations = 2000;
    let selection_size = 8;
    let population_type = "rosomaxa";
    let logger = Environment::default().logger;

    solve_function(function_name, population_type, selection_size, vec![x, z], generations, logger);

    let area = BitMapBackend::new("rosenbrock.png", (1024, 768)).into_drawing_area();
    let generation = 100;
    let pitch = 0.;
    let yaw = 0.;
    let axes = Axes { x: (-2.0..2.0, 0.15), y: (0.0..3610.), z: (-2.0..2.0, 0.15) };

    draw_function_plots(area, generation, pitch, yaw, axes, function_name).unwrap();
}
