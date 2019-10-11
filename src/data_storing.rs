use crate::common::DistanceData;
use failure::Error;
use futures::{
    pin_mut,
    stream::{FuturesOrdered, FuturesUnordered},
    StreamExt, TryFutureExt, TryStreamExt,
};
use steamworks::ugc::PublishedFileVisibility;

pub async fn run(mut db: tokio_postgres::Client, data: DistanceData) -> Result<(), Error> {
    println!("Clearing the database");
    db.batch_execute("TRUNCATE levels, users CASCADE").await?;

    println!("Inserting users into the database");
    let stmt = db
        .prepare("INSERT INTO users VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .await?;
    data.users
        .iter()
        .map(|user| {
            db.execute(&stmt, &[&(user.steam_id as i64), &user.name])
                .map_ok(drop)
        })
        .collect::<FuturesUnordered<_>>()
        .try_collect::<Vec<_>>()
        .await?;

    println!("Inserting levels into the database");
    let stmt = db
        .prepare("INSERT INTO levels (name, is_sprint, is_challenge, is_stunt) VALUES ($1, $2, $3, $4) RETURNING id")
        .await?;
    let level_ids: Vec<i32> = data
        .levels
        .iter()
        .map(|level| {
            let fut = db.query(
                &stmt,
                &[
                    &level.name,
                    &level.is_sprint,
                    &level.is_challenge,
                    &level.is_stunt,
                ],
            );

            async move {
                pin_mut!(fut);
                let row = fut.next().await.unwrap()?;
                let id: i32 = row.get(0);

                Ok::<_, Error>(id)
            }
        })
        .collect::<FuturesOrdered<_>>()
        .try_collect()
        .await?;

    println!("Inserting the rest of the data into the database");
    let wld_stmt = db
        .prepare("INSERT INTO workshop_level_details VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)")
        .await?;
    let sprint_stmt = db
        .prepare("INSERT INTO sprint_leaderboard_entries VALUES ($1, $2, $3)")
        .await?;
    let challenge_stmt = db
        .prepare("INSERT INTO challenge_leaderboard_entries VALUES ($1, $2, $3)")
        .await?;
    let stunt_stmt = db
        .prepare("INSERT INTO stunt_leaderboard_entries VALUES ($1, $2, $3)")
        .await?;
    let mut futs = FuturesUnordered::new();
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
                let fut = db
                    .execute(
                        &wld_stmt,
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
                    .map_ok(drop);
                futs.push(fut);
            }

            for entry in &level.sprint_entries {
                let fut = db
                    .execute(
                        &sprint_stmt,
                        &[&level_id, &(entry.steam_id as i64), &entry.time],
                    )
                    .map_ok(drop);
                futs.push(fut);
            }

            for entry in &level.challenge_entries {
                let fut = db
                    .execute(
                        &challenge_stmt,
                        &[&level_id, &(entry.steam_id as i64), &entry.time],
                    )
                    .map_ok(drop);
                futs.push(fut);
            }

            for entry in &level.stunt_entries {
                let fut = db
                    .execute(
                        &stunt_stmt,
                        &[&level_id, &(entry.steam_id as i64), &entry.score],
                    )
                    .map_ok(drop);
                futs.push(fut);
            }
        })
        .for_each(drop);

    futs.try_collect().await?;

    Ok(())
}
