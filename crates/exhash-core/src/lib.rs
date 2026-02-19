//! exhash — Verified Line-Addressed File Editor (core library)
//!
//! This crate provides the string-based editing engine and command parsing for the
//! `exhash` and `lnhashview` CLIs.

mod engine;
mod lnhash;
mod parse;

pub use engine::{edit_text, EditResult};
pub use lnhash::{format_lnhash, line_hash_u16, parse_lnhash, LnHash};
pub use parse::{parse_commands_from_args, parse_commands_from_script, parse_commands_from_strs, Command, Subcommand};

/// Library error type.
#[derive(Debug, Clone)]
pub struct EditError {
    msg: String,
}

impl EditError {
    pub(crate) fn new(msg: impl Into<String>) -> Self {
        Self { msg: msg.into() }
    }

    /// Human-friendly error message.
    pub fn message(&self) -> &str {
        &self.msg
    }
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl std::error::Error for EditError {}
