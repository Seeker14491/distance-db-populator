//! Extracting data from the contents of a level file.
//!
//! The rules here follow how the game itself reads a level's settings (`LevelSettings` and
//! `LevelInfo` in the game's code), so the extracted data is what the game uses.

use anyhow::{Result, bail};
use distance_bytes::components::LevelSettings;
use distance_bytes::{
    BytesFile, ComponentBody, ComponentData, ComponentId, LegacyLevelSettings, LegacySettingsBody,
    LevelSettingsData,
};
use serde::{Deserialize, Serialize};

/// The version of the data that [`extract`] produces. Bump it whenever `extract` changes what it
/// produces (such as when more data is extracted), so that every level is processed again.
pub const EXTRACTOR_VERSION: i32 = 1;

// Values of the game's `GameModeID` enum.
const SPRINT: i32 = 1;
const STUNT: i32 = 2;
const CHALLENGE: i32 = 8;

/// The data extracted from a level file.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LevelFileData {
    /// The game modes the file enables. The game offers a workshop level in these modes, but
    /// offers official levels in the modes of its own level lists instead.
    pub modes: Modes,
    /// Medal times in milliseconds, if the level has all four set. The game uses them in Sprint
    /// and Challenge mode.
    pub time_medals: Option<Medals>,
    /// Medal scores, if the level has all four set. The game uses them in Stunt mode.
    pub score_medals: Option<Medals>,
}

