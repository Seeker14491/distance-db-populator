use crate::common::{DistanceData, Level, ScoreLeaderboardEntry, TimeLeaderboardEntry, User};
use anyhow::Error;
use distance_util::LeaderboardGameMode;
use futures::stream::FuturesUnordered;
use futures::{future, StreamExt, TryStreamExt};
use indicatif::ProgressBar;
use itertools::Itertools;
use std::collections::{HashMap, HashSet};
use steamworks::ugc::MatchingUgcType;
use steamworks::user_stats::{FindLeaderboardError, LeaderboardEntry};
use tracing::debug;

const MAX_LEADERBOARD_RANK_TO_DOWNLOAD: u32 = u32::MAX;
const EMPTY_LEADERBOARD_RETRY_COUNT: usize = 5;

pub async fn run(steam: steamworks::Client) -> Result<DistanceData, Error> {
    let mut data = DistanceData::new();

    let mut official_levels: HashMap<&'static str, Level> = HashMap::new();
    for game_mode in &[
        LeaderboardGameMode::Sprint,
        LeaderboardGameMode::Challenge,
        LeaderboardGameMode::Stunt,
    ] {
        for &level_name in game_mode.official_level_names() {
            let entry = official_levels.entry(level_name).or_insert(Level {
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
    let workshop_levels: Vec<Level> = steam
        .query_all_ugc(MatchingUgcType::ItemsReadyToUse)
        .match_any_tags()
        .required_tags(["Sprint", "Challenge", "Stunt"].iter().copied())
        .run()
        .inspect(|_| pb.tick())
        .try_filter_map(|details| {
            let is_sprint = details.tags.iter().any(|tag| tag == "Sprint");
            let is_challenge = details.tags.iter().any(|tag| tag == "Challenge");
            let is_stunt = details.tags.iter().any(|tag| tag == "Stunt");

            let level = if (is_sprint || is_challenge || is_stunt)
                && !details.file_name.is_empty()
                && details.file_size > 0
            {
                Some(Level {
                    name: details.title.clone(),
                    is_sprint,
                    is_challenge,
                    is_stunt,
                    workshop_level_details: Some(details),
                    ..Level::default()
                })
            } else {
                None
            };

            future::ready(Ok(level))
        })
        .try_collect()
        .await?;
    data.levels.extend(workshop_levels.into_iter());
    pb.finish();

    println!("Downloading Sprint leaderboard entries");
    {
        let entries = get_mode_entries(&steam, &data.levels, LeaderboardGameMode::Sprint, |l| {
            l.is_sprint
        })
        .await;

        for (i, level_entries_raw) in entries {
            let level_entries =
                level_entries_raw
                    .into_iter()
                    .map(|(entry, rank)| TimeLeaderboardEntry {
                        steam_id: entry.steam_id.into(),
                        time: entry.score,
                        rank,
                        has_replay: entry.ugc.is_some(),
                    });

            data.levels[i].sprint_entries.extend(level_entries);
        }
    }

    println!("Downloading Challenge leaderboard entries");
    {
        let entries = get_mode_entries(&steam, &data.levels, LeaderboardGameMode::Challenge, |l| {
            l.is_challenge
        })
        .await;

        for (i, level_entries_raw) in entries {
            let level_entries =
                level_entries_raw
                    .into_iter()
                    .map(|(entry, rank)| TimeLeaderboardEntry {
                        steam_id: entry.steam_id.into(),
                        time: entry.score,
                        rank,
                        has_replay: entry.ugc.is_some(),
                    });

            data.levels[i].challenge_entries.extend(level_entries);
        }
    }

    println!("Downloading Stunt leaderboard entries");
    {
        let entries = get_mode_entries(&steam, &data.levels, LeaderboardGameMode::Stunt, |l| {
            l.is_stunt
        })
        .await;

        for (i, level_entries_raw) in entries {
            let level_entries =
                level_entries_raw
                    .into_iter()
                    .map(|(entry, rank)| ScoreLeaderboardEntry {
                        steam_id: entry.steam_id.into(),
                        score: entry.score,
                        rank,
                        has_replay: entry.ugc.is_some(),
                    });

            data.levels[i].stunt_entries.extend(level_entries);
        }
    }

    // Resolve Player and Author names
    {
        let mut users = HashSet::new();

        // Level authors
        data.levels
            .iter()
            .filter_map(|level| {
                level
                    .workshop_level_details
                    .as_ref()
                    .map(|details| details.steam_id_owner)
            })
            .for_each(|steam_id| {
                users.insert(steam_id);
            });

        // Sprint players
        data.levels
            .iter()
            .flat_map(|level| level.sprint_entries.iter().map(|entry| entry.steam_id))
            .for_each(|steam_id| {
                users.insert(steam_id.into());
            });

        // Challenge players
        data.levels
            .iter()
            .flat_map(|level| level.challenge_entries.iter().map(|entry| entry.steam_id))
            .for_each(|steam_id| {
                users.insert(steam_id.into());
            });

        // Stunt players
        data.levels
            .iter()
            .flat_map(|level| level.stunt_entries.iter().map(|entry| entry.steam_id))
            .for_each(|steam_id| {
                users.insert(steam_id.into());
            });

        println!("Resolving player + author names");
        let pb = &ProgressBar::new(users.len() as u64);
        let users: Vec<_> = users
            .into_iter()
            .map(|steam_id| {
                let steam = steam.clone();
                async move {
                    let name = steam_id.persona_name(&steam).await;
                    pb.inc(1);

                    User {
                        steam_id: steam_id.into(),
                        name,
                    }
                }
            })
            .collect::<FuturesUnordered<_>>()
            .collect()
            .await;

        data.users = users;
        pb.finish();
    }

    Ok(data)
}

/// Returns the leaderboard entries for the specified `game_mode`.
///
/// The return value is a vec of tuples, where each tuple consists of 1. an
/// index into the passed-in `levels` slice, and 2. a vec containing all
/// entries for that particular level, together with the rank for each entry.
async fn get_mode_entries(
    steam: &steamworks::Client,
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

            let leaderboard_name_string = if let Some(details) = &level.workshop_level_details {
                // Workshop level
                let filename_without_extension =
                    &details.file_name[0..(details.file_name.len().saturating_sub(".bytes".len()))];
                distance_util::create_leaderboard_name_string(
                    filename_without_extension,
                    game_mode,
                    Some(details.steam_id_owner.into()),
                )
            } else {
                // Official level
                distance_util::create_leaderboard_name_string(&level.name, game_mode, None)
            };

            leaderboard_name_string.map(|s| (i, s))
        })
        .collect();

    let pb = ProgressBar::new(mode_level_leaderboard_names.len() as u64);
    let entries: Vec<(usize, Vec<(LeaderboardEntry, u32)>)> = mode_level_leaderboard_names
        .into_iter()
        .map(|(i, leaderboard_name_string)| {
            let steam = steam.clone();
            async move {
                let mut leaderboard = None;
                for pos in (0..(1 + EMPTY_LEADERBOARD_RETRY_COUNT)).with_position() {
                    let leaderboard_2 = steam
                        .find_leaderboard(leaderboard_name_string.clone())
                        .await;

                    match leaderboard_2 {
                        Ok(handle) => {
                            match pos {
                                itertools::Position::Middle(_) | itertools::Position::Last(_) => {
                                    debug!(
                                        leaderboard = %leaderboard_name_string,
                                        current_retry = pos.into_inner(),
                                        "retry succeeded in finding leaderboard"
                                    );
                                }
                                itertools::Position::First(_) | itertools::Position::Only(_) => {}
                            }

                            leaderboard = Some(handle);
                            break;
                        }
                        Err(e) => match e {
                            FindLeaderboardError::NotFound { .. } => {
                                let will_retry = match pos {
                                    itertools::Position::First(_)
                                    | itertools::Position::Middle(_) => true,
                                    itertools::Position::Last(_) | itertools::Position::Only(_) => {
                                        false
                                    }
                                };

                                debug!(
                                    error = %e,
                                    current_retry = pos.into_inner(),
                                    will_retry,
                                    "error finding leaderboard"
                                );
                            }
                            _ => {
                                debug!(
                                    error = %e,
                                    "error finding leaderboard"
                                );

                                break;
                            }
                        },
                    }
                }

                let leaderboard = match leaderboard {
                    Some(x) => x,
                    None => {
                        return None;
                    }
                };

                let mut level_entries = Vec::new();
                for pos in (0..(1 + EMPTY_LEADERBOARD_RETRY_COUNT)).with_position() {
                    level_entries = leaderboard
                        .download_global(1, MAX_LEADERBOARD_RANK_TO_DOWNLOAD, 0)
                        .await;

                    if !level_entries.is_empty() {
                        match pos {
                            itertools::Position::First(_) | itertools::Position::Only(_) => {}
                            itertools::Position::Middle(i) | itertools::Position::Last(i) => {
                                debug!(
                                    leaderboard = %leaderboard_name_string,
                                    current_retry = i,
                                    "leaderboard download retry succeeded"
                                );
                            }
                        }

                        break;
                    }

                    match pos {
                        itertools::Position::First(_) | itertools::Position::Only(_) => {
                            debug!(
                                leaderboard = %leaderboard_name_string,
                                "empty leaderboard response"
                            );
                        }
                        itertools::Position::Middle(_) => {}
                        itertools::Position::Last(_) => {
                            debug!(
                                leaderboard = %leaderboard_name_string,
                                limit = EMPTY_LEADERBOARD_RETRY_COUNT,
                                "leaderboard download retry limit reached; assuming it's empty"
                            );
                        }
                    }
                }

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
            }
        })
        .collect::<FuturesUnordered<_>>()
        .inspect(|_| pb.inc(1))
        .filter_map(future::ready)
        .collect()
        .await;

    pb.finish_and_clear();

    entries
}
