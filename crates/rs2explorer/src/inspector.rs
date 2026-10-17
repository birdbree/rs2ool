//! What the inspector shows, kept apart from egui so it can be tested:
//! the sidecar as key/value rows, the decoded fields, and a hex dump.

use serde_json::Value;

use crate::selection::LoadedData;

/// Key/value rows for a grid.
pub type Rows = Vec<(String, String)>;

/// Flatten a JSON value into `(path, value)` rows. Arrays longer than
/// `max_items` show their length and the first items.
pub fn rows(value: &Value, max_items: usize) -> Rows {
    fn walk(prefix: &str, v: &Value, max: usize, out: &mut Vec<(String, String)>) {
        match v {
            Value::Object(m) => {
                for (k, x) in m {
                    let p = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{prefix}.{k}")
                    };
                    walk(&p, x, max, out);
                }
            }
            Value::Array(a) if a.iter().all(|x| !x.is_object() && !x.is_array()) => {
                let shown: Vec<String> = a.iter().take(max).map(scalar).collect();
                let more = if a.len() > max {
                    format!(" … ({} total)", a.len())
                } else {
                    String::new()
                };
                out.push((prefix.to_owned(), format!("[{}]{more}", shown.join(", "))));
            }
            Value::Array(a) => {
                out.push((prefix.to_owned(), format!("{} items", a.len())));
                // Arrays of structures (faces, frames) show a few entries.
                for (i, x) in a.iter().take(max.min(8)).enumerate() {
                    walk(&format!("{prefix}[{i}]"), x, max, out);
                }
            }
            other => out.push((prefix.to_owned(), scalar(other))),
        }
    }
    fn scalar(v: &Value) -> String {
        match v {
            Value::String(s) => s.clone(),
            Value::Null => "–".into(),
            other => other.to_string(),
        }
    }
    let mut out = Vec::new();
    walk("", value, max_items, &mut out);
    out
}

/// The sidecar rows and the decoded-field rows of a loaded selection.
pub fn sections(data: &LoadedData) -> (Rows, Rows) {
    match data {
        LoadedData::Record { record, meta } => {
            let mut m = serde_json::to_value(meta).unwrap_or(Value::Null);
            // The definition is shown below; keep the sidecar to the facts.
            if let Some(o) = m.as_object_mut() {
                o.remove("definition");
            }
            let r = serde_json::to_value(record).unwrap_or(Value::Null);
            let inner = r.get("record").cloned().unwrap_or(r);
            (rows(&m, 12), rows(&inner, 24))
        }
        LoadedData::Pack {
            kind,
            id,
            bytes,
            decoded,
        } => (
            vec![
                ("kind".into(), kind.to_string()),
                ("id".into(), format!("{id} ({id:#x})")),
                ("size".into(), bytes.len().to_string()),
            ],
            vec![("decoded".into(), decoded.clone())],
        ),
        LoadedData::Dump { path, json, bytes } => (
            vec![
                ("file".into(), path.display().to_string()),
                ("size".into(), bytes.len().to_string()),
            ],
            json.as_ref().map(|j| rows(j, 24)).unwrap_or_default(),
        ),
    }
}

/// The raw bytes to show in the hex view, when the selection has any.
pub fn raw_bytes(data: &LoadedData) -> Option<&[u8]> {
    match data {
        LoadedData::Record { record, .. } => match &**record {
            rs2intermediate::record::Record::Midi(b) => Some(b),
            _ => None,
        },
        LoadedData::Pack { bytes, .. } => Some(bytes),
        LoadedData::Dump {
            json: None, bytes, ..
        } => Some(bytes),
        _ => None,
    }
}

/// One hex-view line: offset, 16 bytes in hex, and their printable ASCII.
pub fn hex_line(bytes: &[u8], row: usize) -> String {
    let start = row * 16;
    let chunk = &bytes[start..bytes.len().min(start + 16)];
    let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
    let ascii: String = chunk
        .iter()
        .map(|&b| {
            if (32..127).contains(&b) {
                b as char
            } else {
                '.'
            }
        })
        .collect();
    format!("{start:08x}  {:<47}  {ascii}", hex.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flattens_nested_values() {
        let v = serde_json::json!({"a": 1, "b": {"c": "x", "d": [1, 2, 3]}, "e": [{"f": null}]});
        let r = rows(&v, 2);
        assert!(r.contains(&("a".into(), "1".into())));
        assert!(r.contains(&("b.c".into(), "x".into())));
        assert!(r.contains(&("b.d".into(), "[1, 2] … (3 total)".into())));
        assert!(r.contains(&("e[0].f".into(), "–".into())));
    }

    #[test]
    fn hex_lines() {
        let b: Vec<u8> = (0..20).collect();
        assert!(hex_line(&b, 0).starts_with("00000000  00 01 02"));
        assert!(hex_line(&b, 1).starts_with("00000010  10 11 12 13"));
        assert_eq!(
            hex_line(b"AB", 0).trim_end(),
            format!("00000000  41 42{}  AB", " ".repeat(42)).trim_end()
        );
    }
}
