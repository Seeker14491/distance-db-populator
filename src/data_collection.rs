use crate::common::{DistanceData, Level, User};
use distance_util::LeaderboardGameMode;
use failure::Error;
use futures::{future, stream, stream::FuturesUnordered, StreamExt, TryStreamExt};
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
        let entries =
            get_mode_entries(&steam, &mut data.levels, LeaderboardGameMode::Sprint, |l| {
                l.is_sprint
            })
            .await;

        for (i, entry) in entries {
            data.levels[i].sprint_entries.push(entry);
        }
    }

    println!("Downloading Challenge leaderboard entries");
    {
        let entries = get_mode_entries(
            &steam,
            &mut data.levels,
            LeaderboardGameMode::Challenge,
            |l| l.is_challenge,
        )
        .await;

        for (i, entry) in entries {
            data.levels[i].challenge_entries.push(entry);
        }
    }

    println!("Downloading Stunt leaderboard entries");
    {
        let entries = get_mode_entries(&steam, &mut data.levels, LeaderboardGameMode::Stunt, |l| {
            l.is_stunt
        })
        .await;

        for (i, entry) in entries {
            data.levels[i].stunt_entries.push(entry);
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

async fn get_mode_entries<T>(
    steam: &steamworks::Client,
    levels: &[Level],
    game_mode: LeaderboardGameMode,
    game_mode_predicate: impl Fn(&Level) -> bool,
) -> Vec<(usize, T)>
where
    T: From<LeaderboardEntry>,
{
    let pb = ProgressBar::new_spinner();
    let entries: Vec<(usize, T)> = levels
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
                    Some(
                        leaderboard
                            .download_global(1, MAX_LEADERBOARD_RANK_TO_DOWNLOAD, 0)
                            .await
                            .into_iter()
                            .map(move |entry| (i, T::from(entry))),
                    )
                } else {
                    None
                }
            }
        })
        .collect::<FuturesUnordered<_>>()
        .inspect(|_| pb.tick())
        .filter_map(|x| future::ready(x.map(stream::iter)))
        .flatten()
        .collect()
        .await;

    pb.finish_and_clear();

    entries
}
