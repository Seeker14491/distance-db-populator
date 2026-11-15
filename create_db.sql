CREATE TABLE
    levels (
        id bigint PRIMARY KEY,
        name character varying NOT NULL,
        is_sprint boolean NOT NULL,
        is_challenge boolean NOT NULL,
        is_stunt boolean NOT NULL,
        sprint_leaderboard_hash bigint,
        challenge_leaderboard_hash bigint,
        stunt_leaderboard_hash bigint
    );

CREATE TABLE
    users (
        steam_id bigint PRIMARY KEY CHECK (steam_id <> 0),
        name character varying NOT NULL
    );

CREATE TABLE
    workshop_level_details (
        level_id bigint PRIMARY KEY REFERENCES levels CHECK (
            level_id = (raw_details ->> 'publishedfileid')::bigint
        ),
        raw_details jsonb NOT NULL,
        tags character varying ARRAY NOT NULL,
        author_steam_id bigint GENERATED ALWAYS AS ((raw_details ->> 'creator')::bigint) STORED REFERENCES users,
        time_created timestamp with time zone GENERATED ALWAYS AS (
            to_timestamp((raw_details ->> 'time_created')::bigint)
        ) STORED NOT NULL,
        time_updated timestamp with time zone GENERATED ALWAYS AS (
            to_timestamp((raw_details ->> 'time_updated')::bigint)
        ) STORED NOT NULL
    );

CREATE TABLE
    sprint_leaderboard_entries (
        level_id bigint REFERENCES levels,
        steam_id bigint REFERENCES users,
        time integer NOT NULL,
        rank integer NOT NULL CHECK (rank > 0),
        has_replay boolean NOT NULL,
        PRIMARY KEY (level_id, steam_id)
    );

CREATE TABLE
    challenge_leaderboard_entries (
        level_id bigint REFERENCES levels,
        steam_id bigint REFERENCES users,
        time integer NOT NULL,
        rank integer NOT NULL CHECK (rank > 0),
        has_replay boolean NOT NULL,
        PRIMARY KEY (level_id, steam_id)
    );

CREATE TABLE
    stunt_leaderboard_entries (
        level_id bigint REFERENCES levels,
        steam_id bigint REFERENCES users,
        score integer NOT NULL,
        rank integer NOT NULL CHECK (rank > 0),
        has_replay boolean NOT NULL,
        PRIMARY KEY (level_id, steam_id)
    );

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
        diamond_score integer,
        CHECK (num_nulls(bronze_time, silver_time, gold_time, diamond_time) IN (0, 4)),
        CHECK (num_nulls(bronze_score, silver_score, gold_score, diamond_score) IN (0, 4))
    );

CREATE TABLE
    metadata (
        onerow_id boolean DEFAULT true PRIMARY KEY,
        last_updated timestamp with time zone,
        CHECK (onerow_id)
    );

CREATE VIEW
    official_levels AS
SELECT
    *
FROM
    levels
WHERE
    id NOT IN (
        SELECT
            level_id
        FROM
            workshop_level_details
    );

CREATE VIEW
    workshop_levels AS
SELECT
    *
FROM
    levels
WHERE
    id IN (
        SELECT
            level_id
        FROM
            workshop_level_details
    );

CREATE INDEX ON users (name);

CREATE INDEX ON sprint_leaderboard_entries (level_id, rank);

CREATE INDEX ON sprint_leaderboard_entries USING HASH (steam_id);

CREATE INDEX ON challenge_leaderboard_entries (level_id, rank);

CREATE INDEX ON challenge_leaderboard_entries USING HASH (steam_id);

CREATE INDEX ON stunt_leaderboard_entries (level_id, rank);

CREATE INDEX ON stunt_leaderboard_entries USING HASH (steam_id);

CREATE FUNCTION get_official_level_by_name (level_name text) RETURNS SETOF levels AS $$
    SELECT *
    FROM levels
    WHERE name = level_name AND id NOT IN (SELECT level_id FROM workshop_level_details)
$$ LANGUAGE SQL STABLE;

REVOKE CREATE ON SCHEMA public
FROM
    PUBLIC;

GRANT
SELECT
    ON ALL TABLES IN SCHEMA public TO reader;

ALTER ROLE reader
SET
    statement_timeout TO '10000';