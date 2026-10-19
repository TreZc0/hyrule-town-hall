ALTER TABLE pooled_qualifier_configs
    ADD COLUMN include_opted_out_in_par BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN include_unqualified_in_par BOOLEAN NOT NULL DEFAULT TRUE;

-- Apply policy at read time so changing it also recalculates historical results.
-- Participation depends on finalized counted results, not scores (which depend on par).
CREATE FUNCTION qualifier_par_eligible(s TEXT, e TEXT, runner TEXT) RETURNS BOOLEAN LANGUAGE SQL STABLE AS $$
    SELECT (config.include_opted_out_in_par OR NOT EXISTS (
        SELECT 1 FROM opt_outs
        LEFT JOIN users ON users.id = opt_outs.user_id
        WHERE opt_outs.series = s AND opt_outs.event = e
          AND (opt_outs.racetime_id = runner OR users.racetime_id = runner)
    )) AND (config.include_unqualified_in_par OR (
        SELECT COUNT(DISTINCT attempt.mode_id)
        FROM qualifier_attempts attempt JOIN qualifier_modes mode ON mode.id = attempt.mode_id
        WHERE attempt.series = s AND attempt.event = e AND attempt.racetime_id = runner
          AND mode.enabled AND attempt.counts_for_entrant AND attempt.state = 'finalized'
    ) = config.required_mode_count)
    FROM pooled_qualifier_configs config WHERE config.series = s AND config.event = e
$$;
