//! Fills the `level_file_data` table with data extracted from level files.
//!
//! Runs forever, processing one level at a time: it picks a level that needs processing, gets its
//! level file (downloading it from Steam for workshop levels, or reading it from
//! `OFFICIAL_LEVELS_DIR` for official levels), extracts data from it, and stores the result. A
//! level needs processing when it has no data yet, when its workshop file has changed, or when
//! the extractor version has been bumped. Workshop files aren't kept.
//!
//! Workshop files can be made by anyone, so extraction runs in a separate process with limited
//! memory and time: this program, run with the `--extract` argument.

#![warn(
    rust_2018_idioms,
    deprecated_in_future,
    macro_use_extern_crate,
    missing_debug_implementations,
    unused_qualifications
)]

mod extract;

use crate::extract::{EXTRACTOR_VERSION, LevelFileData, Medals, Modes};
use anyhow::{Context, Error, Result, anyhow, bail, ensure};
use futures::FutureExt;
use reqwest::Url;
use sha1::{Digest, Sha1};
use std::env;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::time;
use tokio_postgres::Statement;
use tracing::{info, warn};

/// Larger downloads are aborted. The largest workshop level is about 32 MB.
const MAX_FILE_SIZE: usize = 256 * 1024 * 1024;

/// How long to wait before trying a level again after getting or extracting its file failed.
const RETRY_INTERVAL: &str = "1 day";

/// Runs this program as an extraction process, which reads a level file from stdin and writes
/// the result to stdout. It's followed by the memory limit in bytes.
const EXTRACT_ARG: &str = "--extract";

/// Exit code of an extraction process that failed for a reason other than the level file, such
/// as not being able to limit its memory. That stops the processor rather than being stored as
/// the level's error.
const EXTRACT_PROCESS_ERROR: i32 = 3;

/// At most this much of an extraction process's error output is kept.
const MAX_EXTRACT_STDERR: usize = 2000;

#[derive(Debug)]
struct Config {
    /// Where official level files are, named `<level name>.bytes`. Official levels are skipped
    /// if this isn't set.
    official_levels_dir: Option<PathBuf>,
    /// Pause between processing levels.
    delay: Duration,
    /// Pause when no level needs processing.
    idle_delay: Duration,
    /// Address space limit of an extraction process, in bytes.
    extract_memory_limit: u64,
    /// How long an extraction process may run.
    extract_timeout: Duration,
}

impl Config {
    fn from_env() -> Result<Self> {
        Ok(Config {
            official_levels_dir: env::var_os("OFFICIAL_LEVELS_DIR").map(PathBuf::from),
            delay: Duration::from_secs(env_u64("LEVEL_PROCESSOR_DELAY_SECS", 5)?),
            idle_delay: Duration::from_secs(env_u64("LEVEL_PROCESSOR_IDLE_SECS", 600)?),
            extract_memory_limit: env_u64("LEVEL_PROCESSOR_EXTRACT_MEMORY_MB", 1024)?
                .checked_mul(1024 * 1024)
                .context("`LEVEL_PROCESSOR_EXTRACT_MEMORY_MB` is too large")?,
            extract_timeout: Duration::from_secs(env_u64(
                "LEVEL_PROCESSOR_EXTRACT_TIMEOUT_SECS",
                300,
            )?),
        })
    }
}

fn env_u64(name: &str, default: u64) -> Result<u64> {
    match env::var(name) {
        Ok(s) => s
            .parse()
            .with_context(|| format!("`{name}` must be a whole number")),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(e) => Err(Error::new(e).context(format!("invalid `{name}`"))),
    }
}

#[derive(Debug)]
struct Job {
    level_id: i64,
    name: String,
    source: Source,
}

#[derive(Debug)]
enum Source {
    Official {
        /// The modes the game offers the level in, from its own level lists rather than the
        /// level file. These are the level's leaderboards in the database.
        modes: Modes,
    },
    Workshop {
        file_url: Option<String>,
        hcontent_file: Option<String>,
        file_size: Option<i64>,
    },
}

