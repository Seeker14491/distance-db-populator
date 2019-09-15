macro_rules! add_users {
    ($db:expr, $steam:expr, $users:expr) => {{
        let db: Rc<PgConnection> = $db;
        let steam: &steamworks::Client = $steam;
        let mut users: Vec<SteamId> = $users;

        let steam = steam.clone();
        async move {
            users.sort_unstable();

            let authors: Vec<NewUser> = users
                .into_iter()
                .dedup()
                .map(|steam_id| {
                    let steam = steam.clone();
                    async move {
                        let name = steam_id.persona_name(&steam).await;

                        NewUser {
                            steam_id: steam_id.as_u64() as i64,
                            name,
                        }
                    }
                })
                .collect::<FuturesUnordered<_>>()
                .collect()
                .await;

            insert_into_chunked(schema::users::table, &authors, |query| {
                query
                    .on_conflict(schema::users::steam_id)
                    .do_update()
                    .set(
                        schema::users::name::default()
                            .eq(upsert::excluded(schema::users::name::default())),
                    )
                    .execute(&*db)
            })
        }
    }};
}
