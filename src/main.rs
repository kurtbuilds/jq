#![allow(unused)]
mod exit_status;

use std::borrow::Cow;
pub use exit_status::ExitOk;

use clap::{Parser, Subcommand, ValueEnum};
use crate::Command::{Extract, Keys};
use anyhow::{Result, anyhow};
use serde_json::Value;


#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    command: Vec<String>,
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
}

struct Options {
    pretty: bool,
}

fn evaluate_command(s: &str) -> Result<Command> {
    let c = if s.starts_with(".") {
        Extract { path: s[1..].to_string() }
    } else if s.starts_with("keys") {
        Keys
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
    let mut path = s.splitn(2, '.');
    let key = path.next().ok_or(anyhow!("Invalid path"))?;
    let rest = path.next().unwrap_or("");
    Ok((key, rest))
}

fn extract(obj: Value, path: &str) -> Result<Value> {
    match (obj, path) {
        (obj, path) if path.is_empty() => {
            Ok(obj)
        }
        (Value::Object(mut obj), path) => {
            let (key, rest) = next_key(path)?;
            let item = obj.get_mut(key).ok_or(anyhow!("No such key: {}", key))?.take();
            extract(item, rest)
        }
        (Value::Array(mut arr), path ) if path.starts_with("[") => {
            let (key, rest) = next_key(path)?;
            if key == "[]" {
                let vec = arr.into_iter().map(|v| extract(v, rest)).collect::<Result<Vec<_>,_>>()?;
                Ok(Value::Array(vec))
            } else {
                let index = key[1..key.len() - 1].parse::<usize>()?;
                let item = arr.get_mut(index).ok_or(anyhow!("No such index: {}", index))?.take();
                extract(item, rest)
            }
        }
        _ => {
            Err(anyhow!("Invalid path"))
        }
    }
}


fn extract_by_ref<'a>(obj: &'a Value, path: &str) -> Result<&'a Value> {
    match (obj, path) {
        (_, path) if path.is_empty() => {
            Ok(obj)
        }
        (Value::Object(obj), path) => {
            let (key, rest) = next_key(path)?;
            let item = obj.get(key).ok_or(anyhow!("No such key: {}", key))?;
            extract_by_ref(&item, rest)
        }
        (Value::Array(arr), path ) if path.starts_with("[") => {
            let (key, rest) = next_key(path)?;
            if path == "[]" {
                Err(anyhow!("Cannot extract array of arrays"))
            } else {
                let index = path[1..path.len() - 1].parse::<usize>()?;
                let item = arr.get(index).ok_or(anyhow!("Index out of bounds"))?;
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
            let obj = extract(obj, path)?;
            if obj.as_str().is_some() {
                println!("{}", obj.as_str().unwrap());
            } else if option.pretty {
                println!("{}", serde_json::to_string_pretty(&obj)?);
            } else {
                println!("{}", serde_json::to_string(&obj)?);
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
                let values = values.into_iter()
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
        _ => {
            Err(anyhow!("Tried to apply command: {:?} to object: {:?}", command, obj))
        }
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let command = cli.command.join(" ");
    let command = evaluate_command(&command)?;
    let options = Options { pretty: true };

    let stdin = std::io::stdin();
    let stdin = stdin.lock();

    let deserializer = serde_json::Deserializer::from_reader(stdin);
    let iterator = deserializer.into_iter::<serde_json::Value>();

    for item in iterator {
        apply_command(item?, &command, &options)?;
    }
    Ok(())
}