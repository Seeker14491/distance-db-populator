use crate::common::DistanceData;
use anyhow::Error;
use futures::{
    stream::{FuturesOrdered, FuturesUnordered},
    FutureExt, TryFutureExt, TryStreamExt,
};
use steamworks::ugc::PublishedFileVisibility;
use tokio_postgres::error::SqlState;

pub async fn run(db: &mut tokio_postgres::Client, data: DistanceData) -> Result<(), Error> {
    let mut tr_owned = db.transaction().await?;
    let tr = &tr_owned;

    println!("Clearing the database");
    tr.batch_execute("TRUNCATE levels, users CASCADE").await?;

    println!("Inserting users into the database");
    let stmt = &tr
        .prepare("INSERT INTO users VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .await?;
    data.users
        .iter()
        .map(|user| async move {
            tr.execute(stmt, &[&(user.steam_id as i64), &user.name])
                .map_ok(drop)
                .await
        })
        .collect::<FuturesUnordered<_>>()
        .try_collect::<Vec<_>>()
        .await?;

    println!("Inserting levels into the database");
    let stmt = &tr
        .prepare("INSERT INTO levels (name, is_sprint, is_challenge, is_stunt) VALUES ($1, $2, $3, $4) RETURNING id")
        .await?;
    let level_ids: Vec<i32> = data
        .levels
        .iter()
        .map(|level| async move {
            let row = &tr
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
    let wld_stmt = &tr
        .prepare("INSERT INTO workshop_level_details VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)")
        .await?;
    let sprint_stmt = &tr
        .prepare("INSERT INTO sprint_leaderboard_entries VALUES ($1, $2, $3, $4)")
        .await?;
    let challenge_stmt = &tr
        .prepare("INSERT INTO challenge_leaderboard_entries VALUES ($1, $2, $3, $4)")
        .await?;
    let stunt_stmt = &tr
        .prepare("INSERT INTO stunt_leaderboard_entries VALUES ($1, $2, $3, $4)")
        .await?;
    let futs = FuturesUnordered::new();
    level_ids
        .iter()
        .zip(data.levels.iter())
        .map(|(level_id, level)| {
            if let Some(details) = &level.workshop_level_details {
                let visibility = match details.visibility {
                    PublishedFileVisibility::Public => "public",
                    PublishedFileVisibility::FriendsOnly => "friends_only",
                    PublishedFileVisibility::Private => "private",
                };
                let fut = async move {
                    tr.execute(
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
                    tr.execute(
                        sprint_stmt,
                        &[
                            &level_id,
                            &(entry.steam_id as i64),
                            &entry.time,
                            &(entry.rank as i32),
                        ],
                    )
                    .map_ok(drop)
                    .await
                };
                futs.push(fut.boxed());
            }

            for entry in &level.challenge_entries {
                let fut = async move {
                    tr.execute(
                        challenge_stmt,
                        &[
                            &level_id,
                            &(entry.steam_id as i64),
                            &entry.time,
                            &(entry.rank as i32),
                        ],
                    )
                    .map_ok(drop)
                    .await
                };
                futs.push(fut.boxed());
            }

            for entry in &level.stunt_entries {
                let fut = async move {
                    tr.execute(
                        stunt_stmt,
                        &[
                            &level_id,
                            &(entry.steam_id as i64),
                            &entry.score,
                            &(entry.rank as i32),
                        ],
                    )
                    .map_ok(drop)
                    .await
                };
                futs.push(fut.boxed());
            }
        })
        .for_each(drop);

    futs.try_collect().await?;

    println!("Updating 'last_updated' timestamp");
    {
        let tr = &mut tr_owned;
        let tr_2 = tr.transaction().await?;
        let result = tr_2
            .batch_execute("INSERT INTO metadata (last_updated) VALUES (now())")
            .await;
        match result {
            Ok(_) => {
                tr_2.commit().await?;
                return Ok(());
            }
            Err(e) if e.code() != Some(&SqlState::UNIQUE_VIOLATION) => {
                tr_2.commit().await?;
                return Err(e.into());
            }
            _ => tr_2.rollback().await?,
        }

        tr.batch_execute("UPDATE metadata SET last_updated = now()")
            .await?;
    }

    tr_owned.commit().await?;

    Ok(())
}
