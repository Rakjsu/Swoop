-- Fase 2b: preferências do usuário (chave → JSON). Fonte única para o app,
-- o painel e o `serve`; o CLI com opções explícitas não grava aqui.
-- Nenhum segredo nesta tabela (contas vão para o cofre do SO).

CREATE TABLE settings (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);

CREATE INDEX history_by_finished ON history(finished_at DESC);

-- Pacote com pasta automática: cada arquivo vai para a pasta do seu tipo
-- (vídeos, músicas ou downloads), decidida quando o nome real é conhecido.
-- `dest_dir` continua sendo a pasta de recuo.
ALTER TABLE packages ADD COLUMN auto_dest INTEGER NOT NULL DEFAULT 0;