impl LevelFileData {
    /// The time and score medals of a level played in the given modes: medal times if one of
    /// them is Sprint or Challenge, and medal scores if one is Stunt.
    pub fn medals_in(&self, modes: Modes) -> (Option<Medals>, Option<Medals>) {
        (
            self.time_medals.filter(|_| modes.sprint || modes.challenge),
            self.score_medals.filter(|_| modes.stunt),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Modes {
    pub sprint: bool,
    pub challenge: bool,
    pub stunt: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Medals {
    pub bronze: i32,
    pub silver: i32,
    pub gold: i32,
    pub diamond: i32,
}

/// Extracts data from the contents of a `.bytes` level file.
pub fn extract(file: &[u8]) -> Result<LevelFileData> {
    let BytesFile::Level(level) = BytesFile::read(file)? else {
        bail!("not a level file");
    };

    let settings = Settings::from_level_settings(&level.settings)?;

    Ok(LevelFileData {
        modes: Modes {
            sprint: settings.mode_enabled(SPRINT),
            challenge: settings.mode_enabled(CHALLENGE),
            stunt: settings.mode_enabled(STUNT),
        },
        time_medals: settings.time_medals(),
        score_medals: settings.score_medals(),
    })
}

/// The level settings values we use. `None` values are left to the game's defaults.
#[derive(Debug, Default)]
struct Settings {
    /// `GameModeID`s and whether each is enabled, in file order.
    modes: Vec<(Option<i32>, Option<bool>)>,
    /// Bronze, silver, gold, diamond.
    times: [Option<f32>; 4],
    /// Bronze, silver, gold, diamond.
    points: [Option<i32>; 4],
}

impl Settings {
    fn from_level_settings(settings: &LevelSettingsData) -> Result<Self> {
        match settings {
            LevelSettingsData::GameObject(obj) => {
                let Some(component) = obj
                    .components
                    .iter()
                    .find(|c| c.id() == Some(ComponentId::LEVEL_SETTINGS))
                else {
                    bail!("the level settings object has no `LevelSettings` component");
                };

                match &component.body {
                    ComponentBody::Data {
                        data: ComponentData::LevelSettings(s),
                        ..
                    } => Ok(Self::from_modern(s)),
                    // The component is identical to the prefab's, so everything is a default.
                    ComponentBody::Empty => Ok(Self::default()),
                    _ => bail!("couldn't decode the `LevelSettings` component"),
                }
            }
            LevelSettingsData::Legacy { body, .. } => match body {
                LegacySettingsBody::Data { data, .. } => Ok(Self::from_legacy(data)),
                LegacySettingsBody::Raw(_) => bail!("couldn't decode the (legacy) level settings"),
            },
        }
    }

    fn from_modern(s: &LevelSettings) -> Self {
        Settings {
            modes: s.modes.clone(),
            times: [s.bronze_time, s.silver_time, s.gold_time, s.diamond_time],
            points: [
                s.bronze_points,
                s.silver_points,
                s.gold_points,
                s.diamond_points,
            ],
        }
    }

    fn from_legacy(s: &LegacyLevelSettings) -> Self {
        // Settings versions 1 to 3 store modes by name, which the game maps to `GameModeID`s.
        let named_modes = s.mode_names.iter().filter_map(|(name, enabled)| {
            let id = match name
                .as_ref()?
                .to_string_lossy()
                .to_ascii_lowercase()
                .as_str()
            {
                "sprint" => SPRINT,
                "stunt" => STUNT,
                "challenge" => CHALLENGE,
                _ => return None,
            };
            Some((Some(id), *enabled))
        });

        Settings {
            modes: s.modes.iter().copied().chain(named_modes).collect(),
            times: [s.bronze_time, s.silver_time, s.gold_time, s.diamond_time],
            points: [
                s.bronze_points,
                s.silver_points,
                s.gold_points,
                s.diamond_points,
            ],
        }
    }

    /// Whether the game mode is enabled. Modes the file doesn't list get the game's defaults
    /// (`LevelSettings.GetDefaultModesMap`): only Sprint is enabled.
    fn mode_enabled(&self, mode: i32) -> bool {
        // The modes are a dictionary, so a later entry replaces an earlier one.
        match self.modes.iter().rev().find(|(id, _)| *id == Some(mode)) {
            Some((_, enabled)) => enabled.unwrap_or(false),
            None => mode == SPRINT,
        }
    }

    /// The game only has medals for a time-based mode if all four times are positive
    /// (`LevelInfo.SupportsMedals`). Missing values default to 0.
    fn time_medals(&self) -> Option<Medals> {
        let [bronze, silver, gold, diamond] = self.times.map(|t| t.unwrap_or(0.0));
        if ![bronze, silver, gold, diamond].iter().all(|&t| t > 0.0) {
            return None;
        }

        Some(Medals {
            bronze: medal_time_ms(bronze),
            silver: medal_time_ms(silver),
            gold: medal_time_ms(gold),
            diamond: medal_time_ms(diamond),
        })
    }

    /// Like [`Settings::time_medals`], the game requires all four scores to be positive.
    fn score_medals(&self) -> Option<Medals> {
        let [bronze, silver, gold, diamond] = self.points.map(|p| p.unwrap_or(0));
        if ![bronze, silver, gold, diamond].iter().all(|&p| p > 0) {
            return None;
        }

        Some(Medals {
            bronze,
            silver,
            gold,
            diamond,
        })
    }
}

/// Converts a medal time, which the game stores as a float of milliseconds, to whole
/// milliseconds. The game awards a time medal when `time <= medal_time`, so rounding down keeps
/// that comparison exact for the whole-millisecond times of leaderboard entries. Times too large
/// for an `i32` saturate.
fn medal_time_ms(time: f32) -> i32 {
    time.floor() as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use distance_bytes::{BodyEnd, Component, GameObject, Level};

    fn level_file(settings: LevelSettings) -> Vec<u8> {
        let mut level = Level::new("Test");
        level.settings = LevelSettingsData::GameObject(
            GameObject::new("LevelSettings")
                .with(Component::empty(ComponentId::TRANSFORM))
                .with(settings),
        );
        BytesFile::Level(level).write()
    }

    fn settings(modes: &[(i32, bool)], times: [f32; 4], points: [i32; 4]) -> LevelSettings {
        LevelSettings {
            modes: modes.iter().map(|&(m, e)| (Some(m), Some(e))).collect(),
            bronze_time: Some(times[0]),
            silver_time: Some(times[1]),
            gold_time: Some(times[2]),
            diamond_time: Some(times[3]),
            bronze_points: Some(points[0]),
            silver_points: Some(points[1]),
            gold_points: Some(points[2]),
            diamond_points: Some(points[3]),
            ..LevelSettings::default()
        }
    }

    const TIMES: [f32; 4] = [190000.0, 124000.0, 95000.0, 89000.0];
    const POINTS: [i32; 4] = [1600, 2000, 2600, 4000];

    fn medals([bronze, silver, gold, diamond]: [i32; 4]) -> Option<Medals> {
        Some(Medals {
            bronze,
            silver,
            gold,
            diamond,
        })
    }

    const TIME_MEDALS: [i32; 4] = [190000, 124000, 95000, 89000];

    /// The medals of a workshop level, which the game offers in the modes its file enables.
    fn workshop_medals(file: &[u8]) -> (Option<Medals>, Option<Medals>) {
        let data = extract(file).unwrap();
        data.medals_in(data.modes)
    }

    #[test]
    fn sprint_level_has_only_time_medals() {
        let file = level_file(settings(&[(SPRINT, true), (STUNT, false)], TIMES, POINTS));
        assert_eq!(workshop_medals(&file), (medals(TIME_MEDALS), None));
    }

    #[test]
    fn stunt_level_has_only_score_medals() {
        let file = level_file(settings(&[(SPRINT, false), (STUNT, true)], TIMES, POINTS));
        assert_eq!(workshop_medals(&file), (None, medals(POINTS)));
    }

    #[test]
    fn challenge_level_has_time_medals() {
        let file = level_file(settings(
            &[(SPRINT, false), (CHALLENGE, true)],
            TIMES,
            POINTS,
        ));
        assert_eq!(workshop_medals(&file), (medals(TIME_MEDALS), None));
    }

    #[test]
    fn official_level_modes_can_differ_from_the_file() {
        // Like the official Sprint level Deterrence, whose file has every mode disabled.
        let file = level_file(settings(&[(SPRINT, false)], TIMES, POINTS));
        let data = extract(&file).unwrap();
        assert_eq!(data.modes, Modes::default());
        let sprint = Modes {
            sprint: true,
            ..Modes::default()
        };
        assert_eq!(data.medals_in(sprint), (medals(TIME_MEDALS), None));
    }

    #[test]
    fn sprint_is_enabled_unless_listed() {
        let file = level_file(settings(&[(CHALLENGE, false)], TIMES, POINTS));
        let modes = extract(&file).unwrap().modes;
        assert_eq!(
            modes,
            Modes {
                sprint: true,
                ..Modes::default()
            }
        );
    }

    #[test]
    fn later_mode_entries_win() {
        let file = level_file(settings(&[(STUNT, false), (STUNT, true)], TIMES, POINTS));
        assert!(extract(&file).unwrap().modes.stunt);
    }

    #[test]
    fn unset_medals_are_none() {
        let file = level_file(settings(
            &[(SPRINT, true), (STUNT, true)],
            [-1.0, -1.0, 95000.0, -1.0],
            [0, 2000, 2600, 4000],
        ));
        let data = extract(&file).unwrap();
        assert_eq!((data.time_medals, data.score_medals), (None, None));
    }

    #[test]
    fn missing_medal_values_default_to_zero() {
        let file = level_file(LevelSettings {
            gold_time: Some(95000.0),
            ..LevelSettings::default()
        });
        let data = extract(&file).unwrap();
        assert_eq!((data.time_medals, data.score_medals), (None, None));
    }

    #[test]
    fn medal_times_round_down() {
        let file = level_file(settings(
            &[(SPRINT, true)],
            [1e12, f32::INFINITY, 95000.75, 0.5],
            POINTS,
        ));
        assert_eq!(
            extract(&file).unwrap().time_medals,
            medals([i32::MAX, i32::MAX, 95000, 0])
        );
    }

    #[test]
    fn legacy_settings() {
        let mut level = Level::new("Old");
        level.version = 1;
        level.layers.clear();
        level.settings = LevelSettingsData::Legacy {
            version: 8,
            body: LegacySettingsBody::Data {
                data: Box::new(LegacyLevelSettings {
                    modes: vec![(Some(SPRINT), Some(true))],
                    bronze_time: Some(TIMES[0]),
                    silver_time: Some(TIMES[1]),
                    gold_time: Some(TIMES[2]),
                    diamond_time: Some(TIMES[3]),
                    ..LegacyLevelSettings::default()
                }),
                end: BodyEnd::Complete,
            },
        };
        let file = BytesFile::Level(level).write();

        assert_eq!(workshop_medals(&file), (medals(TIME_MEDALS), None));
    }

    #[test]
    fn legacy_mode_names() {
        let s = LegacyLevelSettings {
            mode_names: vec![
                (Some("Sprint".into()), Some(false)),
                (Some("Stunt".into()), Some(true)),
            ],
            ..LegacyLevelSettings::default()
        };
        let settings = Settings::from_legacy(&s);
        assert!(!settings.mode_enabled(SPRINT));
        assert!(settings.mode_enabled(STUNT));
        assert!(!settings.mode_enabled(CHALLENGE));
    }

    #[test]
    fn non_level_files_are_errors() {
        assert!(extract(b"not a level").is_err());
        assert!(extract(&[]).is_err());
    }
}
