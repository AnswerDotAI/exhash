//! exhash — Verified Line-Addressed File Editor
//!
//! This crate provides the string-based editing engine and command parsing for the
//! `exhash` and `lnhashview` CLIs.

mod engine;
mod lnhash;
mod outline;
mod parse;

mod python;

pub use engine::{BufferCommand, BufferEditResult, EditResult, edit_buffers_with_sw, edit_text, edit_text_with_sw};
pub use lnhash::{LnHash, format_lnhash, line_hash_u16, lnhashview, parse_lnhash};
pub use outline::{HeadingRow, LinkRow, scan_code, scan_md};
pub use parse::{Address, Command, Subcommand, parse_commands_from_args, parse_commands_from_script};

#[derive(Debug, Clone)]
pub struct EditError { msg: String }

impl EditError {
    pub(crate) fn new(msg: impl Into<String>) -> Self { Self { msg: msg.into() } }

    pub fn message(&self) -> &str { &self.msg }
}

impl std::fmt::Display for EditError { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.msg) } }

impl std::error::Error for EditError {}
