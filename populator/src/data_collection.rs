use crate::common::{
    DistanceData, Level, PublishedFileDetailsSubset, ScoreLeaderboardEntry, TimeLeaderboardEntry,
    User,
};
use anyhow::Error;
use az::Az;
use distance_steam_data_client::{Client as GrpcClient, LeaderboardEntry};
use distance_util::LeaderboardGameMode;
use futures::stream::{self};
use futures::{StreamExt, TryStreamExt, future};
use indicatif::ProgressBar;
use itertools::Itertools;
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};
use tap::{Pipe, TapFallible};
use tracing::{Level as TracingLevel, event};

pub async fn run(
    web_client: reqwest::Client,
    grpc_client: GrpcClient,
    web_api_key: impl Into<String>,
) -> Result<DistanceData, Error> {
    let mut data = DistanceData::new();

    let mut official_levels: HashMap<&'static str, Level> = HashMap::new();
    for (game_mode, idx_offset) in &[
        (LeaderboardGameMode::Sprint, -1000),
        (LeaderboardGameMode::Challenge, -2000),
        (LeaderboardGameMode::Stunt, -3000),
    ] {
        for (idx, &level_name) in game_mode.official_level_names().iter().enumerate() {
            let entry = official_levels.entry(level_name).or_insert(Level {
                id: idx_offset - idx.az::<i64>(),
                name: level_name.to_owned(),
                is_sprint: false,
                is_challenge: false,
                is_stunt: false,
                ..Level::default()
            });

            match game_mode {
                LeaderboardGameMode::Sprint => entry.is_sprint = true,
                LeaderboardGameMode::Challenge => entry.is_challenge = true,
                LeaderboardGameMode::Stunt => entry.is_stunt = true,
            }
        }
    }
    data.levels.extend(official_levels.drain().map(|(_k, v)| v));

    let pb = ProgressBar::new_spinner();
    pb.set_message("Querying all workshop levels");

    let all_workshop_json: Vec<JsonValue> =
        steam_workshop::query_all_files(web_client.clone(), web_api_key.into(), 233610)
            .inspect(|_| pb.tick())
            .map_ok(|x| stream::iter(x.into_iter().map(Ok::<_, Error>)))
            .try_flatten()
            .try_collect()
            .await?;
    let filtered_workshop_data = all_workshop_json.into_iter().filter_map(|json| {
        let details: PublishedFileDetailsSubset = serde_json::from_value(json.clone()).ok()?;
        Some((details, json))
    });
    let workshop_levels = filtered_workshop_data.filter_map(|(details, json)| {
        let is_sprint = details.tags.iter().any(|x| x.tag == "Sprint");
        let is_challenge = details.tags.iter().any(|x| x.tag == "Challenge");
        let is_stunt = details.tags.iter().any(|x| x.tag == "Stunt");

        if (is_sprint || is_challenge || is_stunt)
            && !details.filename.is_empty()
            && details.file_size > 0
        {
            Some(Level {
                id: details.published_file_id,
                name: details.title.clone(),
                is_sprint,
                is_challenge,
                is_stunt,
                workshop_level_details: Some((details, json)),
                ..Level::default()
            })
        } else {
            None
        }
    });

    data.levels.extend(workshop_levels);
    pb.finish();

    println!("Downloading Sprint leaderboard entries");
    {
        let entries = get_mode_entries(
            &grpc_client,
            &data.levels,
            LeaderboardGameMode::Sprint,
            |l| l.is_sprint,
        )
        .await;

        for (i, level_entries_raw) in entries {
            let level_entries =
                level_entries_raw
                    .into_iter()
                    .map(|(entry, rank)| TimeLeaderboardEntry {
                        steam_id: entry.steam_id,
                        time: entry.score,
                        rank,
                        has_replay: entry.has_replay,
                    });

            data.levels[i].sprint_entries.extend(level_entries);
        }
    }

    println!("Downloading Challenge leaderboard entries");
    {
        let entries = get_mode_entries(
            &grpc_client,
            &data.levels,
            LeaderboardGameMode::Challenge,
            |l| l.is_challenge,
        )
        .await;

        for (i, level_entries_raw) in entries {
            let level_entries =
                level_entries_raw
                    .into_iter()
                    .map(|(entry, rank)| TimeLeaderboardEntry {
                        steam_id: entry.steam_id,
                        time: entry.score,
                        rank,
                        has_replay: entry.has_replay,
                    });

            data.levels[i].challenge_entries.extend(level_entries);
        }
    }

    println!("Downloading Stunt leaderboard entries");
    {
        let entries = get_mode_entries(
            &grpc_client,
            &data.levels,
            LeaderboardGameMode::Stunt,
            |l| l.is_stunt,
        )
        .await;

        for (i, level_entries_raw) in entries {
            let level_entries =
                level_entries_raw
                    .into_iter()
                    .map(|(entry, rank)| ScoreLeaderboardEntry {
                        steam_id: entry.steam_id,
                        score: entry.score,
                        rank,
                        has_replay: entry.has_replay,
                    });

            data.levels[i].stunt_entries.extend(level_entries);
        }
    }

    // Resolve Player and Author names
    {
        let mut user_ids = HashSet::new();

        // Level authors
        data.levels
            .iter()
            .filter_map(|level| {
                level
                    .workshop_level_details
                    .as_ref()
                    .map(|(details, _json)| details.creator)
            })
            .for_each(|steam_id| {
                user_ids.insert(steam_id);
            });

        // Sprint players
        data.levels
            .iter()
            .flat_map(|level| level.sprint_entries.iter().map(|entry| entry.steam_id))
            .for_each(|steam_id| {
                user_ids.insert(steam_id);
            });

        // Challenge players
        data.levels
            .iter()
            .flat_map(|level| level.challenge_entries.iter().map(|entry| entry.steam_id))
            .for_each(|steam_id| {
                user_ids.insert(steam_id);
            });

        // Stunt players
        data.levels
            .iter()
            .flat_map(|level| level.stunt_entries.iter().map(|entry| entry.steam_id))
            .for_each(|steam_id| {
                user_ids.insert(steam_id);
            });

        let user_ids = user_ids.into_iter().collect_vec();

        println!("Resolving player + author names.");
        let mut user_names = Vec::with_capacity(user_ids.len());
        for (i, chunk) in user_ids.chunks(1000).enumerate() {
            println!("request #{i}");
            user_names.extend(grpc_client.persona_names(chunk.to_vec()).await?);
        }
        println!("Finished resolving player + author names.");

        let users = user_ids
            .iter()
            .zip(user_names)
            .map(|(&steam_id, name)| User {
                steam_id,
                name: name.unwrap_or_default(),
            })
            .collect();

        data.users = users;
    }

    Ok(data)
}

