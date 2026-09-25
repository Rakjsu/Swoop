//! Tamanhos legíveis: `2M`, `512K`, `1.5G` (múltiplos de 1024).

/// Texto de tamanho inválido.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("tamanho inválido: {0:?} (use por exemplo 512K, 2M, 1.5G)")]
pub struct UnitError(pub String);

/// Converte `2M`, `2MiB`, `2mb`, `512k`, `1.5G` ou `1024` em bytes.
pub fn parse_bytes(input: &str) -> Result<u64, UnitError> {
    let err = || UnitError(input.to_owned());
    let s = input.trim();
    let split = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let value: f64 = num.parse().map_err(|_| err())?;
    let mult: f64 = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1.0,
        "k" | "kb" | "kib" => 1024.0,
        "m" | "mb" | "mib" => 1024.0 * 1024.0,
        "g" | "gb" | "gib" => 1024.0 * 1024.0 * 1024.0,
        _ => return Err(err()),
    };
    let bytes = value * mult;
    if !bytes.is_finite() || bytes < 0.0 || bytes > u64::MAX as f64 {
        return Err(err());
    }
    Ok(bytes.round() as u64)
}

/// Formata bytes para leitura humana (`12.3 MiB`).
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_tamanhos() {
        assert_eq!(parse_bytes("1024"), Ok(1024));
        assert_eq!(parse_bytes("512K"), Ok(512 * 1024));
        assert_eq!(parse_bytes("2M"), Ok(2 * 1024 * 1024));
        assert_eq!(parse_bytes("2MiB"), Ok(2 * 1024 * 1024));
        assert_eq!(parse_bytes("1.5g"), Ok(1024 * 1024 * 1024 * 3 / 2));
        assert!(parse_bytes("dois").is_err());
        assert!(parse_bytes("2X").is_err());
        assert!(parse_bytes("").is_err());
    }

    #[test]
    fn formata_tamanhos() {
        assert_eq!(format_bytes(10), "10 B");
        assert_eq!(format_bytes(1536), "1.5 KiB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.0 MiB");
    }
}
