-- The schema that Alembic managed before sqlx. A database that Alembic migrated already has its tables, so every
-- statement skips what exists and the migration only drops Alembic's version table.

CREATE TABLE IF NOT EXISTS app_setting (
    key VARCHAR(80) NOT NULL,
    value TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT pk_app_setting PRIMARY KEY (key)
);

CREATE TABLE IF NOT EXISTS source_channel (
    id SERIAL NOT NULL,
    username VARCHAR(120) NOT NULL,
    active BOOLEAN NOT NULL DEFAULT true,
    -- The newest post the feed delivered, so that it resends nothing after a restart.
    last_committed_message_id INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT pk_source_channel PRIMARY KEY (id),
    CONSTRAINT uq_source_channel_username UNIQUE (username)
);
-- The feed and the Mini App page through active channels in username order.
CREATE INDEX IF NOT EXISTS ix_source_channel_active_username ON source_channel (active, username);

CREATE TABLE IF NOT EXISTS sent_recap_story (
    id SERIAL NOT NULL,
    url TEXT NOT NULL,
    title TEXT NOT NULL,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT pk_sent_recap_story PRIMARY KEY (id)
);
-- Recaps read the newest stories and prune the oldest ones, both by send time.
CREATE INDEX IF NOT EXISTS ix_sent_recap_story_sent_at ON sent_recap_story (sent_at);

-- Alembic's last revision deleted these settings; a database migrated only up to an earlier revision still has them.
DELETE FROM app_setting WHERE key IN ('recap_model', 'recap_thinking', 'recap_available_models');

DROP TABLE IF EXISTS alembic_version;
