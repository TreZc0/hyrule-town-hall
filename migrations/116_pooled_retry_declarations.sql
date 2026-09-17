ALTER TABLE qualifier_attempts
    ADD COLUMN retry_declared_at TIMESTAMPTZ,
    ADD COLUMN retry_declared_by BIGINT REFERENCES users(id),
    ADD CHECK ((retry_declared_at IS NULL) = (retry_declared_by IS NULL));

-- Preserve existing declarations, but remove their advance race selection.
UPDATE qualifier_attempts attempt
SET retry_declared_at = entry.retry_reserved_at,
    retry_declared_by = entry.retry_declared_by
FROM qualifier_live_entries entry
WHERE entry.retry_original_attempt_id = attempt.id
  AND entry.retry_reserved_at IS NOT NULL
  AND entry.retry_committed_at IS NULL AND entry.retry_released_at IS NULL
  AND attempt.counts_for_entrant AND attempt.state <> 'void';

UPDATE qualifier_live_entries SET retry_released_at = NOW()
WHERE retry_reserved_at IS NOT NULL
  AND retry_committed_at IS NULL AND retry_released_at IS NULL
  AND eligibility_frozen_at IS NULL;

CREATE UNIQUE INDEX qualifier_attempts_one_pending_retry_declaration
    ON qualifier_attempts(team_id)
    WHERE retry_declared_at IS NOT NULL AND counts_for_entrant AND state <> 'void';
