ALTER TABLE posts
    ADD COLUMN tripcode VARCHAR(16)
    CHECK (tripcode IS NULL OR tripcode ~ '^[A-Za-z0-9_-]{16}$');
