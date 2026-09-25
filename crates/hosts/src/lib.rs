//! Plugins de servidores de hospedagem.
//!
//! - `Registry`: escolhe o plugin pelo link (ou trata como link direto) e é
//!   o `Resolver` que o motor usa;
//! - `rules`: regras em dados (`rules/hosts.toml`), com override do usuário;
//! - `detect`: acha links num texto;
//! - `dump`: grava as respostas lidas para virar fixture;
//! - plugins em `<servidor>/{mod.rs (rede), parse.rs (puro, testado com
//!   fixtures)}`.
//!
//! Nada aqui contorna espera, cota ou captcha: esses casos viram
//! `HostError::Wait` ou `HostError::BrowserRequired` com mensagem clara.

mod detect;
mod direct;
mod dump;
mod gdrive;
mod mediafire;
mod page;
mod pixeldrain;
mod plugin;
mod registry;
pub mod rules;
mod xfs;

pub use direct::DirectResolver;
pub use plugin::{FileInfo, HostCtx, HostPlugin};
pub use registry::{DIRECT, Registry};
pub use rules::Rules;
