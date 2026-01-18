--
-- PostgreSQL database dump
--

-- Dumped from database version 14.18 (Ubuntu 14.18-0ubuntu0.22.04.1)
-- Dumped by pg_dump version 14.18 (Ubuntu 14.18-0ubuntu0.22.04.1)

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
-- Name: async_kind; Type: TYPE; Schema: public; Owner: mido
--

CREATE TYPE public.async_kind AS ENUM (
    'qualifier',
    'qualifier2',
    'qualifier3',
    'tiebreaker1',
    'tiebreaker2'
);


ALTER TYPE public.async_kind OWNER TO mido;

--
-- Name: hash_icon; Type: TYPE; Schema: public; Owner: mido
--

CREATE TYPE public.hash_icon AS ENUM (
    'Deku Stick',
    'Deku Nut',
    'Bow',
    'Slingshot',
    'Fairy Ocarina',
    'Bombchu',
    'Longshot',
    'Boomerang',
    'Lens of Truth',
    'Beans',
    'Megaton Hammer',
    'Bottled Fish',
    'Bottled Milk',
    'Mask of Truth',
    'SOLD OUT',
    'Cucco',
    'Mushroom',
    'Saw',
    'Frog',
    'Master Sword',
    'Mirror Shield',
    'Kokiri Tunic',
    'Hover Boots',
    'Silver Gauntlets',
    'Gold Scale',
    'Stone of Agony',
    'Skull Token',
    'Heart Container',
    'Boss Key',
    'Compass',
    'Map',
    'Big Magic'
);


ALTER TYPE public.hash_icon OWNER TO mido;

--
-- Name: language; Type: TYPE; Schema: public; Owner: mido
--

CREATE TYPE public.language AS ENUM (
    'en',
    'fr',
    'de',
    'pt'
);


ALTER TYPE public.language OWNER TO mido;

--
-- Name: mw_impl; Type: TYPE; Schema: public; Owner: mido
--

CREATE TYPE public.mw_impl AS ENUM (
    'bizhawk_co_op',
    'midos_house'
);


ALTER TYPE public.mw_impl OWNER TO mido;

--
-- Name: notification_kind; Type: TYPE; Schema: public; Owner: mido
--

CREATE TYPE public.notification_kind AS ENUM (
    'decline',
    'resign',
    'accept'
);


ALTER TYPE public.notification_kind OWNER TO mido;

--
-- Name: racetime_pronouns; Type: TYPE; Schema: public; Owner: mido
--

CREATE TYPE public.racetime_pronouns AS ENUM (
    'she',
    'he',
    'they',
    'she_they',
    'he_they',
    'other'
);


ALTER TYPE public.racetime_pronouns OWNER TO mido;

--
-- Name: restream_ok; Type: TYPE; Schema: public; Owner: mido
--

CREATE TYPE public.restream_ok AS ENUM (
    'no',
    'yes',
    'clean_audio'
);


ALTER TYPE public.restream_ok OWNER TO mido;

--
-- Name: role_preference; Type: TYPE; Schema: public; Owner: mido
--

CREATE TYPE public.role_preference AS ENUM (
    'sheikah_only',
    'sheikah_preferred',
    'no_preference',
    'gerudo_preferred',
    'gerudo_only'
);


ALTER TYPE public.role_preference OWNER TO mido;

--
-- Name: role_request_status; Type: TYPE; Schema: public; Owner: postgres
--

CREATE TYPE public.role_request_status AS ENUM (
    'pending',
    'approved',
    'rejected',
    'aborted'
);


ALTER TYPE public.role_request_status OWNER TO postgres;

--
-- Name: rsl_preset; Type: TYPE; Schema: public; Owner: mido
--

CREATE TYPE public.rsl_preset AS ENUM (
    'league',
    'beginner',
    'intermediate',
    'ddr',
    'coop',
    'multiworld',
    's6test'
);


ALTER TYPE public.rsl_preset OWNER TO mido;

--
-- Name: signup_player; Type: TYPE; Schema: public; Owner: mido
--

CREATE TYPE public.signup_player AS (
	id bigint,
	confirmed boolean
);


ALTER TYPE public.signup_player OWNER TO mido;

--
-- Name: signup_status; Type: TYPE; Schema: public; Owner: mido
--

CREATE TYPE public.signup_status AS ENUM (
    'created',
    'confirmed',
    'unconfirmed'
);


ALTER TYPE public.signup_status OWNER TO mido;

--
-- Name: team_config; Type: TYPE; Schema: public; Owner: mido
--

