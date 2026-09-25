//! Identificadores fortes (evitam trocar id de pacote por id de download).

use serde::{Deserialize, Serialize};
use std::fmt;

/// Id de um download (linha da tabela `downloads`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DownloadId(pub i64);

/// Id de um pacote (grupo de links com o mesmo destino).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PackageId(pub i64);

impl fmt::Display for DownloadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

impl fmt::Display for PackageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "pacote #{}", self.0)
    }
}
