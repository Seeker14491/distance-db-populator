#![warn(
    rust_2018_idioms,
    deprecated_in_future,
    missing_debug_implementations,
    unused_labels,
    unused_qualifications,
    clippy::cast_possible_truncation
)]

#[macro_use]
extern crate diesel;

#[macro_use]
mod macros;

mod extra_level_data;
mod models;
mod schema;

use crate::models::{
    NewChallengeLeaderboardEntry, NewLevel, NewSprintLeaderboardEntry, NewStuntLeaderboardEntry,
    NewUser, NewWorkshopLevelDetails,
};
use diesel::{
    expression::NonAggregate,
    insertable::BatchInsert,
    pg::{upsert, Pg, PgConnection},
    prelude::*,
    query_builder::{InsertStatement, QueryFragment, QueryId, UndecoratedInsertRecord},
    query_source::joins::{Join, JoinOn},
    sql_types,
};
use distance_util::{enumflags2::BitFlags, LeaderboardGameMode};
use failure::{Error, ResultExt};
use futures::{executor::LocalPool, future, prelude::*, stream::FuturesUnordered};
use itertools::Itertools;
use log::error;
use std::{cmp, collections::HashMap, env, process, rc::Rc};
use steamworks::{
    ugc::{MatchingUgcType, PublishedFileVisibility, UgcDetails},
    user_stats::LeaderboardEntry,
    SteamId,
};

const MAX_LEADERBOARD_RANK_TO_DOWNLOAD: u32 = u32::max_value();

fn main() {
    color_backtrace::install();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let mut pool = LocalPool::new();

    if let Err(e) = pool.run_until(run()) {
        print_error(e);
        process::exit(-1);
    }
}

fn print_error<E: Into<Error>>(e: E) {
    let e = e.into();
    error!("error: {}", e);
    for err in e.iter_causes() {
        error!(" caused by: {}", err);
    }
}

