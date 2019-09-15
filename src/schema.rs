table! {
    challenge_leaderboard_entries (level_id, steam_id) {
        level_id -> Int4,
        steam_id -> Int8,
        time -> Int4,
    }
}

table! {
    levels (id) {
        id -> Int4,
        name -> Varchar,
        is_sprint -> Bool,
        is_challenge -> Bool,
        is_stunt -> Bool,
    }
}

table! {
    sprint_leaderboard_entries (steam_id, level_id) {
        level_id -> Int4,
        steam_id -> Int8,
        time -> Int4,
    }
}

table! {
    stunt_leaderboard_entries (level_id, steam_id) {
        level_id -> Int4,
        steam_id -> Int8,
        score -> Int4,
    }
}

table! {
    users (steam_id) {
        steam_id -> Int8,
        name -> Varchar,
    }
}

table! {
    workshop_level_details (level_id) {
        level_id -> Int4,
        author_steam_id -> Int8,
        description -> Varchar,
        time_created -> Timestamptz,
        time_updated -> Timestamptz,
        visibility -> Varchar,
        tags -> Varchar,
        preview_url -> Varchar,
        file_name -> Varchar,
        file_size -> Int4,
        votes_up -> Int4,
        votes_down -> Int4,
        score -> Float4,
    }
}

joinable!(challenge_leaderboard_entries -> levels (level_id));
joinable!(challenge_leaderboard_entries -> users (steam_id));
joinable!(sprint_leaderboard_entries -> levels (level_id));
joinable!(sprint_leaderboard_entries -> users (steam_id));
joinable!(stunt_leaderboard_entries -> levels (level_id));
joinable!(stunt_leaderboard_entries -> users (steam_id));
joinable!(workshop_level_details -> levels (level_id));
joinable!(workshop_level_details -> users (author_steam_id));

allow_tables_to_appear_in_same_query!(
    challenge_leaderboard_entries,
    levels,
    sprint_leaderboard_entries,
    stunt_leaderboard_entries,
    users,
    workshop_level_details,
);
