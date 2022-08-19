use crate::common::DistanceData;
use anyhow::Error;
use futures::prelude::*;
use futures::stream::{self, FuturesOrdered, FuturesUnordered};
use steamworks::ugc::PublishedFileVisibility;

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
        .prepare("INSERT INTO levels (name, is_sprint, is_challenge, is_stunt) VALUES ($1, $2, $3, $4) RETURNING id")
        .await?;
    let level_ids: Vec<i32> = data
        .levels
        .iter()
        .map(|level| async move {
            let row = &transaction
                .query(
                    stmt,
                    &[
                        &level.name,
                        &level.is_sprint,
                        &level.is_challenge,
                        &level.is_stunt,
                    ],
                )
                .await?[0];
            let id: i32 = row.get(0);

            Ok::<_, Error>(id)
        })
        .collect::<FuturesOrdered<_>>()
        .try_collect()
        .await?;

    println!("Inserting the rest of the data into the database");
    let wld_stmt = &transaction
        .prepare("INSERT INTO workshop_level_details VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)")
        .await?;
    let sprint_stmt = &transaction
        .prepare("INSERT INTO sprint_leaderboard_entries VALUES ($1, $2, $3, $4, $5)")
        .await?;
    let challenge_stmt = &transaction
        .prepare("INSERT INTO challenge_leaderboard_entries VALUES ($1, $2, $3, $4, $5)")
        .await?;
    let stunt_stmt = &transaction
        .prepare("INSERT INTO stunt_leaderboard_entries VALUES ($1, $2, $3, $4, $5)")
        .await?;
    let futs = FuturesUnordered::new();
    for (level_id, level) in level_ids.iter().zip(data.levels.iter()) {
        if let Some(details) = &level.workshop_level_details {
            let visibility = match details.visibility {
                PublishedFileVisibility::Public => "public",
                PublishedFileVisibility::FriendsOnly => "friends_only",
                PublishedFileVisibility::Private => "private",
            };
            let fut = async move {
                transaction
                    .execute(
                        wld_stmt,
                        &[
                            &level_id,
                            &(details.steam_id_owner.as_u64() as i64),
                            &details.description,
                            &details.time_created,
                            &details.time_updated,
                            &visibility,
                            &details.tags.as_str(),
                            &details.preview_url,
                            &details.file_name,
                            &details.file_size,
                            &(details.votes_up as i32),
                            &(details.votes_down as i32),
                            &details.score,
                        ],
                    )
                    .map_ok(drop)
                    .await
            };
            futs.push(fut.boxed());
        }

        for entry in &level.sprint_entries {
            let fut = async move {
                transaction
                    .execute(
                        sprint_stmt,
                        &[
                            &level_id,
                            &(entry.steam_id as i64),
                            &entry.time,
                            &(entry.rank as i32),
                            &entry.has_replay,
                        ],
                    )
                    .map_ok(drop)
                    .await
            };
            futs.push(fut.boxed());
        }

        for entry in &level.challenge_entries {
            let fut = async move {
                transaction
                    .execute(
                        challenge_stmt,
                        &[
                            &level_id,
                            &(entry.steam_id as i64),
                            &entry.time,
                            &(entry.rank as i32),
                            &entry.has_replay,
                        ],
                    )
                    .map_ok(drop)
                    .await
            };
            futs.push(fut.boxed());
        }

        for entry in &level.stunt_entries {
            let fut = async move {
                transaction
                    .execute(
                        stunt_stmt,
                        &[
                            &level_id,
                            &(entry.steam_id as i64),
                            &entry.score,
                            &(entry.rank as i32),
                            &entry.has_replay,
                        ],
                    )
                    .map_ok(drop)
                    .await
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