impl Source {
    fn hcontent_file(&self) -> Option<&str> {
        match self {
            Source::Official { .. } => None,
            Source::Workshop { hcontent_file, .. } => hcontent_file.as_deref(),
        }
    }
}

/// What gets stored for a level whose file was extracted.
#[derive(Debug)]
struct Extracted {
    /// The modes the game offers the level in, which the medals are for.
    modes: Modes,
    time_medals: Option<Medals>,
    score_medals: Option<Medals>,
}

impl Extracted {
    fn new(source: &Source, data: &LevelFileData) -> Self {
        // The game offers a workshop level in the modes its file enables.
        let modes = match source {
            Source::Official { modes } => *modes,
            Source::Workshop { .. } => data.modes,
        };
        let (time_medals, score_medals) = data.medals_in(modes);

        Extracted {
            modes,
            time_medals,
            score_medals,
        }
    }
}

#[derive(Debug)]
struct FetchedFile {
    data: Vec<u8>,
    sha1: String,
}

#[derive(Debug)]
enum Outcome {
    Extracted {
        sha1: String,
        size: i64,
        data: Extracted,
    },
    ExtractFailed {
        sha1: String,
        size: i64,
        error: Error,
    },
    FetchFailed {
        error: Error,
    },
}

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    if args.next().as_deref() == Some(EXTRACT_ARG) {
        if let Err(e) = run_extraction(args.next()) {
            eprintln!("{e:#}");
            std::process::exit(EXTRACT_PROCESS_ERROR);
        }
        return Ok(());
    }

    run()
}

#[tokio::main(flavor = "current_thread")]
async fn run() -> Result<()> {
    color_backtrace::install();
    tracing_subscriber::fmt::init();
    dotenv::dotenv().ok();

    ensure!(
        cfg!(unix),
        "this only runs on Unix, where the memory used to parse level files can be limited"
    );

    let config = Config::from_env()?;
    if config.official_levels_dir.is_none() {
        warn!("`OFFICIAL_LEVELS_DIR` is not set, so official levels won't be processed");
    }

    let http = reqwest::Client::builder()
        .user_agent("distance-db-level-processor")
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(10 * 60))
        .build()?;

    let db = establish_connection().await?;
    let statements = Statements::prepare(&db)
        .await
        .context("couldn't prepare statements (has the `level_file_data` table been created?)")?;

    info!("Processing levels (extractor version {EXTRACTOR_VERSION})");
    let mut idle = false;
    loop {
        let Some(job) = statements.next_job(&db, &config).await? else {
            if !idle {
                info!("No levels need processing");
                idle = true;
            }
            time::sleep(config.idle_delay).await;
            continue;
        };
        idle = false;

        let outcome = process(&http, &config, &job).await?;
        statements.store(&db, &job, &outcome).await?;
        log_outcome(&job, &outcome);

        time::sleep(config.delay).await;
    }
}

async fn establish_connection() -> Result<tokio_postgres::Client> {
    let database_url =
        env::var("DATABASE_URL").context("Environment variable DATABASE_URL is not set")?;

    let (client, connection) =
        tokio_postgres::connect(&database_url, tokio_postgres::NoTls).await?;

    // If the connection fails, queries fail and the process exits.
    tokio::spawn(connection.map(|r| {
        if let Err(e) = r {
            warn!("database connection error: {e}");
        }
    }));

    Ok(client)
}

#[derive(Debug)]
struct Statements {
    next_job: Statement,
    store_extracted: Statement,
    store_fetch_failed: Statement,
}

