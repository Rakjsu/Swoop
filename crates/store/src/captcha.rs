//! Captcha de um download: o desafio (JSON do `core::CaptchaChallenge`) fica
//! gravado enquanto espera o usuário; a resposta vale para uma resolução só.

use crate::StoreError;
use rusqlite::{Connection, OptionalExtension, params};
use swoop_core::DownloadId;

/// Guarda o desafio (antes da transição para `captcha_needed`).
pub fn set_challenge(c: &Connection, id: DownloadId, json: &str) -> Result<(), StoreError> {
    c.execute(
        "UPDATE downloads SET captcha = ?2, captcha_token = NULL WHERE id = ?1",
        params![id.0, json],
    )?;
    Ok(())
}

/// Guarda a resposta do usuário.
pub fn set_token(c: &Connection, id: DownloadId, token: &str) -> Result<(), StoreError> {
    c.execute(
        "UPDATE downloads SET captcha_token = ?2 WHERE id = ?1",
        params![id.0, token],
    )?;
    Ok(())
}

/// Desafio pendente (JSON), se houver.
pub fn challenge(c: &Connection, id: DownloadId) -> Result<Option<String>, StoreError> {
    Ok(
        c.query_row("SELECT captcha FROM downloads WHERE id = ?1", [id.0], |r| {
            r.get(0)
        })
        .optional()?
        .flatten(),
    )
}

/// Desafio + resposta, uma vez só: os dois saem do banco.
pub fn take_answer(c: &Connection, id: DownloadId) -> Result<Option<(String, String)>, StoreError> {
    let pair: Option<(Option<String>, Option<String>)> = c
        .query_row(
            "SELECT captcha, captcha_token FROM downloads WHERE id = ?1",
            [id.0],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    clear(c, id)?;
    Ok(match pair {
        Some((Some(challenge), Some(token))) => Some((challenge, token)),
        _ => None,
    })
}

/// Esquece desafio e resposta (pausa, falha, nova tentativa).
pub fn clear(c: &Connection, id: DownloadId) -> Result<(), StoreError> {
    c.execute(
        "UPDATE downloads SET captcha = NULL, captcha_token = NULL WHERE id = ?1",
        [id.0],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{downloads, packages};
    use std::path::Path;

    #[test]
    fn resposta_vale_uma_vez() {
        let mut c = Connection::open_in_memory().unwrap();
        crate::migrations::run(&mut c).unwrap();
        let pkg = packages::insert(&c, "p", Path::new("/tmp"), false).unwrap();
        let id = downloads::insert(&c, pkg, "http://x/a").unwrap();
        assert_eq!(take_answer(&c, id).unwrap(), None);
        set_challenge(&c, id, "{\"d\":1}").unwrap();
        assert_eq!(challenge(&c, id).unwrap().as_deref(), Some("{\"d\":1}"));
        // sem resposta ainda: nada para usar, e o desafio some
        assert_eq!(take_answer(&c, id).unwrap(), None);
        set_challenge(&c, id, "{\"d\":2}").unwrap();
        set_token(&c, id, "tok").unwrap();
        assert_eq!(
            take_answer(&c, id).unwrap(),
            Some(("{\"d\":2}".into(), "tok".into()))
        );
        assert_eq!(take_answer(&c, id).unwrap(), None);
        assert_eq!(challenge(&c, id).unwrap(), None);
    }
}
