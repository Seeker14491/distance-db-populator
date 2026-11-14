-- Data extracted from level files by `distance-db-level-processor`.
BEGIN;

CREATE TABLE
    level_file_data (
        level_id bigint PRIMARY KEY REFERENCES levels,
        hcontent_file character varying,
        file_sha1 character varying,
        file_size bigint,
        extractor_version integer,
        extracted_at timestamp with time zone,
        error character varying,
        retry_at timestamp with time zone,
        failed_hcontent_file character varying,
        has_sprint boolean,
        has_challenge boolean,
        has_stunt boolean,
        bronze_time integer,
        silver_time integer,
        gold_time integer,
        diamond_time integer,
        bronze_score integer,
        silver_score integer,
        gold_score integer,
        diamond_score integer
    );

COMMENT ON TABLE level_file_data IS 'Data extracted from the level file of each level with a leaderboard.';

COMMENT ON COLUMN level_file_data.hcontent_file IS 'For workshop levels, the Steam content handle of the file the data is from (`hcontent_file` in `workshop_level_details.raw_details`); it changes when the level is updated.';

COMMENT ON COLUMN level_file_data.file_sha1 IS 'SHA-1 of the file the data is from, in hex.';

COMMENT ON COLUMN level_file_data.file_size IS 'Size of the file the data is from, in bytes.';

COMMENT ON COLUMN level_file_data.extractor_version IS 'Version of the extraction code that produced the data.';

COMMENT ON COLUMN level_file_data.extracted_at IS 'When the data was extracted.';

COMMENT ON COLUMN level_file_data.error IS 'Why the file could not be obtained, or the data could not be extracted, the last time it was tried. NULL if the last attempt succeeded.';

COMMENT ON COLUMN level_file_data.retry_at IS 'When the level will be tried again, after getting its file or extracting the data failed.';

COMMENT ON COLUMN level_file_data.failed_hcontent_file IS 'For workshop levels, the content handle of the file that could not be downloaded or parsed. If the level is updated to a different file, it is tried without waiting for retry_at.';

COMMENT ON COLUMN level_file_data.has_sprint IS 'Whether the game offers the level in Sprint mode, as set in the level file for workshop levels, or in the game''s level lists (levels.is_sprint) for official levels. Time medals are for Sprint and Challenge. NULL if no data was extracted.';

COMMENT ON COLUMN level_file_data.has_challenge IS 'Whether the game offers the level in Challenge mode, as set in the level file for workshop levels, or in the game''s level lists (levels.is_challenge) for official levels. Time medals are for Sprint and Challenge. NULL if no data was extracted.';

COMMENT ON COLUMN level_file_data.has_stunt IS 'Whether the game offers the level in Stunt mode, as set in the level file for workshop levels, or in the game''s level lists (levels.is_stunt) for official levels. Score medals are for Stunt. NULL if no data was extracted.';

COMMENT ON COLUMN level_file_data.bronze_time IS 'Bronze medal time in milliseconds (rounded down), if the level has Sprint or Challenge mode and medal times. A run gets the medal if its time is at most this.';

COMMENT ON COLUMN level_file_data.silver_time IS 'Silver medal time in milliseconds (rounded down), if the level has Sprint or Challenge mode and medal times. A run gets the medal if its time is at most this.';

COMMENT ON COLUMN level_file_data.gold_time IS 'Gold medal time in milliseconds (rounded down), if the level has Sprint or Challenge mode and medal times. A run gets the medal if its time is at most this.';

COMMENT ON COLUMN level_file_data.diamond_time IS 'Diamond medal time in milliseconds (rounded down), if the level has Sprint or Challenge mode and medal times. A run gets the medal if its time is at most this.';

COMMENT ON COLUMN level_file_data.bronze_score IS 'Bronze medal score, if the level has Stunt mode and medal scores. A run gets the medal if its score is at least this.';

COMMENT ON COLUMN level_file_data.silver_score IS 'Silver medal score, if the level has Stunt mode and medal scores. A run gets the medal if its score is at least this.';

COMMENT ON COLUMN level_file_data.gold_score IS 'Gold medal score, if the level has Stunt mode and medal scores. A run gets the medal if its score is at least this.';

COMMENT ON COLUMN level_file_data.diamond_score IS 'Diamond medal score, if the level has Stunt mode and medal scores. A run gets the medal if its score is at least this.';

GRANT
SELECT
    ON level_file_data TO reader;

COMMIT;
