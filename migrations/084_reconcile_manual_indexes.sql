DO $migration$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM (VALUES
            ('idx_game_series_series', 'CREATE INDEX idx_game_series_series ON public.game_series USING btree (series)'),
            ('idx_notifications_rcpt', 'CREATE INDEX idx_notifications_rcpt ON public.notifications USING btree (rcpt)'),
            ('idx_organizers_organizer_series_event', 'CREATE INDEX idx_organizers_organizer_series_event ON public.organizers USING btree (organizer, series, event)'),
            ('idx_race_player_videos_race', 'CREATE INDEX idx_race_player_videos_race ON public.race_player_videos USING btree (race)'),
            ('idx_races_series_event', 'CREATE INDEX idx_races_series_event ON public.races USING btree (series, event)'),
            ('idx_team_members_member_status', 'CREATE INDEX idx_team_members_member_status ON public.team_members USING btree (member, status)'),
            ('idx_team_members_team_role', 'CREATE INDEX idx_team_members_team_role ON public.team_members USING btree (team, role)'),
            ('idx_teams_active_series_event', 'CREATE INDEX idx_teams_active_series_event ON public.teams USING btree (series, event) WHERE (NOT resigned)')
        ) AS expected(index_name, index_definition)
        JOIN pg_class index_class
          ON index_class.oid = to_regclass(format('public.%I', expected.index_name))
        WHERE pg_get_indexdef(index_class.oid) <> expected.index_definition
    ) THEN
        RAISE EXCEPTION
            'migration 084 found a same-named index with an unexpected definition';
    END IF;
END
$migration$;

CREATE INDEX IF NOT EXISTS idx_game_series_series
    ON public.game_series (series);
CREATE INDEX IF NOT EXISTS idx_notifications_rcpt
    ON public.notifications (rcpt);
CREATE INDEX IF NOT EXISTS idx_organizers_organizer_series_event
    ON public.organizers (organizer, series, event);
CREATE INDEX IF NOT EXISTS idx_race_player_videos_race
    ON public.race_player_videos (race);
CREATE INDEX IF NOT EXISTS idx_races_series_event
    ON public.races (series, event);
CREATE INDEX IF NOT EXISTS idx_team_members_member_status
    ON public.team_members (member, status);
CREATE INDEX IF NOT EXISTS idx_team_members_team_role
    ON public.team_members (team, role);
CREATE INDEX IF NOT EXISTS idx_teams_active_series_event
    ON public.teams (series, event)
    WHERE NOT resigned;

-- Production was built by applying historical SQL manually and is missing
-- four constraints declared by migration 002. Add them without discarding the
-- two known orphaned game_admins rows; that FK remains NOT VALID until those
-- identities are reconciled in a separate maintenance change. NOT VALID still
-- protects every new or changed row.
DO $migration$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM (VALUES
            ('public.game_admins', 'game_admins_admin_id_fkey', 'FOREIGN KEY (admin_id) REFERENCES users(id) ON DELETE CASCADE'),
            ('public.role_bindings', 'role_bindings_game_id_fkey', 'FOREIGN KEY (game_id) REFERENCES games(id) ON DELETE CASCADE'),
            ('public.role_bindings', 'check_custom_pool', 'CHECK (((NOT custom_pool) OR (game_id IS NOT NULL)))'),
            ('public.role_bindings', 'check_game_or_event', 'CHECK ((((game_id IS NOT NULL) AND (event IS NULL)) OR ((game_id IS NULL) AND (event IS NOT NULL))))')
        ) AS expected(table_name, constraint_name, constraint_definition)
        JOIN pg_constraint constraint_row
          ON constraint_row.conrelid = expected.table_name::regclass
         AND constraint_row.conname = expected.constraint_name
        WHERE pg_get_constraintdef(constraint_row.oid) <> expected.constraint_definition
    ) THEN
        RAISE EXCEPTION
            'migration 084 found a same-named constraint with an unexpected definition';
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'public.game_admins'::regclass
          AND conname = 'game_admins_admin_id_fkey'
    ) THEN
        ALTER TABLE public.game_admins
            ADD CONSTRAINT game_admins_admin_id_fkey
            FOREIGN KEY (admin_id) REFERENCES public.users(id)
            ON DELETE CASCADE NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'public.role_bindings'::regclass
          AND conname = 'role_bindings_game_id_fkey'
    ) THEN
        ALTER TABLE public.role_bindings
            ADD CONSTRAINT role_bindings_game_id_fkey
            FOREIGN KEY (game_id) REFERENCES public.games(id)
            ON DELETE CASCADE NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'public.role_bindings'::regclass
          AND conname = 'check_custom_pool'
    ) THEN
        ALTER TABLE public.role_bindings
            ADD CONSTRAINT check_custom_pool
            CHECK (NOT custom_pool OR game_id IS NOT NULL) NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'public.role_bindings'::regclass
          AND conname = 'check_game_or_event'
    ) THEN
        ALTER TABLE public.role_bindings
            ADD CONSTRAINT check_game_or_event
            CHECK (
                (game_id IS NOT NULL AND event IS NULL)
                OR (game_id IS NULL AND event IS NOT NULL)
            ) NOT VALID;
    END IF;
END
$migration$;

ALTER TABLE public.role_bindings
    VALIDATE CONSTRAINT role_bindings_game_id_fkey;
ALTER TABLE public.role_bindings
    VALIDATE CONSTRAINT check_custom_pool;
ALTER TABLE public.role_bindings
    VALIDATE CONSTRAINT check_game_or_event;