CREATE TYPE public.team_config AS ENUM (
    'solo',
    'coop',
    'pictionary',
    'multiworld',
    'tfbcoop'
);


ALTER TYPE public.team_config OWNER TO mido;

--
-- Name: team_role; Type: TYPE; Schema: public; Owner: mido
--

CREATE TYPE public.team_role AS ENUM (
    'sheikah',
    'gerudo',
    'power',
    'wisdom',
    'courage',
    'none'
);


ALTER TYPE public.team_role OWNER TO mido;

--
-- Name: user_display_source; Type: TYPE; Schema: public; Owner: mido
--

CREATE TYPE public.user_display_source AS ENUM (
    'discord',
    'racetime'
);


ALTER TYPE public.user_display_source OWNER TO mido;

--
-- Name: volunteer_signup_status; Type: TYPE; Schema: public; Owner: postgres
--

CREATE TYPE public.volunteer_signup_status AS ENUM (
    'pending',
    'confirmed',
    'declined',
    'aborted'
);


ALTER TYPE public.volunteer_signup_status OWNER TO postgres;

--
-- Name: mhid(numeric); Type: FUNCTION; Schema: public; Owner: mido
--

CREATE FUNCTION public.mhid(i numeric) RETURNS numeric
    LANGUAGE plpgsql
    AS $$ BEGIN RETURN CASE WHEN i < 0 THEN i + 18446744073709551616 WHEN i > 9223372036854775808 THEN i - 18446744073709551616 ELSE i END; END $$;


ALTER FUNCTION public.mhid(i numeric) OWNER TO mido;

SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: api_keys; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.api_keys (
    key character(32) NOT NULL,
    user_id bigint NOT NULL,
    entrants_read boolean DEFAULT false NOT NULL,
    mw_admin boolean DEFAULT false NOT NULL,
    user_search boolean DEFAULT false NOT NULL,
    write boolean DEFAULT false NOT NULL
);


ALTER TABLE public.api_keys OWNER TO mido;

--
-- Name: async_players; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.async_players (
    series character varying(8) NOT NULL,
    event character varying(8) NOT NULL,
    player bigint NOT NULL,
    vod text,
    "time" interval,
    kind public.async_kind NOT NULL
);


ALTER TABLE public.async_players OWNER TO mido;

--
-- Name: async_teams; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.async_teams (
    team bigint NOT NULL,
    requested timestamp with time zone,
    submitted timestamp with time zone,
    fpa text,
    kind public.async_kind NOT NULL,
    pieces smallint
);


ALTER TABLE public.async_teams OWNER TO mido;

--
-- Name: asyncs; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.asyncs (
    series character varying(8) NOT NULL,
    event character varying(8) NOT NULL,
    discord_role bigint,
    discord_channel bigint,
    web_id bigint,
    web_gen_time timestamp with time zone,
    file_stem text,
    hash1 public.hash_icon,
    hash2 public.hash_icon,
    hash3 public.hash_icon,
    hash4 public.hash_icon,
    hash5 public.hash_icon,
    kind public.async_kind NOT NULL,
    max_delay interval DEFAULT '00:00:00'::interval NOT NULL,
    tfb_uuid uuid,
    start timestamp with time zone,
    end_time timestamp with time zone,
    seed_password character(6),
    is_tfb_dev boolean DEFAULT false NOT NULL,
    xkeys_uuid uuid,
    CONSTRAINT asyncs_seed_password_check CHECK ((seed_password ~ '^[Av><^]{6}$'::text)),
    CONSTRAINT matching_hash_nullness CHECK ((((hash1 IS NULL) = (hash2 IS NULL)) AND ((hash1 IS NULL) = (hash3 IS NULL)) AND ((hash1 IS NULL) = (hash4 IS NULL)) AND ((hash1 IS NULL) = (hash5 IS NULL))))
);


ALTER TABLE public.asyncs OWNER TO mido;

--
-- Name: discord_roles; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.discord_roles (
    guild bigint NOT NULL,
    series character varying(8),
    event character varying(8),
    role public.team_role,
    racetime_team text,
    id bigint NOT NULL
);


ALTER TABLE public.discord_roles OWNER TO mido;

