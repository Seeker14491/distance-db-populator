use super::{LevelDifficulty, MusicCueId};
use failure::Error;
use serde::Deserialize;
use std::{collections::HashMap, fs::File, io::BufReader, path::Path};

#[derive(Debug, Deserialize)]
pub struct LevelDump {
    #[serde(rename = "OfficialLevels")]
    pub official_levels: Vec<OfficialLevel>,
}

#[derive(Debug, Deserialize)]
pub struct OfficialLevel {
    #[serde(rename = "levelName_")]
    pub level_name: String,

    #[serde(rename = "relativePath_")]
    pub relative_path: String,

    #[serde(rename = "fileNameWithoutExtension_")]
    pub file_name_without_extension: String,

    #[serde(rename = "levelVersionDateTime_")]
    pub level_version_date_time: String,

    #[serde(rename = "fileLastWriteDateTime_")]
    pub file_last_write_date_time: String,

    #[serde(rename = "levelDescription_")]
    pub level_description: String,

    #[serde(rename = "levelCreatorName_")]
    pub level_creator_name: String,

    #[serde(rename = "modes_")]
    pub modes: HashMap<String, bool>,

    #[serde(rename = "bronzeTime_")]
    pub bronze_time: f32,

    #[serde(rename = "bronzePoints_")]
    pub bronze_points: i32,

    #[serde(rename = "silverTime_")]
    pub silver_time: f32,

    #[serde(rename = "silverPoints_")]
    pub silver_points: i32,

    #[serde(rename = "goldTime_")]
    pub gold_time: f32,

    #[serde(rename = "goldPoints_")]
    pub gold_points: i32,

    #[serde(rename = "diamondTime_")]
    pub diamond_time: f32,

    #[serde(rename = "diamondPoints_")]
    pub diamond_points: i32,

    #[serde(rename = "infiniteCooldown_")]
    pub infinite_cooldown: bool,

    #[serde(rename = "disableFlying_")]
    pub disable_flying: bool,

    #[serde(rename = "disableJumping_")]
    pub disable_jumping: bool,

    #[serde(rename = "disableBoosting_")]
    pub disable_boosting: bool,

    #[serde(rename = "disableJetRotating_")]
    pub disable_jet_rotating: bool,

    #[serde(rename = "difficulty_")]
    pub difficulty: LevelDifficulty,

    #[serde(rename = "levelType_")]
    pub level_type: u8,

    #[serde(rename = "workshopCreatorID_")]
    pub workshop_creator_id: u64,

    #[serde(rename = "musicCueID_")]
    pub music_cue_id: MusicCueId,

    #[serde(rename = "isAdventure_")]
    pub is_adventure: bool,

    #[serde(rename = "isEchoes_")]
    pub is_echoes: bool,

    #[serde(rename = "isOldLevel_")]
    pub is_old_level: bool,
}

fn parse_file<P: AsRef<Path>>(path: P) -> Result<Vec<OfficialLevel>, Error> {
    let data: LevelDump = serde_json::from_reader(BufReader::new(File::open(path.as_ref())?))?;

    Ok(data.official_levels)
}

#[test]
fn test_parse_file() {
    parse_file("test_samples/export.json").unwrap();
}
