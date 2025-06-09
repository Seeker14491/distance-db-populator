use crate::common::{DistanceData, ScoreLeaderboardEntry, TimeLeaderboardEntry};
use anyhow::Error;
use futures::prelude::*;
use futures::stream::{self, FuturesOrdered, FuturesUnordered};
use fxhash::FxHasher;
use std::hash::{Hash, Hasher};
use tokio_postgres::binary_copy::BinaryCopyInWriter;
use tokio_postgres::types::Type as PgType;

fn compute_sprint_hash(entries: &[TimeLeaderboardEntry]) -> i64 {
    let mut hasher = FxHasher::default();
    for entry in entries {
        entry.steam_id.hash(&mut hasher);
        entry.time.hash(&mut hasher);
        entry.has_replay.hash(&mut hasher);
    }
    hasher.finish() as i64
}

fn compute_challenge_hash(entries: &[TimeLeaderboardEntry]) -> i64 {
    let mut hasher = FxHasher::default();
    for entry in entries {
        entry.steam_id.hash(&mut hasher);
        entry.time.hash(&mut hasher);
        entry.has_replay.hash(&mut hasher);
    }
    hasher.finish() as i64
}

fn compute_stunt_hash(entries: &[ScoreLeaderboardEntry]) -> i64 {
    let mut hasher = FxHasher::default();
    for entry in entries {
        entry.steam_id.hash(&mut hasher);
        entry.score.hash(&mut hasher);
        entry.has_replay.hash(&mut hasher);
    }
    hasher.finish() as i64
}

