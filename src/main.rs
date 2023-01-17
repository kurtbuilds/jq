#![allow(unused)]
mod exit_status;

use std::borrow::Cow;
use std::io::{stdout, Write};
use std::iter::once;
use std::ops::Index;
pub use exit_status::ExitOk;

use clap::{Parser, Subcommand, ValueEnum, Args};
use crate::Command::{Extract, Keys};
use anyhow::{Result, anyhow};
use colored_json::ToColoredJson;
use regex::Regex;
use serde::de::Error;
use serde::Deserialize;
use serde_json::Value;

#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    command: Vec<String>,
    #[clap(short, long)]
    yaml: bool,
}

#[derive(Debug)]
enum Command {
    Extract{
        path: String,
    },
    Csv {
        keys: Vec<String>
    },
    Keys,
    Len,
}

struct Options {
    pretty: bool,
}

fn evaluate_command(s: &str) -> Result<Command> {
    let c = if s.starts_with(".") {
        Extract { path: s[1..].to_string() }
    } else if s.starts_with("keys") {
        Keys
    } else if s.starts_with("len") {
        Command::Len
    } else if s.starts_with("csv") {
        let mut keys = s.split_whitespace().skip(1)
            .filter(|s| !s.is_empty())
            .map(|s| s.trim_matches(','))
            .map(|s| s.to_string())
            .collect::<Vec<String>>();
        Command::Csv { keys }
    } else {
        Extract { path: s.to_string() }
    };
    Ok(c)
}


fn next_key(s: &str) -> Result<(&str, &str)> {
    if s == "" {
        return Ok(("", ""));
    }
    let idx = s.find(|c| c == '.' || c == '[');
    let Some(idx) = idx else {
        return Ok((s, ""));
    };
    match s.chars().nth(idx).unwrap() {
        '.' => {
            let (key, rest) = s.split_at(idx);
            Ok((key, &rest[1..]))
        }
        '[' => {
            if idx == 0 {
                match s.find('.') {
                    None => Ok((s, "")),
                    Some(idx)  => {
                        let (key, rest) = s.split_at(idx);
                        Ok((key, &rest[1..]))

                    }
                }
            } else {
                Ok(s.split_at(idx))
            }
        }
        _ => unreachable!(),
    }
}

fn extract_by_ref<'a>(obj: &'a Value, path: &str) -> Result<Box<dyn Iterator<Item=&'a Value> + 'a>> {
    match (obj, path) {
        (_, path) if path.is_empty() => {
            Ok(Box::new(once(obj)))
        }
        (Value::Object(obj), path) => {
            let (key, rest) = next_key(path)?;
            let item = obj.get(key).ok_or(anyhow!("No such key: {}", key))?;
            extract_by_ref(&item, rest)
        }
        (Value::Array(arr), path ) if path.starts_with("[") => {
            let (key, rest) = next_key(path)?;
            if key == "[]" {
                let vec = arr.iter()
                    .map(|v| extract_by_ref(v, rest))
                    .collect::<Result<Vec<_>,_>>()?;
                Ok(Box::new(vec.into_iter().flatten()))
            } else {
                let mut index = key[1..key.len() - 1].parse::<i64>()?;
                if index < 0 {
                    index = arr.len() as i64 + index;
                }
                let item = arr.get(index as usize).ok_or(anyhow!("Index out of bounds"))?;
                extract_by_ref(item, rest)
            }
        }
        _ => {
            Err(anyhow!("Invalid path"))
        }
    }
}

fn apply_command(obj: Value, command: &Command, option: &Options) -> Result<()> {
    match command {
        Extract { path } => {
            let obj = extract_by_ref(&obj, path)?;
            for item in obj {
                if let Some(s) = item.as_str() {
                    println!("{}", s);
                } else if option.pretty {
                    let out = stdout();
                    {
                        let mut out = out.lock();
                        colored_json::write_colored_json(item, &mut out)?;
                        write!(out, "\n")?;
                        out.flush()?;
                    }
                } else {
                    println!("{}", serde_json::to_string(item)?);
                }
            }
            Ok(())
        }
        Command::Csv { keys } => {
            let mut csv = csv::Writer::from_writer(std::io::stdout());
            csv.write_record(keys)?;
            let arr = obj.as_array().ok_or(anyhow!("Not an array"))?;
            for item in arr.into_iter() {
                let values = keys.iter()
                    .map(|key| extract_by_ref(&item, key))
                    .collect::<Result<Vec<_>>>()?;
                let values = values.into_iter().flatten()
                    .map(|v| {
                        match v {
                            Value::String(s) => s.clone(),
                            _ => v.to_string(),
                        }
                    })
                    .collect::<Vec<_>>();
                csv.write_record(values)?;
            }
            Ok(())
        }
        Keys => {
            let obj = obj.as_object().ok_or(anyhow!("Not an object"))?;
            for key in obj.keys() {
                println!("{}", key);
            }
            Ok(())
        }
        Command::Len => {
            let len = match obj {
                Value::Array(arr) => arr.len(),
                Value::Object(obj) => obj.len(),
                _ => return Err(anyhow!("Not an array or object")),
            };
            println!("{}", len);
            Ok(())
        }
        _ => {
            Err(anyhow!("Tried to apply command: {:?} to object: {:?}", command, obj))
        }
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if atty::is(atty::Stream::Stdin) {
        eprintln!("Input data must be piped to stdin.");
        std::process::exit(1);
    }

    let command = cli.command.join(" ");
    let command = evaluate_command(&command)?;
    let options = Options { pretty: true };

    let stdin = std::io::stdin();
    let stdin = stdin.lock();

    if cli.yaml {
        let deserializer = serde_yaml::Deserializer::from_reader(stdin);
        for obj in deserializer.into_iter() {
            let obj = Value::deserialize(obj)?;
            apply_command(obj, &command, &options)?;
        }
    } else {
        let deserializer = serde_json::Deserializer::from_reader(stdin);
        for obj in deserializer.into_iter::<Value>() {
            let obj = obj?;
            apply_command(obj, &command, &options)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn test_next_key() -> Result<()> {
        assert_eq!(next_key("foo.bar")?, ("foo", "bar"));
        assert_eq!(next_key("foo.[]")?, ("foo", "[]"));
        assert_eq!(next_key("foo[]")?, ("foo", "[]"));
        assert_eq!(next_key("[].foo.baz")?, ("[]", "foo.baz"));
        assert_eq!(next_key("[0].foo")?, ("[0]", "foo"));
        assert_eq!(next_key("[0].response.data.list")?, ("[0]", "response.data.list"));
        assert_eq!(next_key("")?, ("", ""));
        Ok(())
    }
}
