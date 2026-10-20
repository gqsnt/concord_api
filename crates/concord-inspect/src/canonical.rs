//! Canonical JSON text: two-space indentation, sorted object keys, preserved
//! array order and a trailing newline.

use serde_json::Value;

/// Renders `value` as canonical JSON text.
///
/// Object keys are sorted by byte order regardless of the map
/// implementation enabled in `serde_json`, array order is preserved, and the
/// output ends with a newline. Equal values always produce identical bytes.
#[must_use]
pub fn to_canonical_string(value: &Value) -> String {
    let mut output = String::new();
    write_value(&mut output, value, 0);
    output.push('\n');
    output
}

fn write_value(output: &mut String, value: &Value, depth: usize) {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(flag) => output.push_str(if *flag { "true" } else { "false" }),
        Value::Number(number) => output.push_str(&number.to_string()),
        Value::String(text) => write_string(output, text),
        Value::Array(items) => {
            if items.is_empty() {
                output.push_str("[]");
                return;
            }
            output.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                newline(output, depth + 1);
                write_value(output, item, depth + 1);
            }
            newline(output, depth);
            output.push(']');
        }
        Value::Object(map) => {
            if map.is_empty() {
                output.push_str("{}");
                return;
            }
            let mut keys = map.keys().collect::<Vec<_>>();
            keys.sort();
            output.push('{');
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                newline(output, depth + 1);
                write_string(output, key);
                output.push_str(": ");
                write_value(output, &map[key], depth + 1);
            }
            newline(output, depth);
            output.push('}');
        }
    }
}

fn newline(output: &mut String, depth: usize) {
    output.push('\n');
    for _ in 0..depth {
        output.push_str("  ");
    }
}

fn write_string(output: &mut String, text: &str) {
    output.push('"');
    for character in text.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if u32::from(character) < 0x20 => {
                output.push_str(&format!("\\u{:04x}", u32::from(character)));
            }
            character => output.push(character),
        }
    }
    output.push('"');
}