--
-- Name: events; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.events (
    series character varying(8) NOT NULL,
    event character varying(8) NOT NULL,
    display_name text NOT NULL,
    start timestamp with time zone,
    url text,
    video_url text,
    end_time timestamp with time zone,
    listed boolean DEFAULT false NOT NULL,
    teams_url text,
    discord_guild bigint,
    discord_race_room_channel bigint,
    enter_url text,
    hide_teams_tab boolean DEFAULT false NOT NULL,
    short_name text,
    discord_race_results_channel bigint,
    discord_organizer_channel bigint,
    discord_scheduling_channel bigint,
    enter_flow jsonb,
    show_qualifier_times boolean DEFAULT true NOT NULL,
    default_game_count smallint DEFAULT 1 NOT NULL,
    min_schedule_notice interval DEFAULT '00:30:00'::interval NOT NULL,
    hide_races_tab boolean DEFAULT false NOT NULL,
    language public.language DEFAULT 'en'::public.language NOT NULL,
    discord_invite_url text,
    show_opt_out boolean DEFAULT false NOT NULL,
    retime_window interval DEFAULT '00:00:00'::interval NOT NULL,
    auto_import boolean DEFAULT false NOT NULL,
    team_config public.team_config NOT NULL,
    challonge_community text,
    speedgaming_slug text,
    open_stream_delay interval DEFAULT '00:00:00'::interval NOT NULL,
    invitational_stream_delay interval DEFAULT '00:00:00'::interval NOT NULL,
    rando_version jsonb,
    single_settings jsonb,
    manual_reporting_with_breaks boolean DEFAULT false NOT NULL
);


ALTER TABLE public.events OWNER TO mido;

--
-- Name: looking_for_team; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.looking_for_team (
    series character varying(8) NOT NULL,
    event character varying(8) NOT NULL,
    user_id bigint NOT NULL,
    role public.role_preference NOT NULL,
    availability text,
    notes text
);


ALTER TABLE public.looking_for_team OWNER TO mido;

--
-- Name: mw_config; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.mw_config (
    verbose_logging boolean NOT NULL,
    regional_vc boolean NOT NULL,
    verbose_logging_reason text
);


ALTER TABLE public.mw_config OWNER TO mido;

--
-- Name: mw_rooms; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.mw_rooms (
    id bigint NOT NULL,
    name character varying(64) NOT NULL,
    base_queue bytea NOT NULL,
    player_queues bytea NOT NULL,
    password_hash bytea,
    password_salt bytea,
    last_saved timestamp with time zone NOT NULL,
    autodelete_delta interval NOT NULL,
    allow_send_all boolean NOT NULL,
    invites bytea NOT NULL,
    created timestamp with time zone
);


ALTER TABLE public.mw_rooms OWNER TO mido;

--
-- Name: mw_versions; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.mw_versions (
    version smallint NOT NULL,
    last_used timestamp with time zone NOT NULL,
    first_used timestamp with time zone NOT NULL
);


ALTER TABLE public.mw_versions OWNER TO mido;

--
-- Name: notifications; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.notifications (
    rcpt bigint NOT NULL,
    kind public.notification_kind NOT NULL,
    series character varying(8),
    event character varying(8),
    sender bigint,
    id bigint NOT NULL
);


ALTER TABLE public.notifications OWNER TO mido;

--
-- Name: notify_on_delete; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.notify_on_delete (
    message_id bigint NOT NULL,
    series character varying(8) NOT NULL,
    event character varying(8) NOT NULL,
    content text NOT NULL
);


ALTER TABLE public.notify_on_delete OWNER TO mido;

--
-- Name: opt_outs; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.opt_outs (
    series character varying(8) NOT NULL,
    event character varying(8) NOT NULL,
    racetime_id text NOT NULL
);


ALTER TABLE public.opt_outs OWNER TO mido;

--
-- Name: organizers; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.organizers (
    series character varying(8) NOT NULL,
    event character varying(8) NOT NULL,
    organizer bigint NOT NULL
);


ALTER TABLE public.organizers OWNER TO mido;

--
-- Name: phase_round_options; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.phase_round_options (
    series character varying(8) NOT NULL,
    event character varying(8) NOT NULL,
    phase text NOT NULL,
    round text NOT NULL,
    display_fr text
);


ALTER TABLE public.phase_round_options OWNER TO mido;

--
-- Name: prerolled_seeds; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.prerolled_seeds (
    goal_name text NOT NULL,
    file_stem text,
    locked_spoiler_log_path text,
    hash1 public.hash_icon,
    hash2 public.hash_icon,
    hash3 public.hash_icon,
    hash4 public.hash_icon,
    hash5 public.hash_icon,
    seed_password character(6),
    progression_spoiler boolean NOT NULL,
    "timestamp" timestamp with time zone,
    CONSTRAINT matching_hash_nullness CHECK ((((hash1 IS NULL) = (hash2 IS NULL)) AND ((hash1 IS NULL) = (hash3 IS NULL)) AND ((hash1 IS NULL) = (hash4 IS NULL)) AND ((hash1 IS NULL) = (hash5 IS NULL)))),
    CONSTRAINT prerolled_seeds_seed_password_check CHECK ((seed_password ~ '^[Av><^]{6}$'::text))
);


