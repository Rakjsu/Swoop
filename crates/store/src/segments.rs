//! Segmentos persistidos de um download: o que falta baixar ao retomar.

use crate::StoreError;
use rusqlite::{Connection, params};
use swoop_core::DownloadId;

/// Faixa `[start, end)` do arquivo; `pos` = bytes confirmados em disco.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentRow {
    pub idx: u32,
    pub start: u64,
    pub end: u64,
    pub pos: u64,
}

impl SegmentRow {
    /// Bytes ainda não confirmados.
    pub fn remaining(&self) -> u64 {
        self.end.saturating_sub(self.pos)
    }
}

/// Segmentos do download em ordem de posição no arquivo.
pub fn load(c: &Connection, id: DownloadId) -> Result<Vec<SegmentRow>, StoreError> {
    let mut stmt = c.prepare(
        "SELECT idx, seg_start, seg_end, pos FROM segments
         WHERE download_id = ?1 ORDER BY seg_start",
    )?;
    let rows = stmt.query_map([id.0], |r| {
        Ok(SegmentRow {
            idx: r.get(0)?,
            start: r.get::<_, i64>(1)? as u64,
            end: r.get::<_, i64>(2)? as u64,
            pos: r.get::<_, i64>(3)? as u64,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Troca a tabela inteira de segmentos (checkpoint) numa transação, junto com
/// o total confirmado em `downloads.done_bytes`.
pub fn replace(c: &mut Connection, id: DownloadId, segs: &[SegmentRow]) -> Result<(), StoreError> {
    let tx = c.transaction()?;
    tx.execute("DELETE FROM segments WHERE download_id = ?1", [id.0])?;
    {
        let mut ins = tx.prepare(
            "INSERT INTO segments (download_id, idx, seg_start, seg_end, pos)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for s in segs {
            ins.execute(params![
                id.0,
                s.idx,
                s.start as i64,
                s.end as i64,
                s.pos as i64
            ])?;
        }
    }
    let done: u64 = segs.iter().map(|s| s.pos.min(s.end) - s.start).sum();
    tx.execute(
        "UPDATE downloads SET done_bytes = ?2 WHERE id = ?1",
        params![id.0, done as i64],
    )?;
    tx.commit()?;
    Ok(())
}

/// Apaga os segmentos (arquivo mudou no servidor ou download recomeça do zero).
pub fn clear(c: &Connection, id: DownloadId) -> Result<(), StoreError> {
    c.execute("DELETE FROM segments WHERE download_id = ?1", [id.0])?;
    c.execute("UPDATE downloads SET done_bytes = 0 WHERE id = ?1", [id.0])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{downloads, packages};

    #[test]
    fn checkpoint_substitui_e_soma_confirmados() {
        let mut c = Connection::open_in_memory().unwrap();
        crate::migrations::run(&mut c).unwrap();
        let pkg = packages::insert(&c, "p", std::path::Path::new("/tmp"), false).unwrap();
        let id = downloads::insert(&c, pkg, "http://x/a").unwrap();

        let segs = [
            SegmentRow {
                idx: 0,
                start: 0,
                end: 100,
                pos: 40,
            },
            SegmentRow {
                idx: 1,
                start: 100,
                end: 200,
                pos: 200,
            },
        ];
        replace(&mut c, id, &segs).unwrap();
        assert_eq!(load(&c, id).unwrap(), segs);
        assert_eq!(downloads::get(&c, id).unwrap().unwrap().done_bytes, 140);

        replace(&mut c, id, &segs[..1]).unwrap();
        assert_eq!(load(&c, id).unwrap().len(), 1);
        clear(&c, id).unwrap();
        assert!(load(&c, id).unwrap().is_empty());
    }
}
