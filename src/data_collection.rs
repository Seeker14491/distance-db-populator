use crate::common::{DistanceData, Level, ScoreLeaderboardEntry, TimeLeaderboardEntry, User};
use anyhow::Error;
use distance_util::LeaderboardGameMode;
use futures::{future, stream::FuturesUnordered, StreamExt, TryStreamExt};
use indicatif::ProgressBar;
use std::collections::{HashMap, HashSet};
use steamworks::{ugc::MatchingUgcType, user_stats::LeaderboardEntry};

const MAX_LEADERBOARD_RANK_TO_DOWNLOAD: u32 = u32::max_value();

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

    {
        let pb = ProgressBar::new_spinner();
        pb.set_message("Resolving player + author names");

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

        users
            .into_iter()
            .map(|steam_id| {
                let steam = steam.clone();
                async move {
                    let name = steam_id.persona_name(&steam).await;

                    User {
                        steam_id: steam_id.into(),
                        name,
                    }
                }
            })
            .collect::<FuturesUnordered<_>>()
            .for_each(|user| {
                data.users.push(user);

                future::ready(())
            })
            .await;

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
    let pb = ProgressBar::new_spinner();
    let entries: Vec<(usize, Vec<(LeaderboardEntry, u32)>)> = levels
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
        .map(|(i, leaderboard_name_string)| {
            let steam = steam.clone();
            async move {
                let leaderboard = steam
                    .find_leaderboard(leaderboard_name_string.clone())
                    .await;
                if let Ok(leaderboard) = leaderboard {
                    let level_entries = leaderboard
                        .download_global(1, MAX_LEADERBOARD_RANK_TO_DOWNLOAD, 0)
                        .await;

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
                } else {
                    None
                }
            }
        })
        .collect::<FuturesUnordered<_>>()
        .inspect(|_| pb.tick())
        .filter_map(future::ready)
        .collect()
        .await;

    pb.finish_and_clear();

    entries
}
