use serde::Deserialize;
use serde_json::Value as JsonValue;
use serde_with::{DisplayFromStr, serde_as};

#[derive(Debug, Clone, Default)]
pub struct DistanceData {
    pub levels: Vec<Level>,
    pub users: Vec<User>,
}

impl DistanceData {
    pub fn new() -> Self {
        DistanceData::default()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Level {
    pub id: i64,
    pub name: String,
    pub is_sprint: bool,
    pub is_challenge: bool,
    pub is_stunt: bool,
    pub workshop_level_details: Option<(PublishedFileDetailsSubset, JsonValue)>,
    pub sprint_entries: Vec<TimeLeaderboardEntry>,
    pub challenge_entries: Vec<TimeLeaderboardEntry>,
    pub stunt_entries: Vec<ScoreLeaderboardEntry>,
}

#[serde_as]
#[derive(Debug, Clone, Deserialize)]
pub struct PublishedFileDetailsSubset {
    #[serde_as(as = "DisplayFromStr")]
    #[serde(rename = "publishedfileid")]
    pub published_file_id: i64,

    #[serde_as(as = "DisplayFromStr")]
    pub creator: u64,

    pub filename: String,

    #[serde_as(as = "DisplayFromStr")]
    pub file_size: i64,
    pub title: String,
    pub tags: Vec<Tag>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Tag {
    pub tag: String,
}

#[derive(Debug, Copy, Clone)]
pub struct TimeLeaderboardEntry {
    pub steam_id: u64,
    pub time: i32,
    pub rank: u32,
    pub has_replay: bool,
}

#[derive(Debug, Copy, Clone)]
pub struct ScoreLeaderboardEntry {
    pub steam_id: u64,
    pub score: i32,
    pub rank: u32,
    pub has_replay: bool,
}

#[derive(Debug, Clone)]
pub struct User {
    pub steam_id: u64,
    pub name: String,
}
