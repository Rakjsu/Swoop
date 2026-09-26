//! O que a interface mostra: linhas da fila (do banco), o retrato ao vivo
//! dos downloads ativos (≈5 por segundo) e os avisos empurrados.

use serde::Serialize;
use swoop_core::{DownloadState, LinkState};
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
    /// Pacote do download (a lista agrupa por ele).
    pub package_id: i64,
    pub package_name: String,
    /// Captcha esperando o usuário (estado `captcha_needed`).
    pub captcha: Option<CaptchaInfo>,
}

/// Captcha pendente, para a interface mostrar e oferecer "Resolver".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct CaptchaInfo {
    /// Site que pediu.
    pub host: String,
    /// Fim do contador do site (ms Unix); antes disso o envio espera.
    pub not_before_ms: i64,
}

/// Um link no coletor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct CollectorView {
    pub id: i64,
    pub url: String,
    /// Servidor (plugin ou domínio).
    pub host: String,
    pub state: LinkState,
    pub file_name: Option<String>,
    pub size: Option<u64>,
    /// Motivo de offline/falha.
    pub error: Option<String>,
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
/// No JSON: `{ "type": "snapshot", "seq": …, … }`, `{ "type": "changed" }` ou
/// `{ "type": "notice", "kind": "queue_finished", … }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum Push {
    Snapshot(Snapshot),
    /// A fila mudou (estado, item novo ou removido): recarregar a lista.
    Changed,
    /// Algo que merece um aviso do sistema.
    Notice(Notice),
}

/// Avisos para o usuário (o app desktop vira notificação do Windows).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum Notice {
    /// Nada mais baixando nem esperando; contagem desde o último aviso.
    QueueFinished { completed: u32, failed: u32 },
    /// Um download falhou de vez (esgotou as tentativas ou erro sem volta).
    Failed { name: String, message: String },
    /// Um servidor pediu captcha: o usuário resolve no app.
    CaptchaNeeded { name: String, host: String },
}

/// Como um item saiu da fila.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum HistoryOutcome {
    Completed,
    Failed,
    Removed,
}

/// Uma entrada do histórico (o mais novo primeiro).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct HistoryView {
    pub id: i64,
    pub url: String,
    pub file_name: Option<String>,
    pub size: Option<u64>,
    pub final_path: Option<String>,
    pub outcome: HistoryOutcome,
    pub error: Option<String>,
    /// Quando saiu da fila (ms Unix).
    pub finished_ms: i64,
    /// Velocidade média em bytes/s.
    pub avg_bps: Option<u64>,
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
        assert_eq!(
            serde_json::to_value(Push::Notice(Notice::QueueFinished {
                completed: 2,
                failed: 0
            }))
            .unwrap(),
            json!({ "type": "notice", "kind": "queue_finished", "completed": 2, "failed": 0 })
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
            package_id: 1,
            package_name: "p".into(),
            captcha: None,
        };
        assert_eq!(serde_json::to_value(view).unwrap()["state"], "downloading");
    }
}
