//! LSP-клиент Anvil.
//!
//! * [`servers`] — декларативный реестр языковых серверов (TOML, как паки тулчейнов);
//! * [`client`] — процесс сервера, JSON-RPC поверх stdio, фоновый поток чтения;
//! * [`pos`] — перевод позиций между rope (символы) и LSP (UTF-8/16/32);
//! * [`snippet`], [`markdown`], [`uri`] — мелкие преобразования для интерфейса.
//!
//! Без tokio: один поток чтения на сервер и канал событий для интерфейса —
//! этого достаточно и не тянет асинхронный рантайм в бинарник.

pub mod client;
pub mod markdown;
pub mod pos;
pub mod servers;
pub mod snippet;
pub mod transport;
pub mod uri;

pub use client::{Client, Incoming};
pub use lsp_types;
pub use pos::Enc;
pub use servers::{ServerDef, ServerRegistry};
