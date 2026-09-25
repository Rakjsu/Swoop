//! Por que uma transferência parou (decide o próximo estado do download).

use std::time::SystemTime;
use swoop_core::WaitReason;

/// Motivo de parada de uma transferência.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransferError {
    /// Pausa ou encerramento: sair quieto, o estado já foi decidido fora.
    #[error("cancelado")]
    Cancelled,
    /// A URL expirou ou foi negada: resolver de novo.
    #[error("link expirado; resolvendo de novo")]
    Reresolve,
    /// O servidor pediu espera.
    #[error("{message}")]
    Wait {
        until: SystemTime,
        reason: WaitReason,
        message: String,
    },
    /// Falhas temporárias esgotaram as tentativas.
    #[error("{0}")]
    Transient(String),
    /// Erro sem recuperação automática.
    #[error("{0}")]
    Fatal(String),
    /// O arquivo no servidor mudou (ETag/tamanho/If-Range): recomeçar do zero.
    #[error("o arquivo mudou no servidor")]
    FileChanged,
    /// Falha de disco (sem espaço, permissão, arquivo travado).
    #[error("erro de disco: {0}")]
    Disk(String),
}

impl From<swoop_store::StoreError> for TransferError {
    /// Transição recusada ou download sumido = outro ator (pausa, remoção)
    /// mudou o estado: o job sai quieto. O resto é falha do banco.
    fn from(e: swoop_store::StoreError) -> Self {
        match e {
            swoop_store::StoreError::Transition(_) | swoop_store::StoreError::NotFound(_) => {
                Self::Cancelled
            }
            e => Self::Fatal(format!("banco de dados: {e}")),
        }
    }
}
