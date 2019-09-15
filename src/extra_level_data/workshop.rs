use std::process::{Command, Stdio};
use std::path::Path;
use failure::{Error, bail, ResultExt};
use serde::Deserialize;
use super::{MusicCueId, LevelDifficulty};

#[derive(Debug, Deserialize)]
pub struct WorkshopLevel {
    pub medal_times: Option<Vec<f32>>,
    pub medal_scores: Option<Vec<i32>>,
    pub difficulty: Option<LevelDifficulty>,
    pub abilities: Option<Vec<u8>>,
    pub music_id: Option<MusicCueId>,
}

fn read_data_from_bytes_file<P: AsRef<Path>>(path: P) -> Result<WorkshopLevel, Error> {
    let output = Command::new("bytes-dumper/dist/bytes-dumper.exe") // FIXME: proper path
        .arg(dunce::canonicalize(path.as_ref()).context("Couldn't find input '.bytes' file")?)
        .stdin(Stdio::null())
        .output()?;

    if !output.status.success() {
        bail!("bytes-dumper returned non-zero exit code");
    }

    let data: WorkshopLevel = serde_json::from_slice(&output.stdout)?;

    Ok(data)
}

#[test]
fn test_read_data_from_normal_bytes_file() {
    read_data_from_bytes_file("test_samples/Hardline.bytes").unwrap();
}

#[test]
fn test_read_data_from_empty_bytes_file() {
    read_data_from_bytes_file("test_samples/empty.bytes").unwrap();
}
