//! "Botões" do servidor de teste, lidos da query string de `/file/{nome}`.
//!
//! | parâmetro     | efeito                                                        |
//! |---------------|---------------------------------------------------------------|
//! | `size`        | tamanho em bytes (padrão 1 MiB)                               |
//! | `seed`        | semente do conteúdo (padrão 1)                                |
//! | `norange=1`   | ignora Range: sempre 200 com o arquivo inteiro                |
//! | `rate`        | bytes/s por conexão                                           |
//! | `slow_at`     | conexão cujo Range começa exatamente aqui fica lenta…         |
//! | `slow_rate`   | …a esta taxa (bytes/s; padrão 64 KiB/s)                       |
//! | `drop_after`  | derruba a conexão depois de enviar N bytes                    |
//! | `fail`        | fração (0–1) de requisições que recebem 503                   |
//! | `valid_until` | ms Unix; depois disso responde 403 (link expirado)            |
//! | `cd=1`        | envia Content-Disposition com o nome                          |
//! | `ctype`       | Content-Type da resposta (padrão `application/octet-stream`)  |

use serde::Deserialize;

/// Configuração de uma requisição.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Knobs {
    pub size: u64,
    pub seed: u64,
    pub norange: u8,
    pub rate: Option<u64>,
    pub slow_at: Option<u64>,
    pub slow_rate: u64,
    pub drop_after: Option<u64>,
    pub fail: f64,
    pub valid_until: Option<i64>,
    pub cd: u8,
    pub ctype: Option<String>,
}

impl Default for Knobs {
    fn default() -> Self {
        Self {
            size: 1 << 20,
            seed: 1,
            norange: 0,
            rate: None,
            slow_at: None,
            slow_rate: 64 * 1024,
            drop_after: None,
            fail: 0.0,
            valid_until: None,
            cd: 0,
            ctype: None,
        }
    }
}

impl Knobs {
    /// Taxa (bytes/s) para uma conexão que começa em `start`.
    pub fn rate_for(&self, start: u64) -> Option<u64> {
        if self.slow_at == Some(start) {
            Some(self.slow_rate)
        } else {
            self.rate
        }
    }
}

/// Faixa pedida em `Range: bytes=a-b` / `bytes=a-` (b inclusivo), já limitada ao tamanho.
pub fn parse_range(header: &str, size: u64) -> Option<(u64, u64)> {
    let spec = header.trim().strip_prefix("bytes=")?;
    if spec.contains(',') {
        return None;
    }
    let (a, b) = spec.split_once('-')?;
    let start: u64 = a.trim().parse().ok()?;
    let end_incl = match b.trim() {
        "" => size.checked_sub(1)?,
        b => b.parse::<u64>().ok()?.min(size.checked_sub(1)?),
    };
    (start <= end_incl && start < size).then_some((start, end_incl))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faixas() {
        assert_eq!(parse_range("bytes=0-0", 10), Some((0, 0)));
        assert_eq!(parse_range("bytes=5-", 10), Some((5, 9)));
        assert_eq!(parse_range("bytes=5-100", 10), Some((5, 9)));
        assert_eq!(parse_range("bytes=10-", 10), None);
        assert_eq!(parse_range("bytes=0-1,3-4", 10), None);
        assert_eq!(parse_range("items=0-1", 10), None);
    }
}