ALTER TABLE public.prerolled_seeds OWNER TO mido;

--
-- Name: race_player_videos; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.race_player_videos (
    race bigint NOT NULL,
    player bigint NOT NULL,
    video text NOT NULL
);


ALTER TABLE public.race_player_videos OWNER TO mido;

--
-- Name: races; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.races (
    startgg_set text,
    start timestamp with time zone,
    series character varying(8) NOT NULL,
    event character varying(8) NOT NULL,
    async_start2 timestamp with time zone,
    async_start1 timestamp with time zone,
    room text,
    scheduling_thread bigint,
    async_room1 text,
    async_room2 text,
    draft_state jsonb,
    async_end1 timestamp with time zone,
    async_end2 timestamp with time zone,
    end_time timestamp with time zone,
    team1 bigint,
    team2 bigint,
    web_id bigint,
    web_gen_time timestamp with time zone,
    file_stem text,
    hash1 public.hash_icon,
    hash2 public.hash_icon,
    hash3 public.hash_icon,
    hash4 public.hash_icon,
    hash5 public.hash_icon,
    game smallint,
    id bigint NOT NULL,
    p1 text,
    p2 text,
    last_edited_by bigint,
    last_edited_at timestamp with time zone,
    video_url text,
    phase text,
    round text,
    ignored boolean DEFAULT false NOT NULL,
    p3 text,
    startgg_event text,
    total integer,
    finished integer,
    tfb_uuid uuid,
    video_url_fr text,
    restreamer text,
    restreamer_fr text,
    locked_spoiler_log_path text,
    video_url_pt text,
    restreamer_pt text,
    p1_twitch text,
    p2_twitch text,
    p1_discord bigint,
    p2_discord bigint,
    schedule_locked boolean DEFAULT false NOT NULL,
    team3 bigint,
    schedule_updated_at timestamp with time zone,
    video_url_de text,
    restreamer_de text,
    sheet_timestamp timestamp without time zone,
    league_id integer,
    p1_racetime text,
    p2_racetime text,
    async_start3 timestamp with time zone,
    async_room3 text,
    async_end3 timestamp with time zone,
    challonge_match text,
    seed_password character(6),
    speedgaming_id bigint,
    notified boolean DEFAULT false NOT NULL,
    is_tfb_dev boolean DEFAULT false NOT NULL,
    fpa_invoked boolean DEFAULT false NOT NULL,
    breaks_used boolean DEFAULT false NOT NULL,
    xkeys_uuid uuid,
    CONSTRAINT async_exclusion CHECK (((start IS NULL) OR ((async_start1 IS NULL) AND (async_start2 IS NULL) AND (async_start3 IS NULL)))),
    CONSTRAINT matching_hash_nullness CHECK ((((hash1 IS NULL) = (hash2 IS NULL)) AND ((hash1 IS NULL) = (hash3 IS NULL)) AND ((hash1 IS NULL) = (hash4 IS NULL)) AND ((hash1 IS NULL) = (hash5 IS NULL)))),
    CONSTRAINT matching_last_edited_nullness CHECK (((last_edited_by IS NULL) = (last_edited_at IS NULL))),
    CONSTRAINT races_seed_password_check CHECK ((seed_password ~ '^[Av><^]{6}$'::text))
);


ALTER TABLE public.races OWNER TO mido;

--
-- Name: restreamers; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.restreamers (
    series character varying(8) NOT NULL,
    event character varying(8) NOT NULL,
    restreamer bigint NOT NULL
);


ALTER TABLE public.restreamers OWNER TO mido;

