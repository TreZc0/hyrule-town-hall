-- NULL retains the exact defaults of the selected historical score kind.
ALTER TABLE events ADD COLUMN qualifier_score_config JSONB;
ALTER TABLE events ADD CONSTRAINT qualifier_score_config_object
    CHECK (qualifier_score_config IS NULL OR jsonb_typeof(qualifier_score_config) = 'object');
