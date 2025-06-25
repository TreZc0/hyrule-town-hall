-- Create enum types for status fields
CREATE TYPE role_request_status AS ENUM ('pending', 'approved', 'rejected');
CREATE TYPE volunteer_signup_status AS ENUM ('pending', 'confirmed', 'declined');

CREATE TABLE role_types (
    id SERIAL PRIMARY KEY,
    name VARCHAR(50) NOT NULL UNIQUE
);

INSERT INTO role_types (name) VALUES 
    ('Commentator'),
    ('Tracker'), 
    ('Race Monitor');

CREATE TABLE role_bindings (
    id SERIAL PRIMARY KEY,
    series VARCHAR(50) NOT NULL,
    event VARCHAR(50) NOT NULL,
    role_type_id INTEGER NOT NULL REFERENCES role_types(id) ON DELETE CASCADE,
    min_count INTEGER NOT NULL DEFAULT 1,
    max_count INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(series, event, role_type_id)
);

CREATE TABLE role_requests (
    id SERIAL PRIMARY KEY,
    role_binding_id INTEGER NOT NULL REFERENCES role_bindings(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status role_request_status NOT NULL DEFAULT 'pending',
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(role_binding_id, user_id)
);

CREATE TABLE signups (
    id SERIAL PRIMARY KEY,
    race_id BIGINT NOT NULL REFERENCES races(id) ON DELETE CASCADE,
    role_binding_id INTEGER NOT NULL REFERENCES role_bindings(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status volunteer_signup_status NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(race_id, role_binding_id, user_id)
);

CREATE INDEX idx_role_bindings_series_event ON role_bindings(series, event);
CREATE INDEX idx_role_requests_role_binding_id ON role_requests(role_binding_id);
CREATE INDEX idx_role_requests_status ON role_requests(status);
CREATE INDEX idx_role_requests_user_id ON role_requests(user_id);
CREATE INDEX idx_signups_race_id ON signups(race_id);
CREATE INDEX idx_signups_role_binding_id ON signups(role_binding_id);
CREATE INDEX idx_signups_status ON signups(status);
CREATE INDEX idx_signups_user_id ON signups(user_id); 