impl Statements {
    async fn prepare(db: &tokio_postgres::Client) -> Result<Self> {
        // A level needs processing if it has a leaderboard and either has no row, or its row is
        // from an older extractor version, a different workshop file or (for official levels,
        // whose modes come from the database) different modes, or its retry time has come.
        // Levels waiting for a retry are skipped until then, unless the workshop file that failed
        // has since been replaced. New levels go first, and retries last.
        let next_job = db
            .prepare(
                "SELECT l.id, l.name, l.is_sprint, l.is_challenge, l.is_stunt,
                        w.level_id IS NOT NULL,
                        w.raw_details ->> 'file_url',
                        w.raw_details ->> 'hcontent_file',
                        (w.raw_details ->> 'file_size')::bigint
                   FROM levels l
                        LEFT JOIN workshop_level_details w ON w.level_id = l.id
                        LEFT JOIN level_file_data d ON d.level_id = l.id
                  WHERE (l.is_sprint OR l.is_challenge OR l.is_stunt)
                    AND (w.level_id IS NOT NULL OR $2)
                    AND (d.retry_at IS NULL
                         OR d.retry_at <= now()
                         OR (w.level_id IS NOT NULL
                             AND d.failed_hcontent_file IS DISTINCT FROM w.raw_details ->> 'hcontent_file'))
                    AND (d.level_id IS NULL
                         OR d.retry_at IS NOT NULL
                         OR coalesce(d.extractor_version, 0) < $1
                         OR (w.level_id IS NOT NULL
                             AND d.hcontent_file IS DISTINCT FROM w.raw_details ->> 'hcontent_file')
                         OR (w.level_id IS NULL
                             AND (d.has_sprint, d.has_challenge, d.has_stunt)
                                 IS DISTINCT FROM (l.is_sprint, l.is_challenge, l.is_stunt)))
                  ORDER BY d.level_id IS NOT NULL, d.retry_at IS NOT NULL, l.id
                  LIMIT 1",
            )
            .await?;

        // Used when the file was obtained, whether or not extraction succeeded. If it failed
        // ($6 is the error), the level is tried again later, or when its workshop file changes.
        let store_extracted = db
            .prepare(&format!(
                "INSERT INTO level_file_data
                     (level_id, hcontent_file, file_sha1, file_size, extractor_version,
                      extracted_at, error, retry_at, failed_hcontent_file,
                      has_sprint, has_challenge, has_stunt,
                      bronze_time, silver_time, gold_time, diamond_time,
                      bronze_score, silver_score, gold_score, diamond_score)
                 VALUES ($1, $2, $3, $4, $5, now(), $6,
                         CASE WHEN $6::character varying IS NOT NULL THEN now() + interval '{RETRY_INTERVAL}' END,
                         CASE WHEN $6::character varying IS NOT NULL THEN $2::character varying END,
                         $15, $16, $17,
                         $7, $8, $9, $10, $11, $12, $13, $14)
                 ON CONFLICT (level_id) DO UPDATE
                 SET hcontent_file = EXCLUDED.hcontent_file,
                     file_sha1 = EXCLUDED.file_sha1,
                     file_size = EXCLUDED.file_size,
                     extractor_version = EXCLUDED.extractor_version,
                     extracted_at = EXCLUDED.extracted_at,
                     error = EXCLUDED.error,
                     retry_at = EXCLUDED.retry_at,
                     failed_hcontent_file = EXCLUDED.failed_hcontent_file,
                     has_sprint = EXCLUDED.has_sprint,
                     has_challenge = EXCLUDED.has_challenge,
                     has_stunt = EXCLUDED.has_stunt,
                     bronze_time = EXCLUDED.bronze_time,
                     silver_time = EXCLUDED.silver_time,
                     gold_time = EXCLUDED.gold_time,
                     diamond_time = EXCLUDED.diamond_time,
                     bronze_score = EXCLUDED.bronze_score,
                     silver_score = EXCLUDED.silver_score,
                     gold_score = EXCLUDED.gold_score,
                     diamond_score = EXCLUDED.diamond_score",
            ))
            .await?;

        // Keeps any data from earlier, which is likely still better than nothing. Records which
        // workshop file failed, so that a new one is tried without waiting for the retry.
        let store_fetch_failed = db
            .prepare(&format!(
                "INSERT INTO level_file_data (level_id, error, retry_at, failed_hcontent_file)
                 VALUES ($1, $2, now() + interval '{RETRY_INTERVAL}', $3)
                 ON CONFLICT (level_id) DO UPDATE
                 SET error = EXCLUDED.error,
                     retry_at = EXCLUDED.retry_at,
                     failed_hcontent_file = EXCLUDED.failed_hcontent_file"
            ))
            .await?;

        Ok(Statements {
            next_job,
            store_extracted,
            store_fetch_failed,
        })
    }

