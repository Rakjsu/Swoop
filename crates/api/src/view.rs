//! O que a interface mostra: linhas da fila (do banco), o retrato ao vivo
//! dos downloads ativos (≈5 por segundo) e os avisos empurrados.

use serde::Serialize;
use swoop_core::DownloadState;
use ts_rs::TS;

/// Uma linha da lista de downloads, como está no banco.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct DownloadView {
    pub id: i64,
    pub url: String,
    pub file_name: Option<String>,
    pub state: DownloadState,
    pub size: Option<u64>,
    /// Bytes confirmados em disco no último checkpoint (o retrato ao vivo
    /// traz o valor atual dos ativos).
    pub done_bytes: u64,
    pub error: Option<String>,
    /// Até quando espera (ms Unix), quando `state` é `waiting`.
    pub wait_until_ms: Option<i64>,
    pub final_path: Option<String>,
}

/// Progresso de um download ativo.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct LiveRow {
    pub id: i64,
    pub received: u64,
    pub total: Option<u64>,
    pub speed_bps: u64,
    pub conns: u32,
    pub eta_secs: Option<u64>,
}

/// Retrato do motor: velocidade total, limite e os ativos.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Snapshot {
    pub seq: u64,
    pub speed_bps: u64,
    pub limit_bps: Option<u64>,
    pub active: Vec<LiveRow>,
}

/// O que o backend empurra para a interface.
/// No JSON: `{ "type": "snapshot", "seq": …, … }` ou `{ "type": "changed" }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum Push {
    Snapshot(Snapshot),
    /// A fila mudou (estado, item novo ou removido): recarregar a lista.
    Changed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn push_achata_o_retrato_com_o_tipo() {
        let json = serde_json::to_value(Push::Snapshot(Snapshot {
            seq: 3,
            ..Snapshot::default()
        }))
        .unwrap();
        assert_eq!(json["type"], "snapshot");
        assert_eq!(json["seq"], 3);
        assert_eq!(
            serde_json::to_value(Push::Changed).unwrap(),
            json!({ "type": "changed" })
        );
    }

    #[test]
    fn estado_sai_em_snake_case() {
        let view = DownloadView {
            id: 1,
            url: "http://a/b".into(),
            file_name: None,
            state: DownloadState::Downloading,
            size: None,
            done_bytes: 0,
            error: None,
            wait_until_ms: None,
            final_path: None,
        };
        assert_eq!(serde_json::to_value(view).unwrap()["state"], "downloading");
    }
}
