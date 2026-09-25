-- Fase 1: fila de downloads, segmentos para retomada e histórico.
-- Tempos em milissegundos Unix (INTEGER). Nenhum segredo neste banco.

CREATE TABLE packages (
    id          INTEGER PRIMARY KEY,
    name        TEXT    NOT NULL,
    dest_dir    TEXT    NOT NULL,
    position    INTEGER NOT NULL,
    created_at  INTEGER NOT NULL
);

CREATE TABLE downloads (
    id             INTEGER PRIMARY KEY,
    package_id     INTEGER NOT NULL REFERENCES packages(id) ON DELETE CASCADE,
    url            TEXT    NOT NULL,
    host_key       TEXT,
    state          TEXT    NOT NULL,
    wait_until     INTEGER,
    wait_reason    TEXT,
    error_kind     TEXT,
    error_msg      TEXT,
    attempts       INTEGER NOT NULL DEFAULT 0,
    file_name      TEXT,
    size           INTEGER,
    done_bytes     INTEGER NOT NULL DEFAULT 0,
    resumable      INTEGER NOT NULL DEFAULT 1,
    etag           TEXT,
    last_modified  TEXT,
    part_path      TEXT,
    final_path     TEXT,
    position       INTEGER NOT NULL,
    created_at     INTEGER NOT NULL,
    started_at     INTEGER,
    finished_at    INTEGER
);

CREATE INDEX downloads_by_state ON downloads(state, position);
CREATE INDEX downloads_by_package ON downloads(package_id, position);

-- Faixas [seg_start, seg_end) do arquivo; `pos` = bytes confirmados em disco
-- (sync_data antes de gravar). A retomada pede [pos, seg_end) de cada uma.
CREATE TABLE segments (
    download_id  INTEGER NOT NULL REFERENCES downloads(id) ON DELETE CASCADE,
    idx          INTEGER NOT NULL,
    seg_start    INTEGER NOT NULL,
    seg_end      INTEGER NOT NULL,
    pos          INTEGER NOT NULL,
    PRIMARY KEY (download_id, idx)
);

-- Só recebe inserções: o que terminou (bem ou mal) e quando.
CREATE TABLE history (
    id           INTEGER PRIMARY KEY,
    url          TEXT    NOT NULL,
    host_key     TEXT,
    file_name    TEXT,
    size         INTEGER,
    final_path   TEXT,
    outcome      TEXT    NOT NULL,
    error_msg    TEXT,
    started_at   INTEGER,
    finished_at  INTEGER NOT NULL,
    avg_bps      INTEGER
);
