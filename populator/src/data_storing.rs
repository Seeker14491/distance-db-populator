use crate::common::DistanceData;
use anyhow::Error;
use chrono::{TimeZone, Utc};
use futures::prelude::*;
use futures::stream::{self, FuturesOrdered, FuturesUnordered};
use itertools::Itertools;
use tokio_postgres::binary_copy::BinaryCopyInWriter;
use tokio_postgres::types::Type as PgType;

pub async fn run(db: &mut tokio_postgres::Client, data: DistanceData) -> Result<(), Error> {
    let mut transaction_owned = db.transaction().await?;
    let transaction = &transaction_owned;

    println!("Clearing the database");
    transaction
        .batch_execute("TRUNCATE levels, users CASCADE")
        .await?;

    println!("Inserting users into the database");
    let stmt = &transaction
        .prepare("INSERT INTO users VALUES ($1, $2) ON CONFLICT DO NOTHING")
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

    println!("Inserting levels into the database");
    let stmt = &transaction
        .prepare("INSERT INTO levels (id, name, is_sprint, is_challenge, is_stunt) VALUES ($1, $2, $3, $4, $5)")
        .await?;
    let level_ids: Vec<_> = data
        .levels
        .iter()
        .map(|level| async move {
            transaction
                .query(
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

    println!("Inserting the rest of the data into the database");
    let wld_stmt = &transaction
        .prepare("INSERT INTO workshop_level_details VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)")
        .await?;

    let futs = FuturesUnordered::new();
    for (level_id, level) in level_ids.iter().zip(data.levels.iter()) {
        if let Some((details, _json)) = &level.workshop_level_details {
            let visibility = match details.visibility {
                0 => "public",
                1 => "friends_only",
                2 => "private",
                _ => panic!("unexpected visibility discriminant: {}", details.visibility),
            };
            let fut = async move {
                transaction
                    .execute(
                        wld_stmt,
                        &[
                            &level_id,
                            &(details.creator as i64),
                            &details.file_description,
                            &Utc.timestamp_opt(details.time_created as i64, 0).unwrap(),
                            &Utc.timestamp_opt(details.time_updated as i64, 0).unwrap(),
                            &visibility,
                            &details.tags.iter().map(|tag| &tag.tag).join(",").as_str(),
                            &details.preview_url,
                            &details.filename,
                            &(details.file_size as i32),
                            &(details.vote_data.votes_up as i32),
                            &(details.vote_data.votes_down as i32),
                            &details.vote_data.score,
                        ],
                    )
                    .map_ok(drop)
                    .await
            };
            futs.push(fut.boxed());
        }

        // Sprint entries
        {
            let fut = async move {
                let sink = transaction
                    .copy_in("COPY sprint_leaderboard_entries FROM STDIN WITH (FORMAT binary)")
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

                Ok(())
            };

            futs.push(fut.boxed());
        }

        // Challenge entries
        {
            let fut = async move {
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

                Ok(())
            };

            futs.push(fut.boxed());
        }

        // Stunt entries
        {
            let fut = async move {
                let sink = transaction
                    .copy_in("COPY stunt_leaderboard_entries FROM STDIN WITH (FORMAT binary)")
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

                Ok(())
            };

            futs.push(fut.boxed());
        }
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
