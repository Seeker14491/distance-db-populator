use steam_workshop::types::PublishedFileDetails;

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
    pub workshop_level_details: Option<PublishedFileDetails>,
    pub sprint_entries: Vec<TimeLeaderboardEntry>,
    pub challenge_entries: Vec<TimeLeaderboardEntry>,
    pub stunt_entries: Vec<ScoreLeaderboardEntry>,
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