    async fn next_job(&self, db: &tokio_postgres::Client, config: &Config) -> Result<Option<Job>> {
        let official_enabled = config.official_levels_dir.is_some();
        let row = db
            .query_opt(&self.next_job, &[&EXTRACTOR_VERSION, &official_enabled])
            .await?;

        Ok(row.map(|row| {
            let is_workshop: bool = row.get(5);
            Job {
                level_id: row.get(0),
                name: row.get(1),
                source: if is_workshop {
                    Source::Workshop {
                        file_url: row.get(6),
                        hcontent_file: row.get(7),
                        file_size: row.get(8),
                    }
                } else {
                    Source::Official {
                        modes: Modes {
                            sprint: row.get(2),
                            challenge: row.get(3),
                            stunt: row.get(4),
                        },
                    }
                },
            }
        }))
    }

    async fn store(&self, db: &tokio_postgres::Client, job: &Job, outcome: &Outcome) -> Result<()> {
        let hcontent_file = job.source.hcontent_file();
        let (sha1, size, data, error) = match outcome {
            Outcome::Extracted { sha1, size, data } => (sha1, size, Some(data), None),
            Outcome::ExtractFailed { sha1, size, error } => (sha1, size, None, Some(error)),
            Outcome::FetchFailed { error } => {
                db.execute(
                    &self.store_fetch_failed,
                    &[&job.level_id, &error_message(error), &hcontent_file],
                )
                .await?;
                return Ok(());
            }
        };

        let error = error.map(error_message);
        let modes = data.map(|d| d.modes);
        let [time_medals, score_medals] = [
            data.and_then(|d| d.time_medals),
            data.and_then(|d| d.score_medals),
        ]
        .map(|m| {
            [
                m.map(|m| m.bronze),
                m.map(|m| m.silver),
                m.map(|m| m.gold),
                m.map(|m| m.diamond),
            ]
        });
        db.execute(
            &self.store_extracted,
            &[
                &job.level_id,
                &hcontent_file,
                sha1,
                size,
                &EXTRACTOR_VERSION,
                &error,
                &time_medals[0],
                &time_medals[1],
                &time_medals[2],
                &time_medals[3],
                &score_medals[0],
                &score_medals[1],
                &score_medals[2],
                &score_medals[3],
                &modes.map(|m| m.sprint),
                &modes.map(|m| m.challenge),
                &modes.map(|m| m.stunt),
            ],
        )
        .await?;

        Ok(())
    }
}

/// Errors are problems with running extraction at all, rather than with the level.
async fn process(http: &reqwest::Client, config: &Config, job: &Job) -> Result<Outcome> {
    let file = match fetch(http, config, job).await {
        Ok(file) => file,
        Err(error) => return Ok(Outcome::FetchFailed { error }),
    };

    let FetchedFile { data, sha1 } = file;
    let size = data.len() as i64;

    Ok(match extract_in_child(config, data).await? {
        Ok(data) => Outcome::Extracted {
            sha1,
            size,
            data: Extracted::new(&job.source, &data),
        },
        Err(error) => Outcome::ExtractFailed { sha1, size, error },
    })
}

