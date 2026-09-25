//! Plugins de servidores de hospedagem.
//!
//! Na fase 1 só existe `DirectResolver` (links HTTP/HTTPS diretos). O trait
//! `HostPlugin`, o registro e os plugins de Pixeldrain, Mediafire e Google
//! Drive entram na fase 3; o registro também implementará `Resolver`.

mod direct;

pub use direct::DirectResolver;
