-- Populate seed_gen_type for Triforce Blitz events (OotrTriforceBlitz)
UPDATE events SET seed_gen_type = 'ootr_tfb'
WHERE racetime_goal_slug = 'triforce_blitz' AND seed_gen_type IS NULL;

-- Populate seed_gen_type for RSL and concluded Pictionary RSL events (OotrRsl)
UPDATE events SET seed_gen_type = 'ootr_rsl'
WHERE racetime_goal_slug IN ('rsl', 'pic_rs2') AND seed_gen_type IS NULL;
