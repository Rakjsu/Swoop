//! Versão `maior.menor.correção` (a tag `v0.1.2` ou o `CARGO_PKG_VERSION`).

use std::fmt;
use std::str::FromStr;

/// Versão estável; sufixos de pré-release (`-beta`) são recusados.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl FromStr for Version {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let raw = s.trim();
        let core = raw.strip_prefix('v').unwrap_or(raw);
        let mut parts = core.split('.');
        let mut next = || -> Result<u64, String> {
            parts
                .next()
                .and_then(|p| p.parse().ok())
                .ok_or_else(|| format!("versão inválida: {raw:?}"))
        };
        let v = Self {
            major: next()?,
            minor: next()?,
            patch: next()?,
        };
        if parts.next().is_some() {
            return Err(format!("versão inválida: {raw:?}"));
        }
        Ok(v)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_e_compara() {
        let a: Version = "v0.1.9".parse().unwrap();
        let b: Version = "0.1.10".parse().unwrap();
        assert!(b > a);
        assert_eq!(a.to_string(), "0.1.9");
        assert!("1.0".parse::<Version>().is_err());
        assert!("1.0.0-beta".parse::<Version>().is_err());
        assert!("1.0.0.1".parse::<Version>().is_err());
    }
}
