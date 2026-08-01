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

-- #218 wide model: 22 columns, all fixed-width, no FK. Drives the update-width
-- axis (ForgeDB appends every column on update; SQLite rewrites the row) and gives
-- a scan subject with no TEXT column. f64 -> REAL is the one type mapping not
-- already covered above.
CREATE TABLE metric (
    id              BLOB PRIMARY KEY NOT NULL,
    recorded_at     INTEGER NOT NULL,
    device_id       INTEGER NOT NULL,
    sample_seq      INTEGER NOT NULL,
    region          INTEGER NOT NULL,
    cpu_pct         REAL    NOT NULL,
    mem_pct         REAL    NOT NULL,
    disk_pct        REAL    NOT NULL,
    net_rx_bytes    INTEGER NOT NULL,
    net_tx_bytes    INTEGER NOT NULL,
    req_count       INTEGER NOT NULL,
    err_count       INTEGER NOT NULL,
    p50_micros      INTEGER NOT NULL,
    p95_micros      INTEGER NOT NULL,
    p99_micros      INTEGER NOT NULL,
    queue_depth     INTEGER NOT NULL,
    open_conns      INTEGER NOT NULL,
    gc_pause_micros INTEGER NOT NULL,
    uptime_secs     INTEGER NOT NULL,
    temp_celsius    REAL    NOT NULL,
    throttled       INTEGER NOT NULL,
    healthy         INTEGER NOT NULL
);
CREATE INDEX idx_metric_device ON metric (device_id);

-- #218 Doc: the variable-width scan subject, Metric's mirror image. Four TEXT
-- columns so a narrow scan is dominated by variable-width reads; `seq`/`kind` are
-- the fixed-only projection that acts as the in-run control.
CREATE TABLE doc (
    id     BLOB PRIMARY KEY NOT NULL,
    seq    INTEGER NOT NULL,
    kind   INTEGER NOT NULL,
    body_a TEXT    NOT NULL,
    body_b TEXT    NOT NULL,
    body_c TEXT    NOT NULL,
    body_d TEXT    NOT NULL
);

-- Post <-> Tag many-to-many junction (ForgeDB's post_tag_link).
CREATE TABLE post_tag_link (
    post_id BLOB NOT NULL REFERENCES post (id),
    tag_id  BLOB NOT NULL REFERENCES tag  (id),
    PRIMARY KEY (post_id, tag_id)
);
CREATE INDEX idx_post_tag_link_tag ON post_tag_link (tag_id);
