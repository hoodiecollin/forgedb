-- Hand-verified 1:1 SQL mapping of bench.forge (see docs/BENCHMARKS.md).
-- Written for SQLite; the type choices mirror ForgeDB's storage as closely as a
-- row-store allows so the comparison is on the engine, not the encoding:
--   uuid       -> BLOB (16 bytes, exactly like ForgeDB's fixed uuid column)
--   u64        -> INTEGER
--   bool       -> INTEGER (0/1)
--   timestamp  -> INTEGER (unix seconds, like ForgeDB's i64 timestamp column)
--   string     -> TEXT
--
-- Every ForgeDB index has a matching SQL index so index-probe scenarios compare
-- like for like: &email (unique), ^views, the *author FK, and &name.

CREATE TABLE user (
    id         BLOB PRIMARY KEY NOT NULL,
    name       TEXT NOT NULL,
    email      TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX idx_user_email ON user (email);

CREATE TABLE post (
    id         BLOB PRIMARY KEY NOT NULL,
    title      TEXT NOT NULL,
    views      INTEGER NOT NULL,
    published  INTEGER NOT NULL,
    author     BLOB NOT NULL REFERENCES user (id),
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_post_views  ON post (views);
CREATE INDEX idx_post_author ON post (author);

CREATE TABLE tag (
    id   BLOB PRIMARY KEY NOT NULL,
    name TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_tag_name ON tag (name);

-- Post <-> Tag many-to-many junction (ForgeDB's post_tag_link).
CREATE TABLE post_tag_link (
    post_id BLOB NOT NULL REFERENCES post (id),
    tag_id  BLOB NOT NULL REFERENCES tag  (id),
    PRIMARY KEY (post_id, tag_id)
);
CREATE INDEX idx_post_tag_link_tag ON post_tag_link (tag_id);