/// Extracts data from a level file in an extraction process. The outer error is a failure to run
/// the process; the inner one is a failure to extract data from the file, including the process
/// crashing (such as from running out of memory) or taking too long.
async fn extract_in_child(config: &Config, file: Vec<u8>) -> Result<Result<LevelFileData>> {
    let mut child = tokio::process::Command::new(
        env::current_exe().context("couldn't find this program's executable")?,
    )
    .arg(EXTRACT_ARG)
    .arg(config.extract_memory_limit.to_string())
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true)
    .spawn()
    .context("couldn't start an extraction process")?;

    let mut stdin = child.stdin.take().context("no stdin")?;
    let write_file = async move {
        // If this fails, it's because the process exited early, which is reported below.
        let _ = stdin.write_all(&file).await;
    };

    // When this times out, the process is killed as it's dropped.
    let Ok(((), output)) = time::timeout(
        config.extract_timeout,
        futures::future::join(write_file, child.wait_with_output()),
    )
    .await
    else {
        return Ok(Err(anyhow!(
            "extraction took longer than {} seconds",
            config.extract_timeout.as_secs()
        )));
    };
    let output = output.context("couldn't communicate with the extraction process")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        let stderr = match stderr.char_indices().nth(MAX_EXTRACT_STDERR) {
            Some((end, _)) => format!("{}…", &stderr[..end]),
            None => stderr.to_owned(),
        };
        if output.status.code() == Some(EXTRACT_PROCESS_ERROR) {
            bail!("the extraction process couldn't run: {stderr}");
        }
        return Ok(Err(anyhow!(
            "the extraction process failed ({}): {stderr}",
            output.status
        )));
    }

    let result: Result<LevelFileData, String> = serde_json::from_slice(&output.stdout)
        .context("couldn't read the output of the extraction process")?;

    Ok(result.map_err(Error::msg))
}

/// Runs as an extraction process: limits its own memory, reads a level file from stdin, and
/// writes the extracted data, or why it couldn't be extracted, to stdout as JSON. An error is a
/// problem with the process rather than the file.
fn run_extraction(memory_limit: Option<String>) -> Result<()> {
    let memory_limit = memory_limit
        .and_then(|s| s.parse().ok())
        .context("expected the memory limit")?;
    limit_resources(memory_limit)?;

    let mut file = Vec::new();
    std::io::stdin().lock().read_to_end(&mut file)?;

    let result = extract::extract(&file).map_err(|e| format!("{e:#}"));

    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer(&mut stdout, &result)?;
    stdout.flush()?;

    Ok(())
}

#[cfg(unix)]
fn limit_resources(memory_limit: u64) -> Result<()> {
    use rlimit::Resource;

    // The process may have inherited a lower limit, which it can't raise.
    let (_, hard) = Resource::AS
        .get()
        .context("couldn't get the memory limit")?;
    let memory_limit = memory_limit.min(hard);
    Resource::AS
        .set(memory_limit, memory_limit)
        .context("couldn't limit memory")?;

    // Running out of memory aborts the process, which shouldn't leave a core dump of up to the
    // memory limit behind.
    Resource::CORE
        .set(0, 0)
        .context("couldn't disable core dumps")?;

    Ok(())
}

#[cfg(not(unix))]
fn limit_resources(_memory_limit: u64) -> Result<()> {
    bail!("memory can only be limited on Unix")
}

async fn fetch(http: &reqwest::Client, config: &Config, job: &Job) -> Result<FetchedFile> {
    match &job.source {
        Source::Official { .. } => {
            let dir = config
                .official_levels_dir
                .as_ref()
                .context("`OFFICIAL_LEVELS_DIR` is not set")?;
            ensure!(
                !job.name.is_empty()
                    && !job.name.starts_with('.')
                    && !job.name.contains(['/', '\\']),
                "unexpected official level name"
            );

            let path = dir.join(format!("{}.bytes", job.name));
            let data = tokio::fs::read(&path)
                .await
                .with_context(|| format!("couldn't read `{}`", path.display()))?;
            let sha1 = sha1_hex(&data);

            Ok(FetchedFile { data, sha1 })
        }
        Source::Workshop {
            file_url,
            hcontent_file,
            file_size,
        } => {
            // The handle is how a new file is noticed, so a level without one would never be
            // updated.
            ensure!(
                hcontent_file.as_deref().is_some_and(|h| !h.is_empty()),
                "the workshop details have no content handle (`hcontent_file`)"
            );
            let url = file_url
                .as_deref()
                .filter(|url| !url.is_empty())
                .context("the workshop details have no file URL")?;
            let url = Url::parse(url).context("invalid file URL")?;
            let expected_sha1 = sha1_from_file_url(&url)
                .context("the file URL doesn't end with the file's SHA-1, to check the file")?;

            let data = download(http, url).await?;

            if let Some(expected) = *file_size
                && expected > 0
            {
                ensure!(
                    data.len() as i64 == expected,
                    "downloaded {} bytes, but the workshop details say the file has {expected}",
                    data.len()
                );
            }

            let sha1 = sha1_hex(&data);
            ensure!(
                sha1 == expected_sha1,
                "the downloaded file's SHA-1 is {sha1}, but the file URL says {expected_sha1}"
            );

            Ok(FetchedFile { data, sha1 })
        }
    }
}

