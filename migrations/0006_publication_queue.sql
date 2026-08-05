CREATE TABLE publication_jobs (
    target TEXT PRIMARY KEY,
    job_kind TEXT NOT NULL CHECK (job_kind IN ('home', 'board', 'thread')),
    board_id BIGINT REFERENCES boards(id) ON DELETE CASCADE,
    thread_id BIGINT REFERENCES posts(id) ON DELETE CASCADE,
    generation BIGINT NOT NULL DEFAULT 1 CHECK (generation > 0),
    queued_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    available_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    claimed_at TIMESTAMPTZ,
    claimed_generation BIGINT,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    last_error TEXT,
    CHECK (
        (job_kind = 'home' AND board_id IS NULL AND thread_id IS NULL)
        OR (job_kind = 'board' AND board_id IS NOT NULL AND thread_id IS NULL)
        OR (job_kind = 'thread' AND board_id IS NOT NULL AND thread_id IS NOT NULL)
    )
);

CREATE INDEX publication_jobs_ready_idx
    ON publication_jobs (available_at, queued_at)
    WHERE claimed_at IS NULL;
