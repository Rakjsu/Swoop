//! O que nunca vai para disco: `sid` do contador, `session_id` do link,
//! token e e-mail da API. Corpos passam por `redact` antes do dump e da
//! pasta de diagnóstico (o plugin lê o corpo inteiro, só na memória).

use regex::Regex;
use std::sync::LazyLock;

/// Campos JSON com sessão ou dado da conta.
static JSON: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"("(?:sid|token|session_id|email|download_url|url)"\s*:\s*")[^"]*""#)
        .expect("regex fixa")
});
/// Sessão na URL do link final.
static QUERY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(session_id=)[A-Za-z0-9_-]+").expect("regex fixa"));

/// Corpo sem sessão nem token.
pub fn redact(body: &str) -> String {
    let body = JSON.replace_all(body, "${1}<oculto>\"");
    QUERY.replace_all(&body, "${1}<oculto>").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tira_sessao_token_e_email() {
        let json = r#"{"state":"started","sid":"abc123"}"#;
        assert_eq!(redact(json), r#"{"state":"started","sid":"<oculto>"}"#);
        let api = r#"{"response":{"token":"tok9","user":{"email":"a@b.c"}},"status":200}"#;
        let out = redact(api);
        assert!(!out.contains("tok9") && !out.contains("a@b.c"), "{out}");
        let page = "return 'https://pr1.rapidgator.net//?r=download/index&session_id=Xy9z';";
        assert!(!redact(page).contains("Xy9z"));
        assert!(redact(page).contains("session_id=<oculto>"));
    }
}
