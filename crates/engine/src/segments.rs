//! Plano de segmentos (puro): como dividir o arquivo entre conexões e onde
//! cortar um segmento em andamento quando uma conexão fica livre.

/// Cortes alinhados a 64 KiB (múltiplo de setor/cluster).
pub const ALIGN: u64 = 64 * 1024;

/// Segmento mínimo: abaixo disso uma conexão nova não compensa.
pub const MIN_SEGMENT: u64 = 1024 * 1024;

/// Faixa `[start, end)` do arquivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u64,
    pub end: u64,
}

/// Arredonda para baixo ao múltiplo de `ALIGN`.
fn align_down(x: u64) -> u64 {
    x - x % ALIGN
}

/// Arredonda para cima ao múltiplo de `ALIGN`.
fn align_up(x: u64) -> u64 {
    x.div_ceil(ALIGN).saturating_mul(ALIGN)
}

/// Divide `[0, size)` em até `conns` faixas contíguas de pelo menos
/// `MIN_SEGMENT` (exceto quando o arquivo inteiro é menor), alinhadas.
pub fn plan(size: u64, conns: u16) -> Vec<Span> {
    if size == 0 {
        return vec![Span { start: 0, end: 0 }];
    }
    let by_size = (size / MIN_SEGMENT).max(1);
    let n = u64::from(conns.max(1)).min(by_size);
    let mut spans = Vec::with_capacity(n as usize);
    let mut start = 0;
    for i in 1..=n {
        let end = if i == n {
            size
        } else {
            align_down(size * i / n).max(start)
        };
        if end > start {
            spans.push(Span { start, end });
            start = end;
        }
    }
    spans
}

/// Onde cortar um segmento em andamento `[cursor, end)` para uma conexão livre.
///
/// - Vítima rápida: corta no meio do que falta, se sobrar ≥ `2 × MIN_SEGMENT`.
/// - Vítima lenta: corta logo à frente do cursor (a conexão livre leva quase
///   tudo), se sobrar ≥ `ALIGN`.
///
/// A vítima continua em `[cursor, corte)` e a conexão livre pega `[corte, end)`.
pub fn split_point(cursor: u64, end: u64, victim_slow: bool) -> Option<u64> {
    let remaining = end.checked_sub(cursor)?;
    let cut = if victim_slow {
        if remaining < ALIGN {
            return None;
        }
        align_up(cursor)
    } else {
        if remaining < 2 * MIN_SEGMENT {
            return None;
        }
        align_down(cursor + remaining / 2)
    };
    // Vítima lenta pode ter corte == cursor: ela para e a livre leva tudo.
    (cut >= cursor && cut < end).then_some(cut)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plano_cobre_o_arquivo() {
        let size = 100 * MIN_SEGMENT + 12345;
        let spans = plan(size, 8);
        assert_eq!(spans.len(), 8);
        assert_eq!(spans[0].start, 0);
        assert_eq!(spans.last().unwrap().end, size);
        for w in spans.windows(2) {
            assert_eq!(w[0].end, w[1].start);
            assert_eq!(w[1].start % ALIGN, 0);
        }
    }

    #[test]
    fn arquivo_pequeno_vira_um_segmento() {
        assert_eq!(
            plan(1000, 8),
            vec![Span {
                start: 0,
                end: 1000
            }]
        );
        assert_eq!(plan(3 * MIN_SEGMENT, 8).len(), 3);
        assert_eq!(plan(0, 8), vec![Span { start: 0, end: 0 }]);
    }

    #[test]
    fn corte_rapido_no_meio_e_lento_no_cursor() {
        let cut = split_point(0, 8 * MIN_SEGMENT, false).unwrap();
        assert_eq!(cut, 4 * MIN_SEGMENT);
        assert_eq!(split_point(0, MIN_SEGMENT, false), None);

        let cut = split_point(1000, 8 * MIN_SEGMENT, true).unwrap();
        assert_eq!(cut, ALIGN);
        assert_eq!(split_point(0, 1000, true), None);
        assert_eq!(split_point(ALIGN, 3 * ALIGN, true), Some(ALIGN));
    }
}
