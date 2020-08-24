CREATE TABLE levels
(
    id           serial PRIMARY KEY,
    name         character varying NOT NULL,
    is_sprint    boolean           NOT NULL,
    is_challenge boolean           NOT NULL,
    is_stunt     boolean           NOT NULL,
    CHECK (is_sprint OR is_challenge OR is_stunt)
);

CREATE TABLE users
(
    steam_id bigint            PRIMARY KEY CHECK (steam_id <> 0),
    name     character varying NOT NULL
);

CREATE TABLE workshop_level_details
(
    level_id        integer PRIMARY KEY REFERENCES levels,
    author_steam_id bigint REFERENCES users,
    description     character varying        NOT NULL,
    time_created    timestamp with time zone NOT NULL,
    time_updated    timestamp with time zone NOT NULL,
    visibility      character varying        NOT NULL CHECK ( visibility IN ('public', 'friends_only', 'private') ),
    tags            character varying        NOT NULL,
    preview_url     character varying        NOT NULL,
    file_name       character varying        NOT NULL,
    file_size       integer                  NOT NULL,
    votes_up        integer                  NOT NULL CHECK ( votes_up >= 0 ),
    votes_down      integer                  NOT NULL CHECK ( votes_down >= 0 ),
    score           real                     NOT NULL CHECK ( score >= 0.0 AND score <= 1.0 )
);

CREATE TABLE sprint_leaderboard_entries
(
    level_id integer REFERENCES levels,
    steam_id bigint REFERENCES users,
    time     integer NOT NULL,
    rank     integer NOT NULL CHECK (rank > 0),
    has_replay boolean NOT NULL,
    PRIMARY KEY (level_id, steam_id)
);

CREATE TABLE challenge_leaderboard_entries
(
    level_id integer REFERENCES levels,
    steam_id bigint REFERENCES users,
    time     integer NOT NULL,
    rank     integer NOT NULL CHECK ( rank > 0 ),
    has_replay boolean NOT NULL,
    PRIMARY KEY (level_id, steam_id)
);

CREATE TABLE stunt_leaderboard_entries
(
    level_id integer REFERENCES levels,
    steam_id bigint REFERENCES users,
    score    integer NOT NULL,
    rank     integer NOT NULL CHECK (rank > 0),
    has_replay boolean NOT NULL,
    PRIMARY KEY (level_id, steam_id)
);

CREATE TABLE metadata
(
    onerow_id    boolean DEFAULT true PRIMARY KEY,
    last_updated timestamp with time zone,
    CHECK (onerow_id)
);

CREATE INDEX ON sprint_leaderboard_entries (level_id, rank);
CREATE INDEX ON sprint_leaderboard_entries USING HASH (steam_id);
CREATE INDEX ON challenge_leaderboard_entries (level_id, rank);
CREATE INDEX ON challenge_leaderboard_entries USING HASH (steam_id);
CREATE INDEX ON stunt_leaderboard_entries (level_id, rank);
CREATE INDEX ON stunt_leaderboard_entries USING HASH (steam_id);

REVOKE CREATE ON SCHEMA public FROM PUBLIC;

GRANT SELECT ON ALL TABLES IN SCHEMA public TO reader;
ALTER ROLE reader SET statement_timeout TO '5000';
