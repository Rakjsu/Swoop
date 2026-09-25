//! Nomes de arquivo vindos de servidores (Content-Disposition, URL, atributos
//! do Mega) são dados não confiáveis: aqui viram um único componente seguro,
//! válido no Windows, que nunca escapa da pasta de destino.

use percent_encoding::percent_decode_str;
use std::path::{Path, PathBuf};
use url::Url;

/// Tamanho máximo do nome (em caracteres), deixando folga para o caminho do Windows.
pub const MAX_NAME_CHARS: usize = 180;

/// Nome usado quando o servidor não dá nenhum aproveitável.
pub const FALLBACK_NAME: &str = "download";

/// Nomes de dispositivo reservados do Windows (com ou sem extensão).
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Converte um nome qualquer num único componente de caminho seguro.
///
/// Troca separadores e caracteres proibidos por `_`, remove controles, pontos e
/// espaços finais, evita nomes reservados e limita o tamanho preservando a
/// extensão. Nunca devolve vazio, `.` ou `..`.
pub fn sanitize_component(raw: &str) -> String {
    let mut name: String = raw
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();

    let trimmed = name.trim_matches(|c: char| c == ' ' || c == '.').to_owned();
    name = trimmed;
    if name.is_empty() {
        return FALLBACK_NAME.to_owned();
    }

    let stem = name.split('.').next().unwrap_or("").trim_end();
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem)) {
        name.insert(0, '_');
    }

    truncate_keeping_extension(&name, MAX_NAME_CHARS)
}

/// Corta o nome em `max` caracteres mantendo a extensão (se curta).
fn truncate_keeping_extension(name: &str, max: usize) -> String {
    if name.chars().count() <= max {
        return name.to_owned();
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if name.len() - i <= 16 && i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    let keep = max.saturating_sub(ext.chars().count());
    let mut out: String = stem.chars().take(keep).collect();
    out = out.trim_end_matches([' ', '.']).to_owned();
    out.push_str(ext);
    out
}

/// Último segmento não vazio do caminho da URL, com percent-decoding.
pub fn file_name_from_url(url: &Url) -> Option<String> {
    let last = url.path_segments()?.rev().find(|s| !s.is_empty())?;
    let decoded = percent_decode_str(last).decode_utf8_lossy().into_owned();
    (!decoded.trim().is_empty()).then_some(decoded)
}

/// Escolhe o nome final: servidor > Content-Disposition > URL > padrão.
pub fn choose_file_name(
    from_host: Option<&str>,
    from_disposition: Option<&str>,
    url: &Url,
) -> String {
    let raw = from_host
        .map(str::to_owned)
        .or_else(|| from_disposition.map(str::to_owned))
        .or_else(|| file_name_from_url(url))
        .unwrap_or_else(|| FALLBACK_NAME.to_owned());
    sanitize_component(&raw)
}

/// Junta pasta + nome garantindo um filho direto de `base`.
pub fn safe_join(base: &Path, raw_name: &str) -> PathBuf {
    base.join(sanitize_component(raw_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_travessia_e_separadores() {
        assert_eq!(sanitize_component("../../etc/passwd"), "_.._etc_passwd");
        assert_eq!(sanitize_component("..\\..\\win.ini"), "_.._win.ini");
        assert_eq!(sanitize_component(".."), FALLBACK_NAME);
        assert_eq!(sanitize_component("."), FALLBACK_NAME);
        assert_eq!(sanitize_component(""), FALLBACK_NAME);
    }

    #[test]
    fn caracteres_proibidos_no_windows() {
        assert_eq!(sanitize_component("a<b>c:d\"e|f?g*h"), "a_b_c_d_e_f_g_h");
        assert_eq!(sanitize_component("tab\there"), "tab_here");
        assert_eq!(sanitize_component("nome. . "), "nome");
    }

    #[test]
    fn nomes_reservados() {
        assert_eq!(sanitize_component("CON"), "_CON");
        assert_eq!(sanitize_component("nul.txt"), "_nul.txt");
        assert_eq!(sanitize_component("console.txt"), "console.txt");
    }

    #[test]
    fn corta_mantendo_extensao() {
        let longo = format!("{}.part1.rar", "a".repeat(300));
        let out = sanitize_component(&longo);
        assert_eq!(out.chars().count(), MAX_NAME_CHARS);
        assert!(out.ends_with(".rar"));
    }

    #[test]
    fn nome_da_url() {
        let url = Url::parse("https://h.com/dir/Meu%20Arquivo.zip?x=1").unwrap();
        assert_eq!(file_name_from_url(&url).as_deref(), Some("Meu Arquivo.zip"));
        let raiz = Url::parse("https://h.com/").unwrap();
        assert_eq!(file_name_from_url(&raiz), None);
    }

    #[test]
    fn prioridade_do_nome() {
        let url = Url::parse("https://h.com/url.bin").unwrap();
        assert_eq!(
            choose_file_name(Some("host.bin"), Some("cd.bin"), &url),
            "host.bin"
        );
        assert_eq!(choose_file_name(None, Some("cd.bin"), &url), "cd.bin");
        assert_eq!(choose_file_name(None, None, &url), "url.bin");
    }

    #[test]
    fn safe_join_fica_dentro_da_base() {
        let base = Path::new("/dl");
        let p = safe_join(base, "../../x");
        assert_eq!(p.parent(), Some(base));
    }
}
