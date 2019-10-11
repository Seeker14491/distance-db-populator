use steamworks::{ugc::UgcDetails, user_stats::LeaderboardEntry};

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
    pub name: String,
    pub is_sprint: bool,
    pub is_challenge: bool,
    pub is_stunt: bool,
    pub workshop_level_details: Option<UgcDetails>,
    pub sprint_entries: Vec<TimeLeaderboardEntry>,
    pub challenge_entries: Vec<TimeLeaderboardEntry>,
    pub stunt_entries: Vec<ScoreLeaderboardEntry>,
}

#[derive(Debug, Copy, Clone)]
pub struct TimeLeaderboardEntry {
    pub steam_id: u64,
    pub time: i32,
}

impl From<LeaderboardEntry> for TimeLeaderboardEntry {
    fn from(raw: LeaderboardEntry) -> Self {
        TimeLeaderboardEntry {
            steam_id: raw.steam_id.into(),
            time: raw.score,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct ScoreLeaderboardEntry {
    pub steam_id: u64,
    pub score: i32,
}

impl From<LeaderboardEntry> for ScoreLeaderboardEntry {
    fn from(raw: LeaderboardEntry) -> Self {
        ScoreLeaderboardEntry {
            steam_id: raw.steam_id.into(),
            score: raw.score,
        }
    }
}

#[derive(Debug, Clone)]
pub struct User {
    pub steam_id: u64,
    pub name: String,
}
