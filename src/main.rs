mod client;
mod graphic;
mod logic;

pub use api::{core, generator, utils};

fn main() {
    client::run();
}
