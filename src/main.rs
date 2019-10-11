#![warn(
    rust_2018_idioms,
    deprecated_in_future,
    missing_debug_implementations,
    unused_labels,
    unused_qualifications,
    clippy::cast_possible_truncation
)]

mod common;
mod data_collection;
mod data_storing;

use crate::common::DistanceData;
use failure::{format_err, Error, ResultExt};
use futures::prelude::*;
use log::error;
use std::{env, process};

#[tokio::main]
async fn main() {
    color_backtrace::install();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    if let Err(e) = run().await {
        print_error(e);
        process::exit(-1);
    }
}

fn print_error<E: Into<Error>>(err: E) {
    let err = err.into();
    let mut err_msg = format!("error: {}", err);
    for err in err.iter_causes() {
        err_msg.push_str(&format!("\ncaused by: {}", err));
    }

    error!("{}\n{}", err_msg, err.backtrace());
}

async fn run() -> Result<(), Error> {
    let steam = steamworks::Client::init()?;
    let db = establish_connection().await?;

    let distance_data = data_collection::run(steam)
        .await
        .context("error acquiring data")?;

    print_stats(&distance_data);

    data_storing::run(db, distance_data)
        .await
        .context("error storing data")?;

    println!("Finished successfully.");
    Ok(())
}

async fn establish_connection() -> Result<tokio_postgres::Client, Error> {
    dotenv::dotenv().ok();

    let database_url =
        env::var("DATABASE_URL").context("Environment variable DATABASE_URL is not set")?;

    let (client, connection) =
        tokio_postgres::connect(&database_url, tokio_postgres::NoTls).await?;

    let connection = connection.map(|r| {
        if let Err(e) = r {
            print_error(format_err!("connection error: {}", e));
        }
    });
    tokio::spawn(connection);

    Ok(client)
}

fn print_stats(data: &DistanceData) {
    let total_levels = data.levels.len();
    let official_levels = data
        .levels
        .iter()
        .filter(|level| level.workshop_level_details.is_none())
        .count();
    let workshop_levels = total_levels - official_levels;
    println!(
        "Total levels: {} (Official: {}, Workshop: {})",
        total_levels, official_levels, workshop_levels
    );

    let total_users = data.users.len();
    println!("Total users: {}", total_users);

    let sprint_entries: usize = data
        .levels
        .iter()
        .map(|level| level.sprint_entries.len())
        .sum();
    let challenge_entries: usize = data
        .levels
        .iter()
        .map(|level| level.challenge_entries.len())
        .sum();
    let stunt_entries: usize = data
        .levels
        .iter()
        .map(|level| level.stunt_entries.len())
        .sum();
    let total_entries = sprint_entries + challenge_entries + stunt_entries;
    println!(
        "Total leaderboard entries: {} (Sprint: {}, Challenge: {}, Stunt: {})",
        total_entries, sprint_entries, challenge_entries, stunt_entries
    );
}
