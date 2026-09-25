//! A porta de entrada das interfaces. A janela Tauri e o painel web falam só
//! com esta trait; quem a implementa (o `swoop-service`) liga banco e motor.

use crate::{Command, DownloadView, HistoryView, Push, Reply, Snapshot};
use async_trait::async_trait;
use serde::Serialize;
use swoop_core::Settings;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, watch};
use ts_rs::TS;

/// Erro devolvido à interface, com mensagem pronta para o usuário.
/// No JSON: `{ "kind": "invalid", "message": "link inválido: …" }`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, TS)]
#[serde(tag = "kind", content = "message", rename_all = "snake_case")]
#[ts(export)]
pub enum ApiError {
    /// Pedido inválido (link malformado, id inexistente).
    #[error("{0}")]
    Invalid(String),
    /// O serviço não abriu ou já foi encerrado.
    #[error("{0}")]
    Unavailable(String),
    /// Falha interna (banco, disco).
    #[error("{0}")]
    Internal(String),
}

/// Assinatura de uma interface: retratos (≈5 Hz, só o mais recente importa)
/// e avisos discretos (`Push::Changed`). `Lagged` nos avisos = recarregar.
pub struct Subscription {
    pub snapshots: watch::Receiver<Snapshot>,
    pub pushes: broadcast::Receiver<Push>,
}

impl Subscription {
    /// Retrato atual (para a interface não começar vazia).
    pub fn current(&mut self) -> Push {
        Push::Snapshot(self.snapshots.borrow_and_update().clone())
    }

    /// Próximo aviso na ordem em que chega: retrato novo ou mudança na fila.
    /// `None` quando o backend encerrou.
    pub async fn next(&mut self) -> Option<Push> {
        let snapshot = tokio::select! {
            r = self.snapshots.changed() => r.is_ok(),
            r = self.pushes.recv() => return match r {
                Ok(push) => Some(push),
                Err(RecvError::Lagged(_)) => Some(Push::Changed),
                Err(RecvError::Closed) => None,
            },
        };
        snapshot.then(|| self.current())
    }
}

/// Operações que qualquer interface pode pedir.
#[async_trait]
pub trait Backend: Send + Sync {
    /// Executa um pedido da interface.
    async fn exec(&self, cmd: Command) -> Result<Reply, ApiError>;

    /// Todos os downloads na ordem da fila.
    async fn list(&self) -> Result<Vec<DownloadView>, ApiError>;

    /// As `limit` entradas mais recentes do histórico.
    async fn history(&self, limit: u32) -> Result<Vec<HistoryView>, ApiError>;

    /// Preferências em uso (pastas já preenchidas com as do sistema).
    async fn settings(&self) -> Result<Settings, ApiError>;

    /// Passa a receber retratos e avisos.
    fn subscribe(&self) -> Subscription;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn assinatura_entrega_retratos_e_avisos() {
        let (snap_tx, snapshots) = watch::channel(Snapshot::default());
        let (push_tx, pushes) = broadcast::channel(4);
        let mut sub = Subscription { snapshots, pushes };
        push_tx.send(Push::Changed).unwrap();
        assert_eq!(sub.next().await, Some(Push::Changed));
        snap_tx.send_replace(Snapshot {
            seq: 9,
            ..Snapshot::default()
        });
        match sub.next().await {
            Some(Push::Snapshot(s)) => assert_eq!(s.seq, 9),
            other => panic!("esperava retrato, veio {other:?}"),
        }
        // avisos perdidos viram "recarregue"
        for _ in 0..6 {
            push_tx.send(Push::Changed).unwrap();
        }
        assert_eq!(sub.next().await, Some(Push::Changed));
        drop(snap_tx);
        drop(push_tx);
        while let Some(p) = sub.next().await {
            assert_eq!(p, Push::Changed);
        }
    }

    #[test]
    fn erro_vira_json_com_tipo_e_mensagem() {
        let json = serde_json::to_value(ApiError::Invalid("link inválido: x".into())).unwrap();
        assert_eq!(json["kind"], "invalid");
        assert_eq!(json["message"], "link inválido: x");
    }
}
