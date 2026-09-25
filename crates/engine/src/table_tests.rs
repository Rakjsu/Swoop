use super::*;
use crate::segments::{MIN_SEGMENT, plan};
use proptest::prelude::*;

/// Confere o invariante: faixas disjuntas cobrindo `[0, size)` e
/// `start ≤ written ≤ cursor ≤ end`.
fn check(t: &Table, size: u64) {
    let mut segs = t.segs.clone();
    segs.sort_by_key(|s| s.start);
    let mut expect = 0;
    for s in &segs {
        assert_eq!(s.start, expect, "buraco ou sobreposição em {s:?}");
        assert!(
            s.start <= s.written && s.written <= s.cursor && s.cursor <= s.end,
            "{s:?}"
        );
        expect = s.end;
    }
    assert_eq!(expect, size);
}

#[test]
fn divisao_rapida_corta_no_meio() {
    let size = 8 * MIN_SEGMENT;
    let mut t = Table::from_spans(&plan(size, 1));
    let now = Instant::now();
    let a = t.claim(0, now).unwrap();
    assert_eq!((a.from, a.to), (0, size));
    let b = t.steal(1, now).unwrap();
    assert_eq!((b.from, b.to), (4 * MIN_SEGMENT, size));
    check(&t, size);

    // A vítima não passa do novo fim.
    let adv = t.advance(a.idx, (5 * MIN_SEGMENT) as usize, now);
    assert_eq!(adv.accepted as u64, 4 * MIN_SEGMENT);
    assert!(adv.finished);
}

#[test]
fn vitima_lenta_perde_quase_tudo() {
    let size = 16 * MIN_SEGMENT;
    let mut t = Table::from_spans(&plan(size, 2));
    let t0 = Instant::now();
    let slow = t.claim(0, t0).unwrap();
    let fast = t.claim(1, t0).unwrap();
    t.advance(slow.idx, 10_000, t0);
    t.advance(fast.idx, (6 * MIN_SEGMENT) as usize, t0);
    let later = t0 + Duration::from_secs(2);
    let stolen = t.steal(2, later).unwrap();
    // A lenta (faixa 0) tem mais bytes faltando e anda 600× mais devagar.
    assert_eq!(stolen.from, ALIGN_TEST);
    assert_eq!(stolen.to, slow.to);
    check(&t, size);
}

const ALIGN_TEST: u64 = crate::segments::ALIGN;

/// Regressão (CI, 25/09): todas as rápidas terminaram juntas e só a lenta
/// seguia em andamento; comparada só consigo mesma, ela não era "lenta" e
/// virava divisões ao meio até sobrar ~2 MiB a 64 KiB/s.
#[test]
fn lenta_e_detectada_mesmo_com_as_rapidas_ja_terminadas() {
    let size = 16 * MIN_SEGMENT;
    let mut t = Table::from_spans(&plan(size, 2));
    let t0 = Instant::now();
    let slow = t.claim(0, t0).unwrap();
    let fast = t.claim(1, t0).unwrap();
    let later = t0 + Duration::from_secs(2);
    t.advance(slow.idx, 10_000, later);
    let done = t.advance(fast.idx, (8 * MIN_SEGMENT) as usize, later);
    assert!(done.finished);

    let stolen = t.steal(1, later).unwrap();
    assert_eq!(stolen.from, ALIGN_TEST, "a lenta devia perder quase tudo");
    assert_eq!(stolen.to, slow.to);
    check(&t, size);
}

#[test]
fn retomada_continua_do_confirmado() {
    let rows = [
        SegmentRow {
            idx: 0,
            start: 0,
            end: 100,
            pos: 60,
        },
        SegmentRow {
            idx: 3,
            start: 100,
            end: 200,
            pos: 200,
        },
    ];
    let mut t = Table::from_rows(&rows);
    assert_eq!(t.received(), 160);
    assert_eq!(t.pending(), 1);
    let c = t.claim(0, Instant::now()).unwrap();
    assert_eq!((c.idx, c.from, c.to), (0, 60, 100));
    assert!(t.claim(1, Instant::now()).is_none());
    t.advance(0, 40, Instant::now());
    t.mark_written(0, 100);
    assert!(t.is_complete());
}

#[derive(Debug, Clone)]
enum Op {
    Claim(usize),
    Steal(usize),
    Advance(usize, u32),
    Write(usize),
    Release(usize),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0usize..6).prop_map(Op::Claim),
        (0usize..6).prop_map(Op::Steal),
        (0usize..32, 1u32..3_000_000).prop_map(|(i, n)| Op::Advance(i, n)),
        (0usize..32).prop_map(Op::Write),
        (0usize..32).prop_map(Op::Release),
    ]
}

proptest! {
    /// Qualquer sequência de operações mantém o invariante (portão 1g).
    #[test]
    fn invariante_sob_operacoes_aleatorias(
        size in 1u64..(40 * MIN_SEGMENT),
        conns in 1u16..10,
        ops in proptest::collection::vec(op(), 1..200),
    ) {
        let mut t = Table::from_spans(&plan(size, conns));
        let mut clock = Instant::now();
        for op in ops {
            clock += Duration::from_millis(300);
            let idxs: Vec<u32> = t.segs.iter().map(|s| s.idx).collect();
            let pick = |i: usize| idxs[i % idxs.len()];
            match op {
                Op::Claim(w) => { t.claim(w, clock); }
                Op::Steal(w) => { t.steal(w, clock); }
                Op::Advance(i, n) => { t.advance(pick(i), n as usize, clock); }
                Op::Write(i) => {
                    let idx = pick(i);
                    let cursor = t.segs.iter().find(|s| s.idx == idx).unwrap().cursor;
                    t.mark_written(idx, cursor);
                }
                Op::Release(i) => t.release(pick(i)),
            }
            check(&t, size);
        }
    }
}
