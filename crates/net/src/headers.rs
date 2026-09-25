//! Parsers puros de cabeçalhos HTTP usados pelo motor.

use percent_encoding::percent_decode;
use std::time::{Duration, SystemTime};

/// `Content-Range: bytes <start>-<end>/<total>` (end inclusivo; total pode ser `*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentRange {
    pub start: u64,
    pub end_inclusive: u64,
    pub total: Option<u64>,
}

/// Lê `Content-Range` de uma resposta 206.
pub fn parse_content_range(value: &str) -> Option<ContentRange> {
    let rest = value.trim().strip_prefix("bytes")?.trim_start();
    let (range, total) = rest.split_once('/')?;
    let (start, end) = range.trim().split_once('-')?;
    let start: u64 = start.trim().parse().ok()?;
    let end_inclusive: u64 = end.trim().parse().ok()?;
    let total = match total.trim() {
        "*" => None,
        t => Some(t.parse().ok()?),
    };
    if end_inclusive < start || total.is_some_and(|t| end_inclusive >= t) {
        return None;
    }
    Some(ContentRange {
        start,
        end_inclusive,
        total,
    })
}

/// Extrai o nome de arquivo de `Content-Disposition`, preferindo `filename*`
/// (RFC 5987, UTF-8 ou ISO-8859-1) a `filename`. Não sanitiza: isso é do core.
pub fn parse_content_disposition(value: &str) -> Option<String> {
    let mut plain = None;
    let mut extended = None;
    for param in split_params(value).into_iter().skip(1) {
        let Some((key, val)) = param.split_once('=') else {
            continue;
        };
        match key.trim().to_ascii_lowercase().as_str() {
            "filename*" => extended = decode_ext_value(val.trim()),
            "filename" => plain = Some(unquote(val.trim())),
            _ => {}
        }
    }
    extended.or(plain).filter(|name| !name.trim().is_empty())
}

/// `Retry-After` em segundos ou data HTTP; devolve quanto falta a partir de `now`.
pub fn parse_retry_after(value: &str, now: SystemTime) -> Option<Duration> {
    let v = value.trim();
    if let Ok(secs) = v.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    let when = httpdate::parse_http_date(v).ok()?;
    Some(when.duration_since(now).unwrap_or(Duration::ZERO))
}

/// Divide por `;` respeitando aspas.
fn split_params(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut escaped = false;
    for c in value.chars() {
        match c {
            _ if escaped => {
                cur.push(c);
                escaped = false;
            }
            '\\' if in_quotes => {
                cur.push(c);
                escaped = true;
            }
            '"' => {
                in_quotes = !in_quotes;
                cur.push(c);
            }
            ';' if !in_quotes => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Remove aspas e escapes de uma quoted-string.
fn unquote(v: &str) -> String {
    let Some(inner) = v.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return v.to_owned();
    };
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Decodifica `charset'lang'valor%20codificado`.
fn decode_ext_value(v: &str) -> Option<String> {
    let mut parts = v.splitn(3, '\'');
    let charset = parts.next()?.to_ascii_lowercase();
    let _lang = parts.next()?;
    let bytes: Vec<u8> = percent_decode(parts.next()?.as_bytes()).collect();
    match charset.as_str() {
        "utf-8" => String::from_utf8(bytes).ok(),
        "iso-8859-1" => Some(bytes.iter().map(|&b| b as char).collect()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_range_valido() {
        assert_eq!(
            parse_content_range("bytes 0-0/1234"),
            Some(ContentRange {
                start: 0,
                end_inclusive: 0,
                total: Some(1234)
            })
        );
        assert_eq!(
            parse_content_range("bytes 100-199/*").map(|c| c.total),
            Some(None)
        );
    }

    #[test]
    fn content_range_invalido() {
        assert_eq!(parse_content_range("bytes */1234"), None);
        assert_eq!(parse_content_range("bytes 10-5/100"), None);
        assert_eq!(parse_content_range("bytes 0-100/100"), None);
        assert_eq!(parse_content_range("items 0-1/2"), None);
    }

    #[test]
    fn disposition_prefere_filename_estrela() {
        let v =
            "attachment; filename=\"fallback.zip\"; filename*=UTF-8''Relat%C3%B3rio%20final.zip";
        assert_eq!(
            parse_content_disposition(v).as_deref(),
            Some("Relatório final.zip")
        );
    }

    #[test]
    fn disposition_simples_e_com_escape() {
        assert_eq!(
            parse_content_disposition("attachment; filename=arquivo.rar").as_deref(),
            Some("arquivo.rar")
        );
        assert_eq!(
            parse_content_disposition(r#"inline; filename="a \"b\"; c.txt""#).as_deref(),
            Some(r#"a "b"; c.txt"#)
        );
        assert_eq!(
            parse_content_disposition("attachment; filename*=iso-8859-1'en'caf%E9.txt").as_deref(),
            Some("café.txt")
        );
        assert_eq!(parse_content_disposition("attachment"), None);
    }

    #[test]
    fn retry_after_segundos_e_data() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
        assert_eq!(
            parse_retry_after("120", now),
            Some(Duration::from_secs(120))
        );
        let date = httpdate::fmt_http_date(now + Duration::from_secs(30));
        assert_eq!(parse_retry_after(&date, now), Some(Duration::from_secs(30)));
        assert_eq!(parse_retry_after("amanhã", now), None);
    }
}