async fn download(http: &reqwest::Client, url: Url) -> Result<Vec<u8>> {
    let mut response = http.get(url).send().await?.error_for_status()?;

    let mut data = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        data.extend_from_slice(&chunk);
        if data.len() > MAX_FILE_SIZE {
            bail!("the file is larger than {MAX_FILE_SIZE} bytes");
        }
    }

    Ok(data)
}

/// The error with its causes, for storing. Postgres text can't contain NUL characters, which an
/// error could have from the contents of a file.
fn error_message(error: &Error) -> String {
    format!("{error:#}").replace('\0', "\u{FFFD}")
}

fn sha1_hex(data: &[u8]) -> String {
    Sha1::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Workshop file URLs end with the file's SHA-1, as in
/// `https://cdn.steamusercontent.com/ugc/<handle>/<SHA-1>/`.
fn sha1_from_file_url(url: &Url) -> Option<String> {
    let last = url.path_segments()?.rev().find(|s| !s.is_empty())?;
    (last.len() == 40 && last.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| last.to_ascii_lowercase())
}

fn log_outcome(job: &Job, outcome: &Outcome) {
    let Job { level_id, name, .. } = job;
    match outcome {
        Outcome::Extracted { data, .. } => {
            let fmt = |m: Option<Medals>| {
                m.map_or("none".to_owned(), |m| {
                    format!("{}/{}/{}/{}", m.bronze, m.silver, m.gold, m.diamond)
                })
            };
            info!(
                "{level_id} {name:?}: time medals {}, score medals {}",
                fmt(data.time_medals),
                fmt(data.score_medals)
            );
        }
        Outcome::ExtractFailed { error, .. } => {
            warn!("{level_id} {name:?}: couldn't extract data: {error:#}");
        }
        Outcome::FetchFailed { error } => {
            warn!("{level_id} {name:?}: couldn't get the level file: {error:#}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha1_from_url(url: &str) -> Option<String> {
        sha1_from_file_url(&Url::parse(url).unwrap())
    }

    #[test]
    fn sha1_from_url_path() {
        let sha1 = Some("15bd74243dbedd7a1b22b65012b76ff9410f4294");
        assert_eq!(
            sha1_from_url(
                "https://cdn.steamusercontent.com/ugc/14602247817176750981/15BD74243DBEDD7A1B22B65012B76FF9410F4294/"
            )
            .as_deref(),
            sha1
        );
        assert_eq!(
            sha1_from_url("https://example.com/ugc/1/15BD74243DBEDD7A1B22B65012B76FF9410F4294")
                .as_deref(),
            sha1
        );
        assert_eq!(
            sha1_from_url(
                "https://example.com/ugc/1/15BD74243DBEDD7A1B22B65012B76FF9410F4294/?a=b#c"
            )
            .as_deref(),
            sha1
        );
        assert_eq!(
            sha1_from_url("https://example.com/ugc/123/not-a-hash/"),
            None
        );
        assert_eq!(sha1_from_url("https://example.com/"), None);
    }

    #[test]
    fn sha1_hex_format() {
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }
}
