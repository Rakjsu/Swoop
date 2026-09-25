//! Preparação antes de transferir: decide se dá para continuar o `.part`
//! anterior, escolhe nomes, reserva espaço e monta a tabela de segmentos.

use super::JobCtx;
use crate::disk;
use crate::error::TransferError;
use crate::probe::Probe;
use crate::segments::{Span, plan};
use crate::table::Table;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use swoop_core::filename::choose_file_name;
use swoop_core::{DownloadId, Resolved, file_kind};
use swoop_store::downloads::{DownloadRow, ProbeInfo};
use swoop_store::packages::PackageRow;
use swoop_store::{SegmentRow, StoreError, downloads, packages, segments};

/// Onde o arquivo está sendo gravado e como vai se chamar.
#[derive(Debug, Clone)]
pub struct Paths {
    pub part: PathBuf,
    pub dest_dir: PathBuf,
    pub file_name: String,
}

/// Pronto para transferir.
pub struct Prepared {
    pub file: File,
    pub table: Table,
    pub ranged: bool,
    pub workers: usize,
    pub paths: Paths,
}

/// Monta arquivo, tabela e nomes; persiste o que a sonda descobriu.
pub async fn prepare(
    ctx: &JobCtx,
    id: DownloadId,
    resolved: &Resolved,
    probe: &Probe,
) -> Result<Prepared, TransferError> {
    let (row, pkg, stored) = ctx
        .store
        .call(move |c| {
            let row = downloads::get(c, id)?.ok_or(StoreError::NotFound(id))?;
            let pkg = packages::get(c, row.package_id)?.ok_or(StoreError::NotFound(id))?;
            Ok((row, pkg, segments::load(c, id)?))
        })
        .await?;

    let size = probe.size.or(resolved.size);
    let ranged = probe.range_ok && resolved.resumable && size.is_some();
    let reuse = ranged && can_resume(&row, &stored, size, probe);
    let conns = ctx
        .settings
        .connections_per_download
        .min(resolved.max_connections)
        .min(ctx.granted_conns)
        .max(1);

    let (file_name, part, dest_dir) = match (reuse, &row.file_name, &row.part_path) {
        // Retomada: o arquivo termina na pasta onde começou.
        (true, Some(name), Some(part)) => {
            let dir = part
                .parent()
                .map_or_else(|| pkg.dest_dir.clone(), Path::to_path_buf);
            (name.clone(), part.clone(), dir)
        }
        _ => {
            if let Some(old) = &row.part_path {
                let _ = std::fs::remove_file(old);
            }
            let name = choose_file_name(
                resolved.file_name.as_deref(),
                probe.disposition_name.as_deref(),
                &resolved.url,
            );
            let dir = dest_dir_for(ctx, &pkg, &name);
            let part = disk::unique_path(&dir, &format!("{name}.part"));
            (name, part, dir)
        }
    };

    let needed = size.map(|s| s.saturating_sub(if reuse { row.done_bytes } else { 0 }));
    let file = {
        let (dir, part) = (dest_dir.clone(), part.clone());
        tokio::task::spawn_blocking(move || {
            if let Some(n) = needed {
                disk::check_space(&dir, n)?;
            }
            disk::open_part(&part, size, !reuse)
        })
        .await
        .map_err(|e| TransferError::Fatal(e.to_string()))?
        .map_err(|e| TransferError::Disk(e.to_string()))?
    };

    let table = match (reuse, size) {
        (true, _) => Table::from_rows(&stored),
        (false, Some(s)) if ranged => Table::from_spans(&plan(s, conns)),
        (false, s) => Table::from_spans(&[Span {
            start: 0,
            end: s.unwrap_or(u64::MAX),
        }]),
    };
    ctx.progress
        .total
        .store(size.unwrap_or(0), Ordering::Relaxed);

    let info = ProbeInfo {
        host_key: resolved.host_key.clone(),
        file_name: file_name.clone(),
        size,
        resumable: ranged,
        etag: probe.etag.clone(),
        last_modified: probe.last_modified.clone(),
        part_path: part.clone(),
    };
    let rows = table.rows();
    ctx.store
        .call(move |c| {
            if !reuse {
                segments::clear(c, id)?;
            }
            downloads::set_probe(c, id, &info)?;
            if ranged {
                segments::replace(c, id, &rows)?;
            }
            Ok(())
        })
        .await?;

    if reuse {
        tracing::info!(id = id.0, "retomando de {} bytes", row.done_bytes);
    }
    Ok(Prepared {
        file,
        table,
        ranged,
        workers: if ranged { usize::from(conns) } else { 1 },
        paths: Paths {
            part,
            dest_dir,
            file_name,
        },
    })
}

/// Pasta do arquivo: a do pacote, ou, com pasta automática, a do tipo do
/// arquivo (vídeos, músicas, downloads) pelas preferências.
fn dest_dir_for(ctx: &JobCtx, pkg: &PackageRow, file_name: &str) -> PathBuf {
    if !pkg.auto_dest {
        return pkg.dest_dir.clone();
    }
    ctx.settings
        .dir_for(file_kind(file_name))
        .map_or_else(|| pkg.dest_dir.clone(), Path::to_path_buf)
}

/// O `.part` anterior ainda vale? Mesmo tamanho, mesmos validadores e o
/// arquivo existe.
fn can_resume(row: &DownloadRow, stored: &[SegmentRow], size: Option<u64>, probe: &Probe) -> bool {
    let same = |old: &Option<String>, new: &Option<String>| match (old, new) {
        (Some(a), Some(b)) => a == b,
        _ => true,
    };
    row.resumable
        && !stored.is_empty()
        && row.file_name.is_some()
        && row.part_path.as_ref().is_some_and(|p| p.exists())
        && row.size == size
        && same(&row.etag, &probe.etag)
        && same(&row.last_modified, &probe.last_modified)
}
