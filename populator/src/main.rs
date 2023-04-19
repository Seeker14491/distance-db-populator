#![warn(
    rust_2018_idioms,
    deprecated_in_future,
    macro_use_extern_crate,
    missing_debug_implementations,
    unused_qualifications
)]

use crate::common::DistanceData;
use anyhow::{anyhow, Context, Error};
use distance_steam_data_client::Client as GrpcClient;
use futures::prelude::*;
use std::env;
use std::time::Instant;

mod common;
mod data_collection;
mod data_storing;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Error> {
    color_backtrace::install();
    tracing_subscriber::fmt::init();
    dotenv::dotenv().ok();

    let grpc_server_address = env::var("GRPC_SERVER_ADDRESS")
        .context("The environment variable `GRPC_SERVER_ADDRESS` must be set.")?;

    let distance_data = {
        let steam_web_api_key = env::var("STEAM_WEB_API_KEY")
            .expect("environment variable STEAM_WEB_API_KEY is not set");

        let web_client = reqwest::Client::new();

        println!("Connecting to Distance gRPC server...");
        let grpc = GrpcClient::connect(&grpc_server_address).await?;
        println!("Connected.");

        println!("Starting data collection.");
        let start_instant = Instant::now();
        let data = data_collection::run(web_client, grpc, steam_web_api_key.clone())
            .await
            .context("error acquiring data")?;
        let data_collection_time = Instant::now().duration_since(start_instant);
        println!(
            "Finished collecting data in {} seconds.",
            data_collection_time.as_secs()
        );

        data
    };

    print_stats(&distance_data);

    println!("Connecting to database...");
    let mut db = establish_connection().await?;
    println!("Connected to database.");

    data_storing::run(&mut db, distance_data)
        .await
        .context("error storing data")?;

    println!("Finished successfully.");

    Ok(())
}

async fn establish_connection() -> Result<tokio_postgres::Client, Error> {
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
        "Total levels: {total_levels} (Official: {official_levels}, Workshop: {workshop_levels})"
    );

    let total_users = data.users.len();
    println!("Total users: {total_users}");

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
        "Total leaderboard entries: {total_entries} (Sprint: {sprint_entries}, Challenge: {challenge_entries}, Stunt: {stunt_entries})"
    );
}
