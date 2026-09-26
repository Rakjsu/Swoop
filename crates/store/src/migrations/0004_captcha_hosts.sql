-- Fase 4a: captcha pendente e esperas por servidor.
-- `captcha` guarda o desafio (JSON, sem segredo) enquanto o download está em
-- `captcha_needed`; `captcha_token` é a resposta do usuário até o plugin usá-la.
ALTER TABLE downloads ADD COLUMN captcha TEXT;
ALTER TABLE downloads ADD COLUMN captcha_token TEXT;

-- Espera imposta a um servidor inteiro ("espere 10 minutos até o próximo
-- download"): vale para todos os downloads dele e sobrevive a reabrir o app.
CREATE TABLE host_state (
    host_key    TEXT PRIMARY KEY,
    wait_until  INTEGER NOT NULL,
    reason      TEXT NOT NULL
);
