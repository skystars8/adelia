CREATE TABLE boards (
    id BIGSERIAL PRIMARY KEY,
    slug VARCHAR(32) NOT NULL UNIQUE CHECK (slug ~ '^[a-z0-9][a-z0-9_-]{0,31}$'),
    title VARCHAR(100) NOT NULL,
    subtitle VARCHAR(200) NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    position INTEGER NOT NULL DEFAULT 0,
    threads_per_page SMALLINT NOT NULL DEFAULT 10 CHECK (threads_per_page BETWEEN 5 AND 25),
    max_pages SMALLINT NOT NULL DEFAULT 10 CHECK (max_pages BETWEEN 1 AND 50),
    bump_limit INTEGER NOT NULL DEFAULT 300 CHECK (bump_limit BETWEEN 25 AND 5000),
    max_replies INTEGER NOT NULL DEFAULT 1000 CHECK (max_replies BETWEEN 25 AND 10000),
    read_only BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE posts (
    id BIGSERIAL PRIMARY KEY,
    board_id BIGINT NOT NULL REFERENCES boards(id) ON DELETE CASCADE,
    thread_id BIGINT REFERENCES posts(id) ON DELETE CASCADE,
    name VARCHAR(35) NOT NULL DEFAULT 'Anonymous',
    subject VARCHAR(100) NOT NULL DEFAULT '',
    body TEXT NOT NULL DEFAULT '',
    body_html TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    bumped_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    sticky BOOLEAN NOT NULL DEFAULT FALSE,
    locked BOOLEAN NOT NULL DEFAULT FALSE,
    archived_at TIMESTAMPTZ,
    file_original_name VARCHAR(200),
    file_path VARCHAR(300),
    thumb_path VARCHAR(300),
    file_size BIGINT,
    file_mime VARCHAR(100),
    image_width INTEGER,
    image_height INTEGER,
    CHECK ((file_path IS NULL) = (thumb_path IS NULL)),
    CHECK ((file_path IS NULL) = (file_original_name IS NULL))
);

CREATE INDEX posts_active_threads_idx
    ON posts (board_id, sticky DESC, bumped_at DESC, id DESC)
    WHERE thread_id IS NULL AND archived_at IS NULL;
CREATE INDEX posts_archived_threads_idx
    ON posts (board_id, archived_at DESC, id DESC)
    WHERE thread_id IS NULL AND archived_at IS NOT NULL;
CREATE INDEX posts_replies_idx ON posts (thread_id, id);
CREATE INDEX posts_search_idx ON posts USING GIN (
    to_tsvector('english', COALESCE(subject, '') || ' ' || COALESCE(body, ''))
);

CREATE TABLE moderators (
    id BIGSERIAL PRIMARY KEY,
    username VARCHAR(64) NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role VARCHAR(16) NOT NULL DEFAULT 'moderator' CHECK (role IN ('admin', 'moderator')),
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_login_at TIMESTAMPTZ
);

CREATE TABLE moderator_sessions (
    token_hash BYTEA PRIMARY KEY,
    moderator_id BIGINT NOT NULL REFERENCES moderators(id) ON DELETE CASCADE,
    csrf_token VARCHAR(100) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX moderator_sessions_expiry_idx ON moderator_sessions (expires_at);

CREATE TABLE reports (
    id BIGSERIAL PRIMARY KEY,
    post_id BIGINT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    reason VARCHAR(300) NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'dismissed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMPTZ,
    resolved_by BIGINT REFERENCES moderators(id) ON DELETE SET NULL
);
CREATE INDEX reports_open_idx ON reports (created_at DESC) WHERE status = 'open';

CREATE TABLE news (
    id BIGSERIAL PRIMARY KEY,
    subject VARCHAR(150) NOT NULL,
    body TEXT NOT NULL,
    body_html TEXT NOT NULL,
    author_id BIGINT REFERENCES moderators(id) ON DELETE SET NULL,
    author_name VARCHAR(64) NOT NULL DEFAULT 'Admin',
    published BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX news_published_idx ON news (created_at DESC) WHERE published;

CREATE TABLE moderation_log (
    id BIGSERIAL PRIMARY KEY,
    moderator_id BIGINT REFERENCES moderators(id) ON DELETE SET NULL,
    action VARCHAR(80) NOT NULL,
    detail TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO boards (slug, title, subtitle, description, position) VALUES
    ('general', 'General Discussion', 'Conversation for the whole community', 'News, questions, introductions, and topics that do not need a specialized board.', 10),
    ('learning', 'Learning & Questions', 'Share knowledge and ask thoughtful questions', 'Educational discussion, guides, resources, study groups, and requests for help.', 20),
    ('projects', 'Projects & Showcase', 'Show what you are making', 'Hobby projects, work in progress, tutorials, feedback, and completed creations.', 30),
    ('community', 'Community', 'Events, clubs, and site discussion', 'Community activities, local groups, online events, and discussion about the site itself.', 40);

INSERT INTO news (subject, body, body_html, author_name) VALUES (
    'Welcome to Adelia',
    'Adelia is a fast, independent place for hobby and educational communities. Choose a board from the frame and make yourself at home.',
    'Adelia is a fast, independent place for hobby and educational communities. Choose a board from the frame and make yourself at home.',
    'Admin'
);
