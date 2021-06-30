#![warn(
    rust_2018_idioms,
    deprecated_in_future,
    macro_use_extern_crate,
    missing_debug_implementations,
    unused_qualifications
)]

use crate::common::DistanceData;
use anyhow::{anyhow, Context, Error};
use futures::prelude::*;
use std::env;

mod common;
mod data_collection;
mod data_storing;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Error> {
    color_backtrace::install();
    tracing_subscriber::fmt::init();

    println!("Connecting to database...");
    let mut db = establish_connection().await?;
    println!("Connected to database.");

    let distance_data = {
        println!("Initializing Steamworks API...");
        let steam = steamworks::Client::init()?;
        println!("Steamworks API initialized.");

        data_collection::run(steam)
            .await
            .context("error acquiring data")?
    };

    print_stats(&distance_data);

    data_storing::run(&mut db, distance_data)
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
            eprintln!("{}", anyhow!("connection error: {}", e));
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
