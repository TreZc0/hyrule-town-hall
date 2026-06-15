-- Add language column; default 'en' temporarily so existing rows get a value
ALTER TABLE restreamers ADD COLUMN language language NOT NULL DEFAULT 'en';

-- Copy existing rows for the other three languages
INSERT INTO restreamers (series, event, restreamer, language)
SELECT series, event, restreamer, unnest(ARRAY['fr'::language, 'de'::language, 'pt'::language])
FROM (SELECT DISTINCT series, event, restreamer FROM restreamers WHERE language = 'en') existing;

-- Add primary key on the full tuple (replaces the implicit uniqueness the app enforced)
ALTER TABLE restreamers ADD PRIMARY KEY (series, event, restreamer, language);

-- Drop the default — language must be explicit going forward
ALTER TABLE restreamers ALTER COLUMN language DROP DEFAULT;
