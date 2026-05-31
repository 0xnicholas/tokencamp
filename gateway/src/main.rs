mod config;
mod auth;
mod error;
mod routes;

fn main() {
    let config = config::load("config/default.yaml").expect("Failed to load config");
    println!("Config loaded: {:?}", config.server);
}
