--
-- PostgreSQL database dump
--

-- Dumped from database version 12.0
-- Dumped by pg_dump version 12.0

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Name: published_file_visibility; Type: TYPE; Schema: public; Owner: postgres
--

CREATE TYPE public.published_file_visibility AS ENUM (
    'public',
    'friends_only',
    'private'
);


ALTER TYPE public.published_file_visibility OWNER TO postgres;

SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: challenge_leaderboard_entries; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.challenge_leaderboard_entries (
    level_id integer NOT NULL,
    steam_id bigint NOT NULL,
    "time" integer NOT NULL
);


ALTER TABLE public.challenge_leaderboard_entries OWNER TO postgres;

--
-- Name: levels; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.levels (
    id integer NOT NULL,
    name character varying NOT NULL,
    is_sprint boolean NOT NULL,
    is_challenge boolean NOT NULL,
    is_stunt boolean NOT NULL,
    CONSTRAINT levels_check CHECK ((is_sprint OR is_challenge OR is_stunt))
);


ALTER TABLE public.levels OWNER TO postgres;

--
-- Name: levels_id_seq; Type: SEQUENCE; Schema: public; Owner: postgres
--

CREATE SEQUENCE public.levels_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER TABLE public.levels_id_seq OWNER TO postgres;

--
-- Name: levels_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: postgres
--

ALTER SEQUENCE public.levels_id_seq OWNED BY public.levels.id;


--
-- Name: metadata; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.metadata (
    onerow_id boolean DEFAULT true NOT NULL,
    last_updated timestamp with time zone,
    CONSTRAINT onerow CHECK (onerow_id)
);


ALTER TABLE public.metadata OWNER TO postgres;

--
-- Name: sprint_leaderboard_entries; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.sprint_leaderboard_entries (
    level_id integer NOT NULL,
    steam_id bigint NOT NULL,
    "time" integer NOT NULL
);


ALTER TABLE public.sprint_leaderboard_entries OWNER TO postgres;

--
-- Name: stunt_leaderboard_entries; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.stunt_leaderboard_entries (
    level_id integer NOT NULL,
    steam_id bigint NOT NULL,
    score integer NOT NULL
);


ALTER TABLE public.stunt_leaderboard_entries OWNER TO postgres;

--
-- Name: users; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.users (
    steam_id bigint NOT NULL,
    name character varying NOT NULL,
    CONSTRAINT users_steam_id_check CHECK ((steam_id <> 0))
);


ALTER TABLE public.users OWNER TO postgres;

--
-- Name: workshop_level_details; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.workshop_level_details (
    level_id integer NOT NULL,
    author_steam_id bigint NOT NULL,
    description character varying NOT NULL,
    time_created timestamp with time zone NOT NULL,
    time_updated timestamp with time zone NOT NULL,
    visibility character varying NOT NULL,
    tags character varying NOT NULL,
    preview_url character varying NOT NULL,
    file_name character varying NOT NULL,
    file_size integer NOT NULL,
    votes_up integer NOT NULL,
    votes_down integer NOT NULL,
    score real NOT NULL,
    CONSTRAINT workshop_level_details_file_name_check CHECK ((length((file_name)::text) > 0)),
    CONSTRAINT workshop_level_details_file_size_check CHECK ((file_size > 0)),
    CONSTRAINT workshop_level_details_preview_url_check CHECK ((length((preview_url)::text) > 0)),
    CONSTRAINT workshop_level_details_score_check CHECK (((score >= (0)::double precision) AND (score <= (1)::double precision))),
    CONSTRAINT workshop_level_details_visibility_check CHECK (((visibility)::text = ANY (ARRAY[('public'::character varying)::text, ('friends_only'::character varying)::text, ('private'::character varying)::text]))),
    CONSTRAINT workshop_level_details_votes_down_check CHECK ((votes_down >= 0)),
    CONSTRAINT workshop_level_details_votes_up_check CHECK ((votes_up >= 0))
);


ALTER TABLE public.workshop_level_details OWNER TO postgres;

--
-- Name: levels id; Type: DEFAULT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.levels ALTER COLUMN id SET DEFAULT nextval('public.levels_id_seq'::regclass);


--
-- Name: challenge_leaderboard_entries challenge_leaderboard_entries_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.challenge_leaderboard_entries
    ADD CONSTRAINT challenge_leaderboard_entries_pkey PRIMARY KEY (level_id, steam_id);


--
-- Name: levels levels_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.levels
    ADD CONSTRAINT levels_pkey PRIMARY KEY (id);


--
-- Name: metadata metadata_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.metadata
    ADD CONSTRAINT metadata_pkey PRIMARY KEY (onerow_id);


--
-- Name: users players_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT players_pkey PRIMARY KEY (steam_id);


