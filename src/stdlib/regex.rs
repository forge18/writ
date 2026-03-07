use std::cell::RefCell;
use std::rc::Rc;

use regex::Regex;
use crate::vm::{Value, WritObject, VM};

#[derive(Debug)]
struct WritRegex {
    pattern: Regex,
    source: String,
}

impl WritRegex {
    fn new(pattern: &str) -> Result<Self, String> {
        Regex::new(pattern)
            .map(|r| Self { pattern: r, source: pattern.to_string() })
            .map_err(|e| format!("invalid regex: {e}"))
    }
}

impl WritObject for WritRegex {
    fn type_name(&self) -> &str {
        "Regex"
    }

    fn get_field(&self, name: &str) -> Result<Value, String> {
        match name {
            "source" => Ok(Value::Str(Rc::new(self.source.clone()))),
            _ => Err(format!("Regex has no field '{name}'")),
        }
    }

    fn set_field(&mut self, name: &str, _value: Value) -> Result<(), String> {
        Err(format!("Regex has no settable field '{name}'"))
    }

    fn call_method(&mut self, name: &str, args: &[Value]) -> Result<Value, String> {
        match name {
            // .match(input: string) -> Optional<string>
            // Returns the first full match, or null if no match.
            "match" => {
                let input = extract_str(args.first(), "match")?;
                match self.pattern.find(&input) {
                    Some(m) => Ok(Value::Str(Rc::new(m.as_str().to_string()))),
                    None => Ok(Value::Null),
                }
            }

            // .matchAll(input: string) -> Array<string>
            // Returns all non-overlapping matches as an array of strings.
            "matchAll" => {
                let input = extract_str(args.first(), "matchAll")?;
                let matches: Vec<Value> = self
                    .pattern
                    .find_iter(&input)
                    .map(|m| Value::Str(Rc::new(m.as_str().to_string())))
                    .collect();
                Ok(Value::Array(Rc::new(RefCell::new(matches))))
            }

            // .replace(input: string, replacement: string) -> string
            // Replaces the first occurrence. Use $1, $2 etc. for capture groups.
            "replace" => {
                let input = extract_str(args.first(), "replace: input")?;
                let replacement = extract_str(args.get(1), "replace: replacement")?;
                Ok(Value::Str(Rc::new(
                    self.pattern.replacen(&input, 1, replacement.as_str()).to_string(),
                )))
            }

            // .replaceAll(input: string, replacement: string) -> string
            // Replaces all occurrences.
            "replaceAll" => {
                let input = extract_str(args.first(), "replaceAll: input")?;
                let replacement = extract_str(args.get(1), "replaceAll: replacement")?;
                Ok(Value::Str(Rc::new(
                    self.pattern.replace_all(&input, replacement.as_str()).to_string(),
                )))
            }

            // .test(input: string) -> bool
            // Returns true if the pattern matches anywhere in the input.
            "test" => {
                let input = extract_str(args.first(), "test")?;
                Ok(Value::Bool(self.pattern.is_match(&input)))
            }

            _ => Err(format!("Regex has no method '{name}'")),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn extract_str(val: Option<&Value>, context: &str) -> Result<String, String> {
    match val {
        Some(Value::Str(s)) => Ok((**s).clone()),
        Some(other) => Err(format!("{context}: expected string, got {}", other.type_name())),
        None => Err(format!("{context}: missing string argument")),
    }
}

pub fn register(vm: &mut VM) {
    vm.register_type("Regex", |args| {
        let pattern = match args.first() {
            Some(Value::Str(s)) => s.as_str().to_string(),
            Some(other) => {
                return Err(format!(
                    "Regex: expected string pattern, got {}",
                    other.type_name()
                ))
            }
            None => return Err("Regex: missing pattern argument".to_string()),
        };
        Ok(Box::new(WritRegex::new(&pattern)?))
    });
}
