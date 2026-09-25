-- Fase 3b: coletor de links. Links colados ficam aqui até o usuário
-- iniciar (ou até o "iniciar sozinho" mandar para a fila). A mesma URL não
-- entra duas vezes. `batch` junta os links de uma mesma colagem (vira um
-- pacote no "iniciar sozinho").

CREATE TABLE collector (
    id         INTEGER PRIMARY KEY,
    url        TEXT NOT NULL UNIQUE,
    host_key   TEXT NOT NULL,
    batch      INTEGER NOT NULL,
    state      TEXT NOT NULL DEFAULT 'unchecked',
    file_name  TEXT,
    size       INTEGER,
    error      TEXT,
    added_at   INTEGER NOT NULL
);

CREATE INDEX collector_by_state ON collector(state);
