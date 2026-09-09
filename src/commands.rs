//! Structured commands shared by language bindings.
use crate::parse::{
    command_from_parts, parse_buffer_destination_address, parse_destination_address, parse_optional_usize, split_text_payload, subst_from_parts,
    translit_from_parts,
};
use crate::{Command, EditError, Subcommand};

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum CommandField { Str(String), Seq(Vec<CommandField>) }

pub fn command_from_fields(fields: &[CommandField]) -> Result<Command, EditError> {
    let [CommandField::Str(addr), CommandField::Str(op), rest @ ..] = fields else {
        return Err(EditError::new("command must start with (address, op) strings"));
    };
    command_from_parts(addr, subcommand_from_fields(op, rest)?)
}

pub fn buffer_command_from_fields(fields: &[CommandField]) -> Result<Command, EditError> {
    let [CommandField::Str(addr), CommandField::Str(op), CommandField::Str(dest)] = fields else { return command_from_fields(fields); };
    if !matches!(op.as_str(), "m" | "t") { return command_from_fields(fields); }
    let op_char = if op == "m" { 'm' } else { 't' };
    let dest = parse_buffer_destination_address(dest, op_char)?;
    let sub = if op == "m" { Subcommand::Move { dest } } else { Subcommand::Copy { dest } };
    command_from_parts(addr, sub)
}

fn str_fields<'a>(op: &str, fields: &'a [CommandField]) -> Result<Vec<&'a str>, EditError> {
    fields
        .iter()
        .map(|f| match f { CommandField::Str(s) => Ok(s.as_str()), CommandField::Seq(_) => Err(EditError::new(format!("{op} fields must be strings"))) })
        .collect()
}

fn subcommand_from_fields(op: &str, fields: &[CommandField]) -> Result<Subcommand, EditError> {
    if let "g" | "g!" | "v" = op {
        let [CommandField::Str(pattern), CommandField::Seq(inner)] = fields else {
            return Err(EditError::new(format!("{op} takes (pattern, (subcommand, ...))")));
        };
        let [CommandField::Str(iop), irest @ ..] = inner.as_slice() else { return Err(EditError::new("global subcommand must start with an op string")); };
        if matches!(iop.as_str(), "g" | "g!" | "v") { return Err(EditError::new("global commands cannot nest")); }
        return Ok(Subcommand::Global { invert: op != "g", pattern: pattern.clone(), cmd: Box::new(subcommand_from_fields(iop, irest)?) });
    }
    let f = str_fields(op, fields)?;
    match (op, f.as_slice()) {
        ("d", []) => Ok(Subcommand::Delete),
        ("p", []) => Ok(Subcommand::Print),
        ("j", []) => Ok(Subcommand::Join),
        ("sort", []) => Ok(Subcommand::Sort),
        ("a", [text]) => Ok(Subcommand::Append(split_text_payload(text))),
        ("i", [text]) => Ok(Subcommand::Insert(split_text_payload(text))),
        ("c", [text]) => Ok(Subcommand::Change(split_text_payload(text))),
        ("s", [pat, rep]) => Ok(Subcommand::Substitute(subst_from_parts((*pat).into(), (*rep).into(), "")?)),
        ("s", [pat, rep, flags]) => Ok(Subcommand::Substitute(subst_from_parts((*pat).into(), (*rep).into(), flags)?)),
        ("y", [source, dest]) => {
            let (source, dest) = translit_from_parts((*source).into(), (*dest).into())?;
            Ok(Subcommand::Transliterate { source, dest })
        }
        ("m", [dest]) => Ok(Subcommand::Move { dest: parse_destination_address(dest, 'm')? }),
        ("t", [dest]) => Ok(Subcommand::Copy { dest: parse_destination_address(dest, 't')? }),
        (">", rest @ ([] | [_])) => Ok(Subcommand::Indent { levels: parse_optional_usize(rest.first().copied().unwrap_or(""))? }),
        ("<", rest @ ([] | [_])) => Ok(Subcommand::Dedent { levels: parse_optional_usize(rest.first().copied().unwrap_or(""))? }),
        _ => Err(EditError::new(format!("invalid tuple command: {op:?} with {} field(s)", f.len()))),
    }
}
