//! Texto qualquer (colado, página, e-mail) → links. Aceita `http(s)://`,
//! `www.` e endereços sem esquema dos servidores conhecidos
//! (`mediafire.com/file/…`). Remove repetidos mantendo a ordem.

use regex::Regex;
use std::sync::LazyLock;
use url::Url;

/// Candidatos: com esquema, com `www.` ou `host.tld/caminho`.
static CANDIDATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?:https?://|www\.|[a-z0-9-]+(?:\.[a-z0-9-]+)*\.[a-z]{2,}/)[^\s<>"'`]+"#)
        .expect("regex válida")
});

/// Pontuação que costuma grudar no fim de um link dentro de uma frase.
const TRAILING: &[char] = &['.', ',', ';', ':', '!', '?', ')', ']', '}', '>', '*'];

/// Links encontrados no texto. Sem esquema, só vale se o host estiver entre
/// `known_hosts` (evita transformar "arquivo.txt/algo" em link).
pub fn detect(text: &str, known_hosts: &[&str]) -> Vec<Url> {
    let mut out: Vec<Url> = Vec::new();
    for m in CANDIDATE.find_iter(text) {
        let raw = trim(m.as_str());
        let Some(url) = to_url(raw, known_hosts) else {
            continue;
        };
        if !out.contains(&url) {
            out.push(url);
        }
    }
    out
}

/// Tira pontuação do fim; `)` só se não houver `(` aberto no link.
fn trim(raw: &str) -> &str {
    let mut s = raw;
    while let Some(c) = s.chars().last() {
        let keep_paren = c == ')' && s.matches('(').count() >= s.matches(')').count();
        if !TRAILING.contains(&c) || keep_paren {
            break;
        }
        s = &s[..s.len() - c.len_utf8()];
    }
    s
}

fn to_url(raw: &str, known_hosts: &[&str]) -> Option<Url> {
    let lower = raw.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return Url::parse(raw).ok();
    }
    let url = Url::parse(&format!("https://{raw}")).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    let known = lower.starts_with("www.")
        || known_hosts
            .iter()
            .any(|h| host == *h || host.ends_with(&format!(".{h}")));
    known.then_some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KNOWN: &[&str] = &["mediafire.com", "pixeldrain.com"];

    fn found(text: &str) -> Vec<String> {
        detect(text, KNOWN)
            .into_iter()
            .map(|u| u.to_string())
            .collect()
    }

    #[test]
    fn acha_links_em_texto_livre() {
        let text = "Parte 1: https://pixeldrain.com/u/abc123, parte 2 (http://exemplo.com/b.zip).\n\
                    Mirror: mediafire.com/file/xyz/a.rar/file e www.site.org/c.7z!\n\
                    repetido https://pixeldrain.com/u/abc123 e lixo arquivo.txt/algo";
        assert_eq!(
            found(text),
            vec![
                "https://pixeldrain.com/u/abc123",
                "http://exemplo.com/b.zip",
                "https://mediafire.com/file/xyz/a.rar/file",
                "https://www.site.org/c.7z",
            ]
        );
    }

    #[test]
    fn parenteses_do_proprio_link_ficam() {
        assert_eq!(
            found("veja https://site.com/a_(1).zip."),
            vec!["https://site.com/a_(1).zip"]
        );
    }
}
