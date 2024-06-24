CREATE TABLE
    levels (
        id bigint PRIMARY KEY,
        name character varying NOT NULL,
        is_sprint boolean NOT NULL,
        is_challenge boolean NOT NULL,
        is_stunt boolean NOT NULL,
        CHECK (
            is_sprint
            OR is_challenge
            OR is_stunt
        )
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
        tags character varying NOT NULL,
        author_steam_id bigint GENERATED ALWAYS AS ((raw_details ->> 'creator')::bigint) STORED REFERENCES users,
        description character varying GENERATED ALWAYS AS (raw_details ->> 'file_description') STORED NOT NULL,
        time_created timestamp with time zone GENERATED ALWAYS AS (
            to_timestamp((raw_details ->> 'time_created')::bigint)
        ) STORED NOT NULL,
        time_updated timestamp with time zone GENERATED ALWAYS AS (
            to_timestamp((raw_details ->> 'time_updated')::bigint)
        ) STORED NOT NULL,
        visibility character varying GENERATED ALWAYS AS (
            (ARRAY['public', 'friends_only', 'private']) [(raw_details ->> 'visibility')::integer + 1]
        ) STORED NOT NULL,
        preview_url character varying GENERATED ALWAYS AS (raw_details ->> 'preview_url') STORED NOT NULL,
        file_name character varying GENERATED ALWAYS AS (raw_details ->> 'filename') STORED NOT NULL,
        file_size integer GENERATED ALWAYS AS ((raw_details ->> 'file_size')::integer) STORED NOT NULL,
        votes_up integer GENERATED ALWAYS AS (
            (raw_details -> 'vote_data' ->> 'votes_up')::integer
        ) STORED NOT NULL CHECK (votes_up >= 0),
        votes_down integer GENERATED ALWAYS AS (
            (raw_details -> 'vote_data' ->> 'votes_down')::integer
        ) STORED NOT NULL CHECK (votes_down >= 0),
        score real GENERATED ALWAYS AS ((raw_details -> 'vote_data' ->> 'score')::real) STORED NOT NULL CHECK (
            score >= 0.0
            AND score <= 1.0
        )
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