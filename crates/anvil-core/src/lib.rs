//! Ядро редактора Anvil — без интерфейса, полностью покрыто тестами.
//!
//! * [`Buffer`] — rope (`ropey`), нормализация переводов строк, сохранение;
//! * [`History`] — дерево undo/redo со склейкой ввода;
//! * [`Selections`] — мультикурсоры;
//! * [`Editor`] — все операции редактирования;
//! * [`search`] — поиск и замена, в том числе по regex.

pub mod buffer;
pub mod editor;
pub mod history;
pub mod search;
pub mod selection;
pub mod text;

pub use buffer::{Buffer, LineEnding};
pub use editor::{EditConfig, EditEvent, Editor, Move};
pub use history::{Change, EditKind, History, Transaction};
pub use ropey::Rope;
pub use search::SearchOptions;
pub use selection::{Selection, Selections};