--
-- Name: role_bindings; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.role_bindings (
    id integer NOT NULL,
    series character varying(50) NOT NULL,
    event character varying(50) NOT NULL,
    role_type_id integer NOT NULL,
    min_count integer DEFAULT 1 NOT NULL,
    max_count integer DEFAULT 1 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE public.role_bindings OWNER TO mido;

--
-- Name: role_bindings_id_seq; Type: SEQUENCE; Schema: public; Owner: mido
--

CREATE SEQUENCE public.role_bindings_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER TABLE public.role_bindings_id_seq OWNER TO mido;

--
-- Name: role_bindings_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: mido
--

ALTER SEQUENCE public.role_bindings_id_seq OWNED BY public.role_bindings.id;


--
-- Name: role_requests; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.role_requests (
    id integer NOT NULL,
    role_binding_id integer NOT NULL,
    user_id bigint NOT NULL,
    status public.role_request_status DEFAULT 'pending'::public.role_request_status NOT NULL,
    notes text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE public.role_requests OWNER TO mido;

--
-- Name: role_requests_id_seq; Type: SEQUENCE; Schema: public; Owner: mido
--

CREATE SEQUENCE public.role_requests_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER TABLE public.role_requests_id_seq OWNER TO mido;

--
-- Name: role_requests_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: mido
--

ALTER SEQUENCE public.role_requests_id_seq OWNED BY public.role_requests.id;


--
-- Name: role_types; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.role_types (
    id integer NOT NULL,
    name character varying(50) NOT NULL
);


ALTER TABLE public.role_types OWNER TO mido;

--
-- Name: role_types_id_seq; Type: SEQUENCE; Schema: public; Owner: mido
--

CREATE SEQUENCE public.role_types_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER TABLE public.role_types_id_seq OWNER TO mido;

--
-- Name: role_types_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: mido
--

ALTER SEQUENCE public.role_types_id_seq OWNED BY public.role_types.id;


--
-- Name: rsl_seeds; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.rsl_seeds (
    room text NOT NULL,
    file_stem text NOT NULL,
    preset public.rsl_preset NOT NULL,
    start timestamp with time zone,
    web_id bigint,
    web_gen_time timestamp with time zone,
    hash1 public.hash_icon NOT NULL,
    hash2 public.hash_icon NOT NULL,
    hash3 public.hash_icon NOT NULL,
    hash4 public.hash_icon NOT NULL,
    hash5 public.hash_icon NOT NULL
);


ALTER TABLE public.rsl_seeds OWNER TO mido;

--
-- Name: signups; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.signups (
    id integer NOT NULL,
    race_id bigint NOT NULL,
    role_binding_id integer NOT NULL,
    user_id bigint NOT NULL,
    status public.volunteer_signup_status DEFAULT 'pending'::public.volunteer_signup_status NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE public.signups OWNER TO mido;

--
-- Name: signups_id_seq; Type: SEQUENCE; Schema: public; Owner: mido
--

CREATE SEQUENCE public.signups_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER TABLE public.signups_id_seq OWNER TO mido;

--
-- Name: signups_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: mido
--

ALTER SEQUENCE public.signups_id_seq OWNED BY public.signups.id;


--
-- Name: speedgaming_disambiguation_messages; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.speedgaming_disambiguation_messages (
    message_id bigint NOT NULL,
    speedgaming_id bigint NOT NULL
);


ALTER TABLE public.speedgaming_disambiguation_messages OWNER TO mido;

--
-- Name: team_members; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.team_members (
    team bigint NOT NULL,
    member bigint NOT NULL,
    status public.signup_status NOT NULL,
    role public.team_role NOT NULL,
    startgg_id character varying(8)
);


ALTER TABLE public.team_members OWNER TO mido;

--
-- Name: teams; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.teams (
    id bigint NOT NULL,
    series character varying(8) NOT NULL,
    event character varying(8) NOT NULL,
    name character varying(64),
    racetime_slug text,
    resigned boolean DEFAULT false NOT NULL,
    restream_consent boolean DEFAULT false NOT NULL,
    startgg_id text,
    plural_name boolean,
    hard_settings_ok boolean DEFAULT false NOT NULL,
    mq_ok boolean DEFAULT false NOT NULL,
    text_field text DEFAULT ''::text NOT NULL,
    mw_impl public.mw_impl,
    text_field2 text DEFAULT ''::text NOT NULL,
    qualifier_rank smallint,
    yes_no boolean,
    challonge_id text,
    lite_ok boolean DEFAULT false NOT NULL,
    flute_ok boolean DEFAULT false NOT NULL,
    all_dungeons_ok boolean DEFAULT false NOT NULL,
    inverted_ok boolean DEFAULT false NOT NULL,
    keydrop_ok boolean DEFAULT false NOT NULL,
    mirror_scroll_ok boolean DEFAULT false NOT NULL,
    no_delay_ok boolean DEFAULT false NOT NULL,
    pb_ok boolean DEFAULT false NOT NULL,
    zw_ok boolean DEFAULT false NOT NULL,
    hover_ok boolean DEFAULT false NOT NULL
);


ALTER TABLE public.teams OWNER TO mido;

--
-- Name: users; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.users (
    id bigint NOT NULL,
    racetime_id text,
    racetime_display_name character varying(32),
    discord_id bigint,
    discord_display_name character varying(32),
    display_source public.user_display_source DEFAULT 'racetime'::public.user_display_source NOT NULL,
    racetime_pronouns public.racetime_pronouns,
    is_archivist boolean DEFAULT false NOT NULL,
    racetime_discriminator smallint,
    discord_discriminator smallint,
    discord_username character varying(32),
    challonge_id text,
    startgg_id character varying(8),
    CONSTRAINT users_check CHECK (((discord_id IS NULL) OR ((discord_discriminator IS NULL) <> (discord_username IS NULL)))),
    CONSTRAINT users_check1 CHECK (((racetime_id IS NULL) = (racetime_display_name IS NULL))),
    CONSTRAINT users_check2 CHECK (((discord_id IS NULL) = (discord_display_name IS NULL)))
);


ALTER TABLE public.users OWNER TO mido;

--
-- Name: view_as; Type: TABLE; Schema: public; Owner: mido
--

CREATE TABLE public.view_as (
    viewer bigint NOT NULL,
    view_as bigint NOT NULL
);


ALTER TABLE public.view_as OWNER TO mido;

--
-- Name: role_bindings id; Type: DEFAULT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.role_bindings ALTER COLUMN id SET DEFAULT nextval('public.role_bindings_id_seq'::regclass);


--
-- Name: role_requests id; Type: DEFAULT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.role_requests ALTER COLUMN id SET DEFAULT nextval('public.role_requests_id_seq'::regclass);


--
-- Name: role_types id; Type: DEFAULT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.role_types ALTER COLUMN id SET DEFAULT nextval('public.role_types_id_seq'::regclass);


--
-- Name: signups id; Type: DEFAULT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.signups ALTER COLUMN id SET DEFAULT nextval('public.signups_id_seq'::regclass);


--
-- Name: api_keys api_keys_pkey; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.api_keys
    ADD CONSTRAINT api_keys_pkey PRIMARY KEY (key);


--
-- Name: async_teams async_teams_pkey; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.async_teams
    ADD CONSTRAINT async_teams_pkey PRIMARY KEY (team, kind);


--
-- Name: discord_roles discord_roles_pkey; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.discord_roles
    ADD CONSTRAINT discord_roles_pkey PRIMARY KEY (id);


--
-- Name: events events_pkey; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.events
    ADD CONSTRAINT events_pkey PRIMARY KEY (series, event);


--
-- Name: mw_rooms mw_rooms_pkey; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.mw_rooms
    ADD CONSTRAINT mw_rooms_pkey PRIMARY KEY (id);


--
-- Name: mw_versions mw_versions_pkey; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.mw_versions
    ADD CONSTRAINT mw_versions_pkey PRIMARY KEY (version);


--
-- Name: notifications notifications_pkey; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_pkey PRIMARY KEY (id);


--
-- Name: notify_on_delete notify_on_delete_pkey; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.notify_on_delete
    ADD CONSTRAINT notify_on_delete_pkey PRIMARY KEY (message_id);


--
-- Name: prerolled_seeds prerolled_seeds_pkey; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.prerolled_seeds
    ADD CONSTRAINT prerolled_seeds_pkey PRIMARY KEY (goal_name);


--
-- Name: races races_pkey; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.races
    ADD CONSTRAINT races_pkey UNIQUE (startgg_set, game);


--
-- Name: races races_pkey1; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.races
    ADD CONSTRAINT races_pkey1 PRIMARY KEY (id);


--
-- Name: role_bindings role_bindings_pkey; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.role_bindings
    ADD CONSTRAINT role_bindings_pkey PRIMARY KEY (id);


--
-- Name: role_bindings role_bindings_series_event_role_type_id_key; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.role_bindings
    ADD CONSTRAINT role_bindings_series_event_role_type_id_key UNIQUE (series, event, role_type_id);


--
-- Name: role_requests role_requests_pkey; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.role_requests
    ADD CONSTRAINT role_requests_pkey PRIMARY KEY (id);


--
-- Name: role_types role_types_name_key; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.role_types
    ADD CONSTRAINT role_types_name_key UNIQUE (name);


--
-- Name: role_types role_types_pkey; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.role_types
    ADD CONSTRAINT role_types_pkey PRIMARY KEY (id);


--
-- Name: rsl_seeds rsl_seeds_pkey; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.rsl_seeds
    ADD CONSTRAINT rsl_seeds_pkey PRIMARY KEY (room);


--
-- Name: signups signups_pkey; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.signups
    ADD CONSTRAINT signups_pkey PRIMARY KEY (id);


--
-- Name: speedgaming_disambiguation_messages speedgaming_disambiguation_messages_message_id_key; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.speedgaming_disambiguation_messages
    ADD CONSTRAINT speedgaming_disambiguation_messages_message_id_key UNIQUE (message_id);


--
-- Name: speedgaming_disambiguation_messages speedgaming_disambiguation_messages_pkey; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.speedgaming_disambiguation_messages
    ADD CONSTRAINT speedgaming_disambiguation_messages_pkey PRIMARY KEY (speedgaming_id);


--
-- Name: teams teams_pkey; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.teams
    ADD CONSTRAINT teams_pkey PRIMARY KEY (id);


--
-- Name: users users_discord_id_key; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_discord_id_key UNIQUE (discord_id);


--
-- Name: users users_pkey; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (id);


--
-- Name: users users_racetime_id_key; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_racetime_id_key UNIQUE (racetime_id);


--
-- Name: view_as view_as_pkey; Type: CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.view_as
    ADD CONSTRAINT view_as_pkey PRIMARY KEY (viewer);


--
-- Name: idx_role_bindings_series_event; Type: INDEX; Schema: public; Owner: mido
--

CREATE INDEX idx_role_bindings_series_event ON public.role_bindings USING btree (series, event);


--
-- Name: idx_role_requests_role_binding_id; Type: INDEX; Schema: public; Owner: mido
--

CREATE INDEX idx_role_requests_role_binding_id ON public.role_requests USING btree (role_binding_id);


--
-- Name: idx_role_requests_status; Type: INDEX; Schema: public; Owner: mido
--

CREATE INDEX idx_role_requests_status ON public.role_requests USING btree (status);


--
-- Name: idx_role_requests_user_id; Type: INDEX; Schema: public; Owner: mido
--

CREATE INDEX idx_role_requests_user_id ON public.role_requests USING btree (user_id);


--
-- Name: idx_signups_race_id; Type: INDEX; Schema: public; Owner: mido
--

CREATE INDEX idx_signups_race_id ON public.signups USING btree (race_id);


--
-- Name: idx_signups_role_binding_id; Type: INDEX; Schema: public; Owner: mido
--

CREATE INDEX idx_signups_role_binding_id ON public.signups USING btree (role_binding_id);


--
-- Name: idx_signups_status; Type: INDEX; Schema: public; Owner: mido
--

CREATE INDEX idx_signups_status ON public.signups USING btree (status);


--
-- Name: idx_signups_user_id; Type: INDEX; Schema: public; Owner: mido
--

CREATE INDEX idx_signups_user_id ON public.signups USING btree (user_id);


--
-- Name: api_keys api_keys_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.api_keys
    ADD CONSTRAINT api_keys_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: async_teams async_requests_team_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.async_teams
    ADD CONSTRAINT async_requests_team_fkey FOREIGN KEY (team) REFERENCES public.teams(id);


--
-- Name: async_players async_submissions_player_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.async_players
    ADD CONSTRAINT async_submissions_player_fkey FOREIGN KEY (player) REFERENCES public.users(id);


--
-- Name: async_players async_submissions_series_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.async_players
    ADD CONSTRAINT async_submissions_series_fkey FOREIGN KEY (series, event) REFERENCES public.events(series, event);


--
-- Name: asyncs asyncs_series_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.asyncs
    ADD CONSTRAINT asyncs_series_fkey FOREIGN KEY (series, event) REFERENCES public.events(series, event);


--
-- Name: discord_roles discord_roles_series_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.discord_roles
    ADD CONSTRAINT discord_roles_series_fkey FOREIGN KEY (series, event) REFERENCES public.events(series, event);


--
-- Name: looking_for_team looking_for_team_event_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.looking_for_team
    ADD CONSTRAINT looking_for_team_event_fkey FOREIGN KEY (series, event) REFERENCES public.events(series, event);


--
-- Name: looking_for_team looking_for_team_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.looking_for_team
    ADD CONSTRAINT looking_for_team_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);


--
-- Name: notifications notifications_event_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_event_fkey FOREIGN KEY (series, event) REFERENCES public.events(series, event);


--
-- Name: notifications notifications_rcpt_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_rcpt_fkey FOREIGN KEY (rcpt) REFERENCES public.users(id);


--
-- Name: notifications notifications_sender_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_sender_fkey FOREIGN KEY (sender) REFERENCES public.users(id);


--
-- Name: notify_on_delete notify_on_delete_series_event_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.notify_on_delete
    ADD CONSTRAINT notify_on_delete_series_event_fkey FOREIGN KEY (series, event) REFERENCES public.events(series, event);


--
-- Name: organizers organizers_organizer_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.organizers
    ADD CONSTRAINT organizers_organizer_fkey FOREIGN KEY (organizer) REFERENCES public.users(id);


--
-- Name: organizers organizers_series_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.organizers
    ADD CONSTRAINT organizers_series_fkey FOREIGN KEY (series, event) REFERENCES public.events(series, event);


--
-- Name: phase_round_options phase_round_options_series_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.phase_round_options
    ADD CONSTRAINT phase_round_options_series_fkey FOREIGN KEY (series, event) REFERENCES public.events(series, event);


--
-- Name: race_player_videos race_videos_player_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.race_player_videos
    ADD CONSTRAINT race_videos_player_fkey FOREIGN KEY (player) REFERENCES public.users(id);


--
-- Name: race_player_videos race_videos_race_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.race_player_videos
    ADD CONSTRAINT race_videos_race_fkey FOREIGN KEY (race) REFERENCES public.races(id);


--
-- Name: races races_last_edited_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.races
    ADD CONSTRAINT races_last_edited_by_fkey FOREIGN KEY (last_edited_by) REFERENCES public.users(id);


--
-- Name: races races_series_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.races
    ADD CONSTRAINT races_series_fkey FOREIGN KEY (series, event) REFERENCES public.events(series, event);


--
-- Name: races races_team1_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.races
    ADD CONSTRAINT races_team1_fkey FOREIGN KEY (team1) REFERENCES public.teams(id);


--
-- Name: races races_team2_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.races
    ADD CONSTRAINT races_team2_fkey FOREIGN KEY (team2) REFERENCES public.teams(id);


--
-- Name: restreamers restreamers_restreamer_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.restreamers
    ADD CONSTRAINT restreamers_restreamer_fkey FOREIGN KEY (restreamer) REFERENCES public.users(id);


--
-- Name: restreamers restreamers_series_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.restreamers
    ADD CONSTRAINT restreamers_series_fkey FOREIGN KEY (series, event) REFERENCES public.events(series, event);


--
-- Name: role_bindings role_bindings_role_type_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.role_bindings
    ADD CONSTRAINT role_bindings_role_type_id_fkey FOREIGN KEY (role_type_id) REFERENCES public.role_types(id) ON DELETE CASCADE;


--
-- Name: role_requests role_requests_role_binding_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.role_requests
    ADD CONSTRAINT role_requests_role_binding_id_fkey FOREIGN KEY (role_binding_id) REFERENCES public.role_bindings(id) ON DELETE CASCADE;


--
-- Name: role_requests role_requests_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.role_requests
    ADD CONSTRAINT role_requests_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: signups signups_race_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.signups
    ADD CONSTRAINT signups_race_id_fkey FOREIGN KEY (race_id) REFERENCES public.races(id) ON DELETE CASCADE;


--
-- Name: signups signups_role_binding_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.signups
    ADD CONSTRAINT signups_role_binding_id_fkey FOREIGN KEY (role_binding_id) REFERENCES public.role_bindings(id) ON DELETE CASCADE;


--
-- Name: signups signups_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.signups
    ADD CONSTRAINT signups_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: team_members team_members_member_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.team_members
    ADD CONSTRAINT team_members_member_fkey FOREIGN KEY (member) REFERENCES public.users(id);


--
-- Name: team_members team_members_team_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.team_members
    ADD CONSTRAINT team_members_team_fkey FOREIGN KEY (team) REFERENCES public.teams(id);


--
-- Name: teams teams_event_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.teams
    ADD CONSTRAINT teams_event_fkey FOREIGN KEY (series, event) REFERENCES public.events(series, event);


--
-- Name: view_as view_as_view_as_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.view_as
    ADD CONSTRAINT view_as_view_as_fkey FOREIGN KEY (view_as) REFERENCES public.users(id);


--
-- Name: view_as view_as_viewer_fkey; Type: FK CONSTRAINT; Schema: public; Owner: mido
--

ALTER TABLE ONLY public.view_as
    ADD CONSTRAINT view_as_viewer_fkey FOREIGN KEY (viewer) REFERENCES public.users(id);


--
-- Name: SCHEMA public; Type: ACL; Schema: -; Owner: postgres
--

GRANT ALL ON SCHEMA public TO mido;



