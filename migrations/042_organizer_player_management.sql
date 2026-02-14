-- Block players from specific events
CREATE TABLE public.event_blocks (
    series VARCHAR(8) NOT NULL,
    event VARCHAR(8) NOT NULL,
    racetime_id TEXT NOT NULL,
    user_id BIGINT,
    blocked_by BIGINT NOT NULL,
    blocked_at TIMESTAMPTZ DEFAULT NOW() NOT NULL,
    reason TEXT,
    PRIMARY KEY (series, event, racetime_id),
    FOREIGN KEY (series, event) REFERENCES public.events(series, event),
    FOREIGN KEY (user_id) REFERENCES public.users(id),
    FOREIGN KEY (blocked_by) REFERENCES public.users(id)
);

-- Extend opt_outs for organizer-initiated opt-outs
ALTER TABLE public.opt_outs ADD COLUMN user_id BIGINT REFERENCES public.users(id);
ALTER TABLE public.opt_outs ADD COLUMN opted_out_by BIGINT REFERENCES public.users(id);
ALTER TABLE public.opt_outs ADD COLUMN is_organizer_action BOOLEAN DEFAULT FALSE NOT NULL;
ALTER TABLE public.event_blocks OWNER TO mido;
