ALTER TABLE boards
    ADD COLUMN require_approval BOOLEAN NOT NULL DEFAULT FALSE;

ALTER TABLE posts
    ADD COLUMN approved_at TIMESTAMPTZ,
    ADD COLUMN approved_by BIGINT REFERENCES moderators(id) ON DELETE SET NULL;

UPDATE posts
SET approved_at = created_at
WHERE approved_at IS NULL;

ALTER TABLE posts
    ALTER COLUMN approved_at SET DEFAULT NOW();

CREATE INDEX posts_pending_approval_idx
    ON posts (created_at, id)
    WHERE approved_at IS NULL;
