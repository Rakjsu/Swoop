//! Nome de um pacote a partir dos arquivos (como o Mipony): um arquivo dá o
//! próprio nome; vários, o começo em comum ("filme.part1.rar" +
//! "filme.part2.rar" → "filme").

/// Pacote sem nenhum nome aproveitável.
const FALLBACK: &str = "Links do coletor";

/// Extensão de arquivo (curta, só letras e números).
fn is_ext(ext: &str) -> bool {
    (1..=5).contains(&ext.len()) && ext.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Marca de volume ou compactado que sobra depois da extensão
/// (`.part1`, `.001`, `.7z`, `.rar`, `.zip`, `.tar`).
fn is_volume(ext: &str) -> bool {
    let e = ext.to_ascii_lowercase();
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    e.strip_prefix("part").is_some_and(digits)
        || (e.len() == 3 && digits(&e))
        || matches!(e.as_str(), "7z" | "rar" | "zip" | "tar")
}

/// Nome sem a extensão e sem marcas de volume ("a.part01.rar" → "a";
/// "Meu.Filme.2024.mkv" → "Meu.Filme.2024").
fn stem(name: &str) -> &str {
    let mut s = match name.rsplit_once('.') {
        Some((head, ext)) if !head.is_empty() && is_ext(ext) => head,
        _ => return name,
    };
    while let Some((head, ext)) = s.rsplit_once('.') {
        if head.is_empty() || !is_volume(ext) {
            break;
        }
        s = head;
    }
    s
}

/// Nome do pacote para estes arquivos.
pub fn package_name(names: &[String]) -> String {
    let stems: Vec<&str> = names
        .iter()
        .map(|n| stem(n.trim()))
        .filter(|s| !s.is_empty())
        .collect();
    let Some(first) = stems.first() else {
        return FALLBACK.to_owned();
    };
    if stems.iter().all(|s| s == first) {
        return (*first).to_owned();
    }
    let prefix = stems
        .iter()
        .skip(1)
        .fold(*first, |acc, s| common_prefix(acc, s));
    // Corta no último separador: "Serie S01E0" → "Serie".
    let cut = if stems.iter().any(|s| s.len() > prefix.len()) {
        prefix
            .rfind([' ', '.', '_', '-'])
            .map_or("", |i| &prefix[..i])
    } else {
        prefix
    };
    let cut = cut.trim_matches([' ', '.', '_', '-']);
    if cut.chars().count() >= 3 {
        cut.to_owned()
    } else {
        format!("{first} (+{})", stems.len() - 1)
    }
}

/// Maior começo comum, sem partir caractere no meio.
fn common_prefix<'a>(a: &'a str, b: &str) -> &'a str {
    let end = a
        .char_indices()
        .zip(b.chars())
        .find(|((_, x), y)| x != y)
        .map_or(a.len().min(b.len()), |((i, _), _)| i);
    &a[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(files: &[&str]) -> String {
        package_name(&files.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn nomes_de_pacote() {
        assert_eq!(name(&["video.mp4"]), "video");
        assert_eq!(name(&["Meu.Filme.2024.mkv"]), "Meu.Filme.2024");
        assert_eq!(name(&["filme.part1.rar", "filme.part2.rar"]), "filme");
        assert_eq!(name(&["jogo.7z.001", "jogo.7z.002"]), "jogo");
        assert_eq!(name(&["Serie S01E01.mkv", "Serie S01E02.mkv"]), "Serie");
        assert_eq!(
            name(&["Backup_fotos_1.zip", "Backup_fotos_2.zip"]),
            "Backup_fotos"
        );
        assert_eq!(name(&["a.zip", "b.zip"]), "a (+1)");
        assert_eq!(name(&[]), FALLBACK);
        assert_eq!(name(&["ção.txt", "çãx.txt"]), "ção (+1)");
    }
}
