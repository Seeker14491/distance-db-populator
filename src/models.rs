use crate::schema::*;
use chrono::NaiveDateTime;

#[derive(Queryable, Debug, Clone)]
pub struct User {
    pub steam_id: i64,
    pub name: String,
}

#[derive(Insertable)]
#[table_name = "users"]
#[derive(Debug, Clone)]
pub struct NewUser {
    pub steam_id: i64,
    pub name: String,
}

#[derive(Queryable, Debug, Clone)]
pub struct Level {
    pub id: i32,
    pub name: String,
    pub is_sprint: bool,
    pub is_challenge: bool,
    pub is_stunt: bool,
}

#[derive(Insertable)]
#[table_name = "levels"]
#[derive(Debug, Copy, Clone)]
pub struct NewLevel<'a> {
    pub name: &'a str,
    pub is_sprint: bool,
    pub is_challenge: bool,
    pub is_stunt: bool,
}

#[derive(Queryable, Debug, Clone)]
pub struct WorkshopLevelDetails {
    pub id: i32,
    pub author_steam_id: i64,
    pub description: String,
    pub time_created: NaiveDateTime,
    pub time_updated: NaiveDateTime,
    pub visibility: String,
    pub tags: String,
    pub preview_url: String,
    pub file_name: String,
    pub file_size: i32,
    pub votes_up: i32,
    pub votes_down: i32,
    pub score: f32,
}

#[derive(Insertable)]
#[table_name = "workshop_level_details"]
#[derive(Debug, Clone)]
pub struct NewWorkshopLevelDetails<'a> {
    pub level_id: i32,
    pub author_steam_id: i64,
    pub description: &'a str,
    pub time_created: NaiveDateTime,
    pub time_updated: NaiveDateTime,
    pub visibility: &'a str,
    pub tags: &'a str,
    pub preview_url: &'a str,
    pub file_name: &'a str,
    pub file_size: i32,
    pub votes_up: i32,
    pub votes_down: i32,
    pub score: f32,
}

#[derive(Queryable, Debug, Copy, Clone)]
pub struct SprintLeaderboardEntry {
    pub level_id: i32,
    pub steam_id: i64,
    pub time: i32,
}

#[derive(Insertable)]
#[table_name = "sprint_leaderboard_entries"]
#[derive(Debug, Copy, Clone)]
pub struct NewSprintLeaderboardEntry {
    pub level_id: i32,
    pub steam_id: i64,
    pub time: i32,
}

#[derive(Queryable, Debug, Copy, Clone)]
pub struct ChallengeLeaderboardEntry {
    pub level_id: i32,
    pub steam_id: i64,
    pub time: i32,
}

#[derive(Insertable)]
#[table_name = "challenge_leaderboard_entries"]
#[derive(Debug, Copy, Clone)]
pub struct NewChallengeLeaderboardEntry {
    pub level_id: i32,
    pub steam_id: i64,
    pub time: i32,
}

#[derive(Queryable, Debug, Copy, Clone)]
pub struct StuntLeaderboardEntry {
    pub level_id: i32,
    pub steam_id: i64,
    pub score: i32,
}

#[derive(Insertable)]
#[table_name = "stunt_leaderboard_entries"]
#[derive(Debug, Copy, Clone)]
pub struct NewStuntLeaderboardEntry {
    pub level_id: i32,
    pub steam_id: i64,
    pub score: i32,
}
