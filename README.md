# distance-db-populator

Populate the Distance Database with data from Steam.

This software is designed to be run with Docker. The following environment variables should be set:

- `DATABASE_URL`: URL of the Postgres DB that will be updated
- `STEAM_WEB_API_KEY`: Steam Web API key; you can get one [here](https://steamcommunity.com/dev/apikey).
- `GRPC_SERVER_ADDRESS`: Address of a [DistanceSteamDataServer](https://github.com/Seeker14491/DistanceSteamDataServer)
- `MIN_MINUTES_BETWEEN_UPDATES`: Wait at least this many minutes between running the populator

Optionally, the variable `HEALTHCHECKS_URL` can be set to a [healthchecks.io](https://healthchecks.io/) ping url.

## Level processor

Alongside the populator, the container runs `distance-db-level-processor`, which fills the `level_file_data` table with data extracted from each level's `.bytes` file using [distance-bytes](https://github.com/Seeker14491/distance-bytes). Currently that data is the medal times and scores. It runs independently of the populator, processing one level at a time. A level is processed when it has no data yet, when its workshop file has changed, when an official level's modes have changed, or when the extractor version has been bumped. Only levels with a Sprint, Challenge or Stunt leaderboard are processed.

- Workshop level files are downloaded from Steam using the `file_url` in their workshop details, checked against the size and SHA-1 Steam reports, and not kept.
- Official levels can't be downloaded, so their files are read from `OFFICIAL_LEVELS_DIR`, named `<level name>.bytes` (as in the game's `Resources/levels`, for example `Broken Symmetry.bytes`). `scripts/extract_official_levels.py` extracts them from a game installation.
- Workshop files are checked against the size and SHA-1 in the workshop details before use.
- Extraction runs in a separate process (the same program, run with `--extract`) with limited memory and time, since workshop files can be made by anyone.
- If a file can't be downloaded, read or parsed, the error is stored in the level's `error` column and it's tried again a day later (`retry_at`), or as soon as the workshop level is updated to a new file. Data from an earlier file is kept if the new one can't be downloaded, but not if it can't be parsed.
- When the extraction code changes what it produces, bump `EXTRACTOR_VERSION` in `level-processor/src/extract.rs`, and every level is processed again. Data from the previous version stays in place until then.

Environment variables (besides `DATABASE_URL`):

- `OFFICIAL_LEVELS_DIR`: directory with the official level files. If it isn't set, official levels are skipped.
- `LEVEL_PROCESSOR_DELAY_SECS`: pause between levels (default 5).
- `LEVEL_PROCESSOR_IDLE_SECS`: pause when no level needs processing (default 600).
- `LEVEL_PROCESSOR_EXTRACT_MEMORY_MB`: address space limit of an extraction process (default 1024; the largest workshop level, 32 MB, needs under 192).
- `LEVEL_PROCESSOR_EXTRACT_TIMEOUT_SECS`: how long an extraction process may run (default 300).
- `LEVEL_PROCESSOR_ENABLED`: set to `false` to not run the processor.

If the processor exits, the manager starts it again after 10 minutes.

## Misc.

Schema changes for an existing database are in `migrations/`; `create_db.sql` has the full schema.

Dumping the database:

```
pg_dump -s -d distance -f create_db.sql
```