async fn run() -> Result<(), Error> {
    let steam = steamworks::Client::init()?;
    let db = establish_connection()?;

    let mut official_levels: HashMap<&'static str, NewLevel<'_>> = HashMap::new();
    for game_mode in BitFlags::<LeaderboardGameMode>::all().iter() {
        for &level_name in game_mode.official_levels() {
            let entry = official_levels.entry(level_name).or_insert(NewLevel {
                name: level_name,
                is_sprint: false,
                is_challenge: false,
                is_stunt: false,
            });

            match game_mode {
                LeaderboardGameMode::Sprint => entry.is_sprint = true,
                LeaderboardGameMode::Challenge => entry.is_challenge = true,
                LeaderboardGameMode::Stunt => entry.is_stunt = true,
            }
        }
    }

    println!("Inserting official levels");
    let official_levels: Vec<_> = official_levels.into_iter().map(|(_k, v)| v).collect();
    diesel::insert_into(schema::levels::table)
        .values(&official_levels)
        .execute(&*db)?;

    println!("Querying all workshop levels");
    let workshop_level_details: Vec<UgcDetails> = steam
        .query_all_ugc(MatchingUgcType::ItemsReadyToUse)
        .match_any_tags()
        .required_tags(["Sprint", "Challenge", "Stunt"].iter().copied())
        .run()
        .try_filter(|details| {
            let is_sprint = details.tags.iter().any(|tag| tag == "Sprint");
            let is_challenge = details.tags.iter().any(|tag| tag == "Challenge");
            let is_stunt = details.tags.iter().any(|tag| tag == "Stunt");

            future::ready(
                (is_sprint || is_challenge || is_stunt)
                    && !details.file_name.is_empty()
                    && details.file_size > 0,
            )
        })
        .try_collect()
        .await?;

    println!("Inserting workshop authors");
    let authors = workshop_level_details
        .iter()
        .map(|details| details.steam_id_owner)
        .collect();

    add_users!(db.clone(), &steam, authors).await?;

    println!("Inserting workshop levels and level details");
    {
        let levels: Vec<_> = workshop_level_details
            .iter()
            .map(|details| {
                let is_sprint = details.tags.iter().any(|tag| tag == "Sprint");
                let is_challenge = details.tags.iter().any(|tag| tag == "Challenge");
                let is_stunt = details.tags.iter().any(|tag| tag == "Stunt");

                NewLevel {
                    name: &details.title,
                    is_sprint,
                    is_challenge,
                    is_stunt,
                }
            })
            .collect();

        let ids: Vec<i32> = diesel::insert_into(schema::levels::table)
            .values(&levels)
            .returning(schema::levels::id)
            .get_results(&*db)?;

        let workshop_level_details: Vec<_> = workshop_level_details
            .iter()
            .zip(ids)
            .map(|(d, id)| NewWorkshopLevelDetails {
                level_id: id,
                author_steam_id: d.steam_id_owner.as_u64() as i64,
                description: &d.description,
                time_created: d.time_created.naive_local(),
                time_updated: d.time_updated.naive_local(),
                visibility: match d.visibility {
                    PublishedFileVisibility::Public => "public",
                    PublishedFileVisibility::FriendsOnly => "friends_only",
                    PublishedFileVisibility::Private => "private",
                },
                tags: d.tags.as_str(),
                preview_url: &d.preview_url,
                file_name: &d.file_name,
                file_size: d.file_size,
                votes_up: d.votes_up as i32,
                votes_down: d.votes_down as i32,
                score: d.score,
            })
            .collect();

        insert_into_chunked(
            schema::workshop_level_details::table,
            &workshop_level_details,
            |query| query.execute(&*db),
        )?;
    }

    {
        use schema::levels::dsl as l;

        println!("Downloading Sprint leaderboard entries + users");
        {
            let entries = download_leaderboard_entries_stage(
                db.clone(),
                steam.clone(),
                LeaderboardGameMode::Sprint,
                l::is_sprint,
            )
            .await?;

            let new_entries: Vec<_> = entries
                .into_iter()
                .map(|(level_id, entry)| NewSprintLeaderboardEntry {
                    level_id,
                    steam_id: entry.steam_id.as_u64() as i64,
                    time: entry.score,
                })
                .collect();

            insert_into_chunked(
                schema::sprint_leaderboard_entries::table,
                &new_entries,
                |query| query.execute(&*db),
            )?;
        }

        println!("Downloading Challenge leaderboard entries + users");
        {
            let entries = download_leaderboard_entries_stage(
                db.clone(),
                steam.clone(),
                LeaderboardGameMode::Challenge,
                l::is_challenge,
            )
            .await?;

            let new_entries: Vec<_> = entries
                .into_iter()
                .map(|(level_id, entry)| NewChallengeLeaderboardEntry {
                    level_id,
                    steam_id: entry.steam_id.as_u64() as i64,
                    time: entry.score,
                })
                .collect();

            insert_into_chunked(
                schema::challenge_leaderboard_entries::table,
                &new_entries,
                |query| query.execute(&*db),
            )?;
        }

        println!("Downloading Stunt leaderboard entries + users");
        {
            let entries = download_leaderboard_entries_stage(
                db.clone(),
                steam.clone(),
                LeaderboardGameMode::Stunt,
                l::is_stunt,
            )
            .await?;

            let new_entries: Vec<_> = entries
                .into_iter()
                .map(|(level_id, entry)| NewStuntLeaderboardEntry {
                    level_id,
                    steam_id: entry.steam_id.as_u64() as i64,
                    score: entry.score,
                })
                .collect();

            insert_into_chunked(
                schema::stunt_leaderboard_entries::table,
                &new_entries,
                |query| query.execute(&*db),
            )?;
        }
    }

    println!("Finished successfully.");
    Ok(())
}

fn establish_connection() -> Result<Rc<PgConnection>, Error> {
    dotenv::dotenv().ok();

    let database_url =
        env::var("DATABASE_URL").context("Environment variable DATABASE_URL is not set")?;
    Ok(Rc::new(PgConnection::establish(&database_url)?))
}

