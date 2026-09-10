-- The manually provisioned production mappings include IDs beyond the sequence.
-- Without this, the first new series assignment can collide with an existing row.
LOCK TABLE game_series IN SHARE ROW EXCLUSIVE MODE;
SELECT setval(
    pg_get_serial_sequence('public.game_series', 'id'),
    GREATEST(
        COALESCE((SELECT MAX(id) FROM game_series), 0),
        nextval(pg_get_serial_sequence('public.game_series', 'id'))
    ),
    TRUE
);
