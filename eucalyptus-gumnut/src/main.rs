use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::item::ItemKind;

mod client;
mod config;
mod daemon;
mod item;

fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::filter::Targets::new()
                .with_default(tracing::Level::WARN)
                .with_target(env!("CARGO_CRATE_NAME"), tracing::Level::INFO),
        )
        .init();

    let args = Args::parse();
    match args {
        Args::Start => daemon::start(),
        Args::Show { item } => client::show(item).unwrap(),
        Args::Stop => client::stop().unwrap(),
    }
}

#[derive(Parser)]
enum Args {
    Start,
    Show { item: ItemKind },
    Stop,
}