// Work around Postgres 65535 parameter limit by splitting insert up into smaller inserts
fn insert_into_chunked<T, Tbl, U>(
    table: Tbl,
    values: &[T],
    mut customize_and_send: impl FnMut(InsertStatement<Tbl, BatchInsert<'_, T, Tbl>>) -> QueryResult<U>,
) -> QueryResult<Vec<U>>
where
    T: UndecoratedInsertRecord<Tbl>,
    Tbl: Copy,
{
    const SLICE_SIZE: usize = 1024;

    if values.is_empty() {
        return Ok(Vec::new());
    }

    let mut i = 0;
    let mut results = Vec::with_capacity((values.len() - 1) / SLICE_SIZE + 1);
    while i < values.len() {
        let slice = &values[i..cmp::min(i + SLICE_SIZE, values.len())];
        let query = diesel::insert_into(table).values(slice);
        results.push(customize_and_send(query)?);

        i += SLICE_SIZE;
    }

    Ok(results)
}

async fn download_leaderboard_entries_stage<T>(
    db: Rc<PgConnection>,
    steam: steamworks::Client,
    game_mode: LeaderboardGameMode,
    mode_column: T,
) -> Result<Vec<(i32, LeaderboardEntry)>, Error>
where
    T: Expression<SqlType = sql_types::Bool>
        + NonAggregate
        + AppearsOnTable<
            JoinOn<
                Join<
                    schema::levels::table,
                    schema::workshop_level_details::table,
                    diesel::query_source::joins::LeftOuter,
                >,
                diesel::expression::operators::Eq<
                    diesel::expression::nullable::Nullable<
                        schema::workshop_level_details::columns::level_id,
                    >,
                    diesel::expression::nullable::Nullable<schema::levels::columns::id>,
                >,
            >,
        > + QueryId
        + QueryFragment<Pg>,
{
    use schema::{levels::dsl as l, workshop_level_details as wld};

    let db_data: Vec<(i32, String, Option<i64>, Option<String>)> = schema::levels::table
        .left_join(wld::table)
        .select((
            l::id,
            l::name,
            wld::author_steam_id.nullable(),
            wld::file_name.nullable(),
        ))
        .filter(mode_column)
        .load(&*db)?;

    let entries: Vec<(i32, LeaderboardEntry)> = db_data
        .into_iter()
        .filter_map(|(level_id, name, author_steam_id, file_name)| {
            let leaderboard_name_string =
                if let (Some(author_steam_id), Some(file_name)) = (author_steam_id, file_name) {
                    // Workshop level
                    let filename_without_extension =
                        &file_name[0..(file_name.len().saturating_sub(".bytes".len()))];
                    distance_util::create_leaderboard_name_string(
                        filename_without_extension,
                        game_mode,
                        Some(author_steam_id as u64),
                    )
                } else {
                    // Official level
                    distance_util::create_leaderboard_name_string(&name, game_mode, None)
                };

            leaderboard_name_string.map(|s| (level_id, s))
        })
        .map(|(level_id, leaderboard_name_string)| {
            let steam = steam.clone();
            async move {
                let leaderboard = steam
                    .find_leaderboard(leaderboard_name_string.clone())
                    .await;
                if let Ok(leaderboard) = leaderboard {
                    Some(
                        leaderboard
                            .download_global(1, MAX_LEADERBOARD_RANK_TO_DOWNLOAD, 0)
                            .await
                            .into_iter()
                            .map(move |entry| (level_id, entry)),
                    )
                } else {
                    None
                }
            }
        })
        .collect::<FuturesUnordered<_>>()
        .filter_map(|x| future::ready(x.map(stream::iter)))
        .flatten()
        .collect()
        .await;

    let players = entries.iter().map(|(_, entry)| entry.steam_id).collect();

    add_users!(db.clone(), &steam, players).await?;

    Ok(entries)
}
