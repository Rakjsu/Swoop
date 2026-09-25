//! Motor de download do Swoop.
//!
//! Fluxo de um download: agendador escolhe → `job` resolve e sonda →
//! `prepare` monta o `.part` e a tabela de segmentos → `transfer` roda N
//! conexões (`worker`) e a thread escritora (`writer`) → conferência,
//! renomeação e histórico. O progresso sai em `Snapshot` (≈ 5 Hz) e os
//! marcos em `EngineEvent`.
//!
//! O motor não conhece servidores: recebe um `Resolver` (plugins em
//! `swoop-hosts`) e só fala HTTP.

mod disk;
mod engine;
mod error;
mod job;
mod probe;
mod progress;
mod request;
mod scheduler;
pub mod segments;
mod table;
mod transfer;
mod verify;
mod worker;
mod writer;

pub use engine::{Engine, EngineConfig};
pub use error::TransferError;
pub use job::EngineEvent;
pub use progress::{LiveRow, Snapshot};
