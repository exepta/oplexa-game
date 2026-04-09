mod client;
mod graphic;
mod logic;
mod shader;

pub use api::{core, generator, utils};

/// Runs the `main` routine for main in the `project` module.
fn main() {
    client::run();
}