--
-- Name: sprint_leaderboard_entries sprint_leaderboard_entries_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.sprint_leaderboard_entries
    ADD CONSTRAINT sprint_leaderboard_entries_pkey PRIMARY KEY (steam_id, level_id);


--
-- Name: stunt_leaderboard_entries stunt_leaderboard_entries_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.stunt_leaderboard_entries
    ADD CONSTRAINT stunt_leaderboard_entries_pkey PRIMARY KEY (level_id, steam_id);


--
-- Name: workshop_level_details workshop_level_details_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.workshop_level_details
    ADD CONSTRAINT workshop_level_details_pkey PRIMARY KEY (level_id);


--
-- Name: challenge_leaderboard_entries_level_id_idx; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX challenge_leaderboard_entries_level_id_idx ON public.challenge_leaderboard_entries USING btree (level_id);


--
-- Name: sprint_leaderboard_entries_level_id_idx; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX sprint_leaderboard_entries_level_id_idx ON public.sprint_leaderboard_entries USING btree (level_id);


--
-- Name: stunt_leaderboard_entries_level_id_idx; Type: INDEX; Schema: public; Owner: postgres
--

CREATE INDEX stunt_leaderboard_entries_level_id_idx ON public.stunt_leaderboard_entries USING btree (level_id);


--
-- Name: challenge_leaderboard_entries challenge_leaderboard_entries_level_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.challenge_leaderboard_entries
    ADD CONSTRAINT challenge_leaderboard_entries_level_id_fkey FOREIGN KEY (level_id) REFERENCES public.levels(id);


--
-- Name: challenge_leaderboard_entries challenge_leaderboard_entries_steam_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.challenge_leaderboard_entries
    ADD CONSTRAINT challenge_leaderboard_entries_steam_id_fkey FOREIGN KEY (steam_id) REFERENCES public.users(steam_id);


--
-- Name: sprint_leaderboard_entries sprint_leaderboard_entries_level_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.sprint_leaderboard_entries
    ADD CONSTRAINT sprint_leaderboard_entries_level_id_fkey FOREIGN KEY (level_id) REFERENCES public.levels(id);


--
-- Name: sprint_leaderboard_entries sprint_leaderboard_entries_steam_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.sprint_leaderboard_entries
    ADD CONSTRAINT sprint_leaderboard_entries_steam_id_fkey FOREIGN KEY (steam_id) REFERENCES public.users(steam_id);


--
-- Name: stunt_leaderboard_entries stunt_leaderboard_entries_level_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.stunt_leaderboard_entries
    ADD CONSTRAINT stunt_leaderboard_entries_level_id_fkey FOREIGN KEY (level_id) REFERENCES public.levels(id);


--
-- Name: stunt_leaderboard_entries stunt_leaderboard_entries_steam_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.stunt_leaderboard_entries
    ADD CONSTRAINT stunt_leaderboard_entries_steam_id_fkey FOREIGN KEY (steam_id) REFERENCES public.users(steam_id);


--
-- Name: workshop_level_details workshop_level_details_author_steam_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.workshop_level_details
    ADD CONSTRAINT workshop_level_details_author_steam_id_fkey FOREIGN KEY (author_steam_id) REFERENCES public.users(steam_id);


--
-- Name: workshop_level_details workshop_level_details_level_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.workshop_level_details
    ADD CONSTRAINT workshop_level_details_level_id_fkey FOREIGN KEY (level_id) REFERENCES public.levels(id);


--
-- Name: TABLE challenge_leaderboard_entries; Type: ACL; Schema: public; Owner: postgres
--

GRANT SELECT ON TABLE public.challenge_leaderboard_entries TO reader;


--
-- Name: TABLE levels; Type: ACL; Schema: public; Owner: postgres
--

GRANT SELECT ON TABLE public.levels TO reader;


--
-- Name: TABLE metadata; Type: ACL; Schema: public; Owner: postgres
--

GRANT SELECT ON TABLE public.metadata TO reader;


--
-- Name: TABLE sprint_leaderboard_entries; Type: ACL; Schema: public; Owner: postgres
--

GRANT SELECT ON TABLE public.sprint_leaderboard_entries TO reader;


--
-- Name: TABLE stunt_leaderboard_entries; Type: ACL; Schema: public; Owner: postgres
--

GRANT SELECT ON TABLE public.stunt_leaderboard_entries TO reader;


--
-- Name: TABLE users; Type: ACL; Schema: public; Owner: postgres
--

GRANT SELECT ON TABLE public.users TO reader;


--
-- Name: TABLE workshop_level_details; Type: ACL; Schema: public; Owner: postgres
--

GRANT SELECT ON TABLE public.workshop_level_details TO reader;


--
-- PostgreSQL database dump complete
--

