//! URL segura para logs: sem query, sem fragmento (a chave do Mega fica no
//! `#`) e sem usuário/senha. Mostra só esquema, host e o último segmento.

use std::fmt;
use url::Url;

/// Envolve uma URL para exibição em logs.
pub struct Redacted<'a>(pub &'a Url);

impl fmt::Display for Redacted<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let url = self.0;
        let host = url.host_str().unwrap_or("?");
        let last = url
            .path_segments()
            .and_then(|mut segs| segs.rfind(|s| !s.is_empty()))
            .unwrap_or("");
        let short: String = last.chars().take(48).collect();
        let hidden = if url.query().is_some() || url.fragment().is_some() {
            "?…"
        } else {
            ""
        };
        write!(f, "{}://{}/…/{}{}", url.scheme(), host, short, hidden)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn esconde_query_fragmento_e_credenciais() {
        let url = Url::parse("https://user:pw@mega.nz/file/ABC#chave-secreta?token=x").unwrap();
        let s = Redacted(&url).to_string();
        assert!(!s.contains("chave"));
        assert!(!s.contains("pw"));
        assert!(!s.contains("token"));
        assert_eq!(s, "https://mega.nz/…/ABC?…");
    }
}
