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
/// Sessão ou token numa URL citada no corpo (`?token=`, `?sid=`,
/// `session_id=`, também escapada como `\/…?token=`).
static QUERY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b((?:token|sid|session_id)=)[^&"'\s<\\]+"#).expect("regex fixa")
});

/// Corpo sem sessão nem token.
pub fn redact(body: &str) -> String {
    let body = JSON.replace_all(body, "${1}<oculto>\"");
    QUERY.replace_all(&body, "${1}<oculto>").into_owned()
}

/// Resposta de API ou do contador: JSON redigido; outra coisa (página de
/// erro ou do Cloudflare, que pode repetir o pedido com o token codificado)
/// não é gravada.
pub fn redact_api(body: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(body.trim()) {
        Ok(_) => redact(body),
        Err(_) => format!("<resposta que não é JSON omitida: {} bytes>", body.len()),
    }
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
        let cf = r#"cUPMDTk:"\/api\/v2\/file\/download?token=SEGREDO&file_id=1", x:"?sid=SID9&a""#;
        let out = redact(cf);
        assert!(!out.contains("SEGREDO") && !out.contains("SID9"), "{out}");
        let html = "<html>ru: 'L2FwaS92Mi9maWxlP3Rva2VuPVNFR1JFRE8='</html>";
        assert!(!redact_api(html).contains("L2Fwa"));
        assert!(redact_api(r#"{"sid":"x1"}"#).contains("<oculto>"));
    }
}