/// Returns the leaderboard entries for the specified `game_mode`.
///
/// The return value is a vec of tuples, where each tuple consists of 1. an
/// index into the passed-in `levels` slice, and 2. a vec containing all
/// entries for that particular level, together with the rank for each entry.
async fn get_mode_entries(
    client: &GrpcClient,
    levels: &[Level],
    game_mode: LeaderboardGameMode,
    game_mode_predicate: impl Fn(&Level) -> bool,
) -> Vec<(usize, Vec<(LeaderboardEntry, u32)>)> {
    let mode_level_leaderboard_names: Vec<_> = levels
        .iter()
        .enumerate()
        .filter_map(|(i, level)| {
            if !game_mode_predicate(level) {
                return None;
            }

            let leaderboard_name_string =
                if let Some((details, _json)) = &level.workshop_level_details {
                    // Workshop level
                    let filename_without_extension = &details.filename
                        [0..(details.filename.len().saturating_sub(".bytes".len()))];
                    distance_util::create_leaderboard_name_string(
                        filename_without_extension,
                        game_mode,
                        Some(details.creator),
                    )
                } else {
                    // Official level
                    distance_util::create_leaderboard_name_string(&level.name, game_mode, None)
                };

            leaderboard_name_string.ok().map(|s| (i, s))
        })
        .collect();

    let pb = ProgressBar::new(mode_level_leaderboard_names.len() as u64);
    let entries: Vec<(usize, Vec<(LeaderboardEntry, u32)>)> = mode_level_leaderboard_names
        .into_iter()
        .map(|(i, leaderboard_name_string)| async move {
            let level_entries = client
                .leaderboard_entries_all(&leaderboard_name_string)
                .await
                .tap_err(|err| {
                    event!(
                        TracingLevel::WARN,
                        "failed to download entries for `{leaderboard_name_string}` {err}"
                    )
                })
                .ok()?;

            let mut level_entries_with_rank = Vec::with_capacity(level_entries.len());
            let mut level_entries = level_entries.into_iter();

            if let Some(entry) = level_entries.next() {
                let mut prev_score = entry.score;
                let mut prev_rank = 1;
                level_entries_with_rank.push((entry, 1));
                for (entry, position) in level_entries.zip(2..) {
                    let tied_previous = entry.score == prev_score;

                    let rank = if tied_previous { prev_rank } else { position };

                    prev_score = entry.score;
                    prev_rank = rank;
                    level_entries_with_rank.push((entry, rank));
                }
            }

            Some((i, level_entries_with_rank))
        })
        .pipe(stream::iter)
        .buffer_unordered(4)
        .inspect(|_| pb.inc(1))
        .filter_map(future::ready)
        .collect()
        .await;

    pb.finish_and_clear();

    entries
}