pub async fn run(db: &mut tokio_postgres::Client, data: DistanceData) -> Result<(), Error> {
    let mut transaction_owned = db.transaction().await?;
    let transaction = &transaction_owned;

    println!("Updating users in the database");
    let stmt = &transaction
        .prepare("INSERT INTO users VALUES ($1, $2) ON CONFLICT (steam_id) DO UPDATE SET name = EXCLUDED.name")
        .await?;
    stream::iter(&data.users)
        .map(Ok)
        .try_for_each_concurrent(None, |user| async move {
            transaction
                .execute(stmt, &[&(user.steam_id as i64), &user.name])
                .map_ok(drop)
                .await
        })
        .await?;

    println!("Updating levels in the database");
    let stmt = &transaction
        .prepare("INSERT INTO levels (id, name, is_sprint, is_challenge, is_stunt) VALUES ($1, $2, $3, $4, $5) ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, is_sprint = EXCLUDED.is_sprint, is_challenge = EXCLUDED.is_challenge, is_stunt = EXCLUDED.is_stunt")
        .await?;
    let level_ids: Vec<_> = data
        .levels
        .iter()
        .map(|level| async move {
            transaction
                .execute(
                    stmt,
                    &[
                        &level.id,
                        &level.name,
                        &level.is_sprint,
                        &level.is_challenge,
                        &level.is_stunt,
                    ],
                )
                .await?;

            Ok::<_, Error>(level.id)
        })
        .collect::<FuturesOrdered<_>>()
        .try_collect()
        .await?;

    println!("Updating workshop level details and leaderboard entries");
    let wld_stmt = &transaction
        .prepare("INSERT INTO workshop_level_details VALUES ($1, $2, $3) ON CONFLICT (level_id) DO UPDATE SET raw_details = EXCLUDED.raw_details, tags = EXCLUDED.tags")
        .await?;

    // Prepare statements for checking existing leaderboard hashes
    let hash_stmt = &transaction
        .prepare("SELECT sprint_leaderboard_hash, challenge_leaderboard_hash, stunt_leaderboard_hash FROM levels WHERE id = $1")
        .await?;

    // Prepare statements for updating hashes
    let update_sprint_hash_stmt = &transaction
        .prepare("UPDATE levels SET sprint_leaderboard_hash = $2 WHERE id = $1")
        .await?;
    let update_challenge_hash_stmt = &transaction
        .prepare("UPDATE levels SET challenge_leaderboard_hash = $2 WHERE id = $1")
        .await?;
    let update_stunt_hash_stmt = &transaction
        .prepare("UPDATE levels SET stunt_leaderboard_hash = $2 WHERE id = $1")
        .await?;

    let futs = FuturesUnordered::new();
    for (level_id, level) in level_ids.iter().zip(data.levels.iter()) {
        if let Some((details, json)) = &level.workshop_level_details {
            let fut = async move {
                transaction
                    .execute(
                        wld_stmt,
                        &[
                            level_id,
                            json,
                            &details.tags.iter().map(|tag| &tag.tag).collect::<Vec<_>>(),
                        ],
                    )
                    .map_ok(drop)
                    .await
            };
            futs.push(fut.boxed());
        }

        // Get existing hashes for this level
        let fut = async move {
            let existing_hashes = transaction.query_one(hash_stmt, &[level_id]).await?;

            let existing_sprint_hash: Option<i64> = existing_hashes.get(0);
            let existing_challenge_hash: Option<i64> = existing_hashes.get(1);
            let existing_stunt_hash: Option<i64> = existing_hashes.get(2);

            // Sprint entries - only update if hash differs
            if level.is_sprint {
                let new_sprint_hash = compute_sprint_hash(&level.sprint_entries);
                if existing_sprint_hash.as_ref() != Some(&new_sprint_hash) {
                    // Delete existing entries for this level
                    transaction
                        .execute(
                            "DELETE FROM sprint_leaderboard_entries WHERE level_id = $1",
                            &[level_id],
                        )
                        .await?;

                    if !level.sprint_entries.is_empty() {
                        // Insert new entries
                        let sink = transaction
                            .copy_in(
                                "COPY sprint_leaderboard_entries FROM STDIN WITH (FORMAT binary)",
                            )
                            .await?;
                        let mut writer = Box::pin(BinaryCopyInWriter::new(
                            sink,
                            &[
                                PgType::INT8,
                                PgType::INT8,
                                PgType::INT4,
                                PgType::INT4,
                                PgType::BOOL,
                            ],
                        ));
                        for entry in &level.sprint_entries {
                            writer
                                .as_mut()
                                .write(&[
                                    &level_id,
                                    &(entry.steam_id as i64),
                                    &entry.time,
                                    &(entry.rank as i32),
                                    &entry.has_replay,
                                ])
                                .await?;
                        }
                        writer.as_mut().finish().await?;
                    }

                    // Update the hash
                    transaction
                        .execute(update_sprint_hash_stmt, &[level_id, &new_sprint_hash])
                        .await?;
                }
            }

            // Challenge entries - only update if hash differs
            if level.is_challenge {
                let new_challenge_hash = compute_challenge_hash(&level.challenge_entries);
                if existing_challenge_hash.as_ref() != Some(&new_challenge_hash) {
                    // Delete existing entries for this level
                    transaction
                        .execute(
                            "DELETE FROM challenge_leaderboard_entries WHERE level_id = $1",
                            &[level_id],
                        )
                        .await?;

                    if !level.challenge_entries.is_empty() {
                        // Insert new entries
                        let sink = transaction
                            .copy_in("COPY challenge_leaderboard_entries FROM STDIN WITH (FORMAT binary)")
                            .await?;
                        let mut writer = Box::pin(BinaryCopyInWriter::new(
                            sink,
                            &[
                                PgType::INT8,
                                PgType::INT8,
                                PgType::INT4,
                                PgType::INT4,
                                PgType::BOOL,
                            ],
                        ));
                        for entry in &level.challenge_entries {
                            writer
                                .as_mut()
                                .write(&[
                                    &level_id,
                                    &(entry.steam_id as i64),
                                    &entry.time,
                                    &(entry.rank as i32),
                                    &entry.has_replay,
                                ])
                                .await?;
                        }
                        writer.as_mut().finish().await?;
                    }

                    // Update the hash
                    transaction
                        .execute(update_challenge_hash_stmt, &[level_id, &new_challenge_hash])
                        .await?;
                }
            }

            // Stunt entries - only update if hash differs
            if level.is_stunt {
                let new_stunt_hash = compute_stunt_hash(&level.stunt_entries);
                if existing_stunt_hash.as_ref() != Some(&new_stunt_hash) {
                    // Delete existing entries for this level
                    transaction
                        .execute(
                            "DELETE FROM stunt_leaderboard_entries WHERE level_id = $1",
                            &[level_id],
                        )
                        .await?;

                    if !level.stunt_entries.is_empty() {
                        // Insert new entries
                        let sink = transaction
                            .copy_in(
                                "COPY stunt_leaderboard_entries FROM STDIN WITH (FORMAT binary)",
                            )
                            .await?;
                        let mut writer = Box::pin(BinaryCopyInWriter::new(
                            sink,
                            &[
                                PgType::INT8,
                                PgType::INT8,
                                PgType::INT4,
                                PgType::INT4,
                                PgType::BOOL,
                            ],
                        ));
                        for entry in &level.stunt_entries {
                            writer
                                .as_mut()
                                .write(&[
                                    &level_id,
                                    &(entry.steam_id as i64),
                                    &entry.score,
                                    &(entry.rank as i32),
                                    &entry.has_replay,
                                ])
                                .await?;
                        }
                        writer.as_mut().finish().await?;
                    }

                    // Update the hash
                    transaction
                        .execute(update_stunt_hash_stmt, &[level_id, &new_stunt_hash])
                        .await?;
                }
            }

            Ok(())
        };

        futs.push(fut.boxed());
    }

    futs.try_for_each(|_| future::ok(())).await?;

    println!("Updating 'last_updated' timestamp");
    {
        let transaction = &mut transaction_owned;
        let nested_transaction = transaction.transaction().await?;
        let result = nested_transaction
            .batch_execute("INSERT INTO metadata (last_updated) VALUES (now())")
            .await;
        match result {
            // No timestamp existed
            Ok(_) => {
                nested_transaction.commit().await?;
            }
            // Timestamp already existed
            Err(e) if e.code() == Some(&tokio_postgres::error::SqlState::UNIQUE_VIOLATION) => {
                nested_transaction.rollback().await?;
                transaction
                    .batch_execute("UPDATE metadata SET last_updated = now()")
                    .await?;
            }
            // Some other error
            Err(e) => {
                return Err(e.into());
            }
        }
    }

    println!("Committing changes");
    transaction_owned.commit().await?;

    Ok(())
}
