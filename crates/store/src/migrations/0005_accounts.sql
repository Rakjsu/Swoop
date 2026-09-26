-- Contas premium: uma por servidor. O segredo (senha ou chave) fica no cofre
-- do sistema; aqui só o nome da entrada (`secret_ref`).
CREATE TABLE accounts (
    host_key TEXT PRIMARY KEY,
    username TEXT NOT NULL,
    kind TEXT NOT NULL,
    secret_ref TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'unchecked',
    premium_until INTEGER,
    traffic_left INTEGER,
    checked_at INTEGER,
    error TEXT,
    created_at INTEGER NOT NULL
);
