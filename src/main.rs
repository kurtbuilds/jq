#![allow(unused)]
mod exit_status;

use std::borrow::Cow;
use std::fs::File;
use std::io;
use std::io::{stdout, IsTerminal, Read, Write};
use std::iter::{empty, once};
use std::ops::Index;
pub use exit_status::ExitOk;

use clap::{Parser, Subcommand, ValueEnum, Args};
use anyhow::{Result, anyhow};
use colored_json::ToColoredJson;
use regex::Regex;
use serde::de::Error;
use serde::{Deserialize, Deserializer};
use serde_json::Value;


#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    command: Vec<String>,

    /// Parse the input as YAML
    #[clap(short, long)]
    yaml: bool,

    /// Parse the input as YAML
    #[clap(short = 'Y', long)]
    yaml_output: bool,

    /// Parse the input as YAML
    #[clap(short = 'J', long)]
    json_output: bool,

    #[clap(short, long)]
    raw: bool,
}

#[derive(Debug, PartialEq)]
enum StreamCommand {
    Key(String),
    Index(usize),
    Range(Option<i64>, Option<i64>),
    Filter(String),
    Put(String, String),
    Delete(String),
}

#[derive(Debug, PartialEq)]
enum PrintCommand {
    Yaml,
    Pretty,
    Json,
    Keys,
    Len,
    Csv(Vec<(String, String)>),
}

fn split_headers(s: &str) -> Vec<(String, String)> {
    s.split([',', '\u{29}'])
        .map(|s| s.split_once('=').unwrap_or_else(|| {
            let h = s.rsplit_once([']', '.']).map(|s| s.1).unwrap_or(s);
            (s, h)
        }))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

/// a[a=5,b=3]
/// the
fn evaluate_command(mut s: &str) -> (Vec<StreamCommand>, PrintCommand) {
    // s is a comma separated list of commands that operate on json objects
    // commands is a list of stream commands, and the final command is a print command
    // stream commands are filter, select, put, delete
    // print commands are json, pretty, yaml, keys, len, csv
    // tokenize the input and then parse it.
    // here are some examples to help you
    // a.b.c -> select a -> select b -> select c -> (default of print json)
    // a[b=5].c -> select a -> filter b=5 -> select c -> (default of print json)
    let mut commands = Vec::new();
    static TOKENS: &[char] = &[',', '.', '[', ']', '\u{29}'];
    static DIGITS: &[char] = &['0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '-'];
    while !s.is_empty() {
        if s.starts_with([']', ',', '\u{29}', ' ']) {
            s = &s[1..];
        } else if s.starts_with("..") {
            let end = s[2..].parse().unwrap();
            commands.push(StreamCommand::Range(None, Some(end)));
            s = &s[2 + end.to_string().len()..];
        } else if s.starts_with('.') {
            s = &s[1..];
            let tok = s.split(TOKENS).next().unwrap_or(s);
            if tok.is_empty() {
                continue;
            }
            commands.push(StreamCommand::Key(tok.to_string()));
            s = &s[tok.len()..];
        } else if s.starts_with("keys") {
            return (commands, PrintCommand::Keys);
        } else if s.starts_with("len") {
            return (commands, PrintCommand::Len);
        } else if s.starts_with("csv") {
            let mut keys = split_headers(&s[4..]);
            return (commands, PrintCommand::Csv(keys));
        } else if s.starts_with("put") {
            s = &s[4..];
            let put = s.split(',').next().unwrap_or(s);
            for kv in put.split('\u{29}') {
                let Some((k, v)) = kv.split_once('=') else {
                    panic!("Invalid put command: {}", kv);
                };
                commands.push(StreamCommand::Put(k.to_string(), v.to_string()));
            }
            s = &s[put.len()..];
        } else if s.starts_with(DIGITS) {
            let mut tok = s.split(TOKENS).next().unwrap_or(s);
            if s[tok.len()..].starts_with("..") {
                let first_token = tok;
                let start = tok.parse().unwrap();
                tok = &s[tok.len() + 2..];
                let tok = tok.split(TOKENS).next().unwrap_or(tok);
                let end = tok.parse().ok();
                // its a range
                commands.push(StreamCommand::Range(Some(start), end));
                s = &s[first_token.len() + 2 + tok.len()..];
            } else {
                commands.push(StreamCommand::Index(tok.parse().unwrap()));
                s = &s[tok.len()..];
            }
        } else if s.starts_with('[') {
            s = &s[1..];
            let filter = s.split(']').next().unwrap_or(s);
            if filter.is_empty() {
                commands.push(StreamCommand::Range(None, None));
            } else if filter.starts_with(DIGITS) {
                if let Some((start, end)) = filter.split_once("..") {
                    dbg!(start, end);
                    let start = start.parse().unwrap();
                    let end = end.parse().ok();
                    commands.push(StreamCommand::Range(Some(start), end));
                } else {
                    let index = filter.parse().unwrap();
                    commands.push(StreamCommand::Index(index));
                }
            } else if filter.starts_with("..") {
                let end = filter[2..].parse().unwrap();
                commands.push(StreamCommand::Range(None, Some(end)));
            } else {
                for f in filter.split([',', '\u{29}']) {
                    commands.push(StreamCommand::Filter(f.to_string()));
                }
            }
            s = &s[filter.len()..];
        } else if s.starts_with("delete") {
            s = &s[7..];
            let delete = s.split(',').next().unwrap_or(s);
            for key in delete.split('\u{29}') {
                commands.push(StreamCommand::Delete(key.to_string()));
            }
            s = &s[delete.len()..];

        } else {
            let tok = s.split(TOKENS).next().unwrap_or(s);
            commands.push(StreamCommand::Key(tok.to_string()));
            s = &s[tok.len()..];
        }
    }
    (commands, PrintCommand::Pretty)
}


// fn next_key(s: &str) -> Result<(&str, &str)> {
//     if s == "" {
//         return Ok(("", ""));
//     }
//     let Some(idx) = s.find(['.', '[']) else {
//         return Ok((s, ""));
//     };
//     match s.chars().nth(idx).unwrap() {
//         '.' => {
//             let (key, rest) = s.split_at(idx);
//             Ok((key, &rest[1..]))
//         }
//         '[' => {
//             if idx == 0 {
//                 match s.find('.') {
//                     None => Ok((s, "")),
//                     Some(idx) => {
//                         let (key, rest) = s.split_at(idx);
//                         Ok((key, &rest[1..]))
//                     }
//                 }
//             } else {
//                 Ok(s.split_at(idx))
//             }
//         }
//         _ => unreachable!(),
//     }
// }
//
// fn extract_by_ref<'a>(obj: &'a Value, path: &str) -> Result<Box<dyn Iterator<Item=&'a Value> + 'a>> {
//     match (obj, path) {
//         (_, path) if path.is_empty() => {
//             Ok(Box::new(once(obj)))
//         }
//         (Value::Object(obj), path) => {
//             let (key, rest) = next_key(path)?;
//             let item = obj.get(key).ok_or(anyhow!("No such key: {}", key))?;
//             extract_by_ref(&item, rest)
//         }
//         (Value::Array(arr), path) if path.starts_with("[") => {
//             let (key, rest) = next_key(path)?;
//             if key == "[]" {
//                 let vec = arr.iter()
//                     .map(|v| extract_by_ref(v, rest))
//                     .collect::<Result<Vec<_>, _>>()?;
//                 Ok(Box::new(vec.into_iter().flatten()))
//             } else {
//                 let mut index = key[1..key.len() - 1].parse::<i64>()?;
//                 if index < 0 {
//                     index = arr.len() as i64 + index;
//                 }
//                 let item = arr.get(index as usize).ok_or(anyhow!("Index out of bounds"))?;
//                 extract_by_ref(item, rest)
//             }
//         }
//         _ => {
//             Err(anyhow!("Invalid path"))
//         }
//     }
// }

fn parse_json(s: &str) -> Value {
    serde_json::from_str(s).unwrap_or(Value::String(s.to_string()))
}

fn equal(value: &Value, other: &str) -> bool {
    match value {
        Value::String(s) => s == other,
        Value::Number(n) => n.to_string() == other,
        Value::Bool(b) => b.to_string() == other,
        Value::Null => other == "null",
        _ => false,
    }
}

fn normalize(n: i64, arr: &Vec<Value>) -> usize {
    (if n < 0 {
        arr.len() as i64 + n
    } else {
        n
    }) as usize
}

fn apply_stream(mut obj: Value, mut stream_command: &[StreamCommand]) -> Box<dyn Iterator<Item=Value> + '_> {
    while !stream_command.is_empty() {
        let command = &stream_command[0];
        stream_command = &stream_command[1..];
        match command {
            StreamCommand::Key(s) => {
                let Value::Object(mut o) = obj else {
                    panic!("Expected object when using key {}, encountered: {:?}", s, obj);
                };
                obj = o.remove(s).unwrap_or(Value::Null);
            }
            StreamCommand::Filter(f) => {
                // a=5, a=b
                // a like foo
                // a > 5
                // > 5
                match obj {
                    Value::Array(arr) => {
                        let Some((key, value)) = f.split_once('=') else {
                            panic!("Invalid filter: {}", f);
                        };
                        let it = arr
                            .into_iter()
                            .filter_map(move |v| {
                                let Value::Object(mut o) = v else {
                                    return None;
                                };
                                let Some(v) = o.remove(key) else {
                                    return None;
                                };
                                Some(v).filter(|v| equal(&v, value))
                            })
                            .flat_map(|v| apply_stream(v, stream_command));
                        return Box::new(it);
                    }
                    Value::Object(o) => {
                        let Some((key, value)) = f.split_once('=') else {
                            panic!("Invalid filter: {}", f);
                        };
                        let Some(v) = o.get(key) else {
                            if value == "null" {
                                obj = Value::Object(o);
                                continue;
                            } else {
                                return Box::new(empty());
                            }
                        };
                        if equal(v, value) {
                            obj = Value::Object(o);
                            continue;
                        } else {
                            return Box::new(empty());
                        }
                    }
                    _ => {
                        panic!("Expected array or object when using filter {}, encountered: {:?}", f, obj);
                    }
                }
            }
            StreamCommand::Put(k, v) => {
                let Value::Object(mut o) = obj else {
                    panic!("Expected object when using key {}, encountered: {:?}", k, obj);
                };
                o.insert(k.clone(), parse_json(v));
                obj = Value::Object(o);
            }
            StreamCommand::Delete(d) => {
                let Value::Object(mut o) = obj else {
                    panic!("Expected object when using key {}, encountered: {:?}", d, obj);
                };
                o.remove(d);
                obj = Value::Object(o);
            }
            &StreamCommand::Index(i) => {
                let Value::Array(mut arr) = obj else {
                    panic!("Expected array when using index {}, encountered: {:?}", i, obj);
                };
                obj = arr.remove(i);
            }
            &StreamCommand::Range(start, end) => {
                let Value::Array(mut arr) = obj else {
                    panic!("Expected array when using range {:?}..{:?}, encountered: {:?}", start, end, obj);
                };
                return match (start, end) {
                    (Some(start), Some(end)) => {
                        let start = normalize(start, &arr);
                        let end = normalize(end, &arr);
                        Box::new(arr.into_iter().skip(start).take(end - start).flat_map(|v| apply_stream(v, stream_command)))
                    }
                    (Some(start), None) => {
                        let start = normalize(start, &arr);
                        Box::new(arr.into_iter().skip(start).flat_map(|v| apply_stream(v, stream_command)))
                    }
                    (None, Some(end)) => {
                        let end = normalize(end, &arr);
                        Box::new(arr.into_iter().take(end).flat_map(|v| apply_stream(v, stream_command)))
                    }
                    (None, None) => {
                        Box::new(arr.into_iter().flat_map(|v| apply_stream(v, stream_command)))
                    }
                };
            }
        }
    }
    Box::new(once(obj))
}

fn apply_print(obj: Value, print: &PrintCommand) {
    match print {
        PrintCommand::Yaml => {
            println!("{}", serde_yaml::to_string(&obj).unwrap());
        }
        PrintCommand::Json => {
            println!("{}", obj);
        }
        PrintCommand::Pretty => {
            if let Some(s) = obj.as_str() {
                println!("{}", s);
            } else {
                let out = stdout();
                {
                    let mut out = out.lock();
                    colored_json::write_colored_json(&obj, &mut out).unwrap();
                    write!(out, "\n").unwrap();
                    out.flush().unwrap();
                }
            }
        }
        PrintCommand::Keys => {
            let obj = obj.as_object().expect("Not an object");
            for key in obj.keys() {
                println!("{}", key);
            }
        }
        PrintCommand::Len => {
            let len = match obj {
                Value::Array(arr) => arr.len(),
                Value::Object(obj) => obj.len(),
                _ => panic!("Not an array or object"),
            };
            println!("{}", len);
        }
        PrintCommand::Csv(pairs) => {
            let (selectors, headers): (Vec<_>, Vec<_>) = pairs.into_iter().cloned().unzip();
            let mut csv = csv::Writer::from_writer(stdout());
            csv.write_record(selectors.iter()).unwrap();
            let obj = obj.as_object().expect("Not an object");
            let values = selectors.iter()
                .map(|k| {
                    let v = obj.get(k).unwrap_or(&Value::Null);
                    match v {
                        Value::String(s) => Cow::Borrowed(s.as_bytes()),
                        z => Cow::Owned(serde_json::to_vec(z).unwrap())
                    }
                })
                .collect::<Vec<_>>();
            csv.write_record(values).unwrap();
        }
    }
}

fn main() -> Result<()> {
    let mut cli = Cli::parse();

    let command = cli.command.join("\u{29}");
    let input: Box<dyn Read> = if io::stdin().is_terminal() {
        let filename = cli.command.remove(0);
        let file = File::open(&filename).unwrap();
        Box::new(io::BufReader::new(file))
    } else {
        let stdin = io::stdin();
        Box::new(stdin.lock())
    };

    let command = cli.command.join("\u{29}");
    let (stream, mut print) = evaluate_command(&command);
    if print == PrintCommand::Pretty {
        if cli.yaml_output {
            print = PrintCommand::Yaml;
        }
        if cli.json_output {
            print = PrintCommand::Json;
        }
        if cli.raw {
            print = PrintCommand::Json;
        }
    }
    let deserializer: Box<dyn Iterator<Item=Result<Value>>> = if cli.yaml {
        Box::new(serde_yaml::Deserializer::from_reader(input).map(|v| {
            Value::deserialize(v).map_err(anyhow::Error::from)
        }))
    } else {
        Box::new(serde_json::Deserializer::from_reader(input).into_iter::<Value>().map(|v| {
            v.map_err(anyhow::Error::from)
        }))
    };

    for obj in deserializer {
        let obj = obj?;
        let mut it = apply_stream(obj, &stream).peekable();
        let Some(first) = it.next() else {
            continue;
        };
        if print == PrintCommand::Json && it.peek().is_some() {
            let mut vec = Vec::new();
            vec.push(first);
            vec.extend(it);
            apply_print(Value::Array(vec), &print);
        } else {
            apply_print(first, &print);
            for obj in it {
                apply_print(obj, &print);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn test_evaluate_command() {
        let (commands, _) = evaluate_command("foo");
        assert_eq!(commands, vec![StreamCommand::Key("foo".to_string())]);

        let (commands, _) = evaluate_command(".keys");
        assert_eq!(commands, vec![StreamCommand::Key("keys".to_string())]);

        let (commands, _) = evaluate_command(".a.b.c.");
        assert_eq!(commands, vec![
            StreamCommand::Key("a".to_string()),
            StreamCommand::Key("b".to_string()),
            StreamCommand::Key("c".to_string()),
        ]);

        let (commands, print) = evaluate_command("foo, keys");
        assert_eq!(commands, vec![StreamCommand::Key("foo".to_string())]);
        assert_eq!(print, PrintCommand::Keys);
    }

    #[test]
    fn test_eval_command() {
        let (commands, _) = evaluate_command("[0..5]");
        assert_eq!(commands, vec![StreamCommand::Range(Some(0), Some(5))]);
        let (commands, _) = evaluate_command("[..5]");
        assert_eq!(commands, vec![StreamCommand::Range(None, Some(5))]);
        let (commands, _) = evaluate_command("[..-5]");
        assert_eq!(commands, vec![StreamCommand::Range(None, Some(-5))]);
        let (commands, _) = evaluate_command("[-5..]");
        assert_eq!(commands, vec![StreamCommand::Range(Some(-5), None)]);
        let (commands, _) = evaluate_command("..5");
        assert_eq!(commands, vec![StreamCommand::Range(None, Some(5))]);
        let (commands, _) = evaluate_command("5..");
        assert_eq!(commands, vec![StreamCommand::Range(Some(5), None)]);
    }
}
