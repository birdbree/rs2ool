//! What the inspector shows, kept apart from egui so it can be tested:
//! the sidecar as key/value rows, the decoded fields, a hex dump, and which
//! fields link to other records.

use serde_json::Value;

use crate::explorer::selection::LoadedData;

/// Key/value rows for a grid.
pub type Rows = Vec<(String, String)>;

/// Flattens a JSON value into `(path, value)` rows, with paths like
/// `models[3].model`.
///
/// An array of plain values is one row, cut to `max_items` with the total
/// after. An array of objects or arrays gets an `N items` row and then its
/// first few entries (at most 8), each flattened.
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
            // Flat arrays: one `[a, b, …]` row.
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
    // Strings without quotes, null as a dash.
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

/// The metadata rows and the decoded-field rows of a loaded selection.
pub fn sections(data: &LoadedData) -> (Rows, Rows) {
    match data {
        LoadedData::Record { record, meta, .. } => {
            let mut m = serde_json::to_value(meta).unwrap_or(Value::Null);
            // The definition is shown below; keep the sidecar to the facts.
            if let Some(o) = m.as_object_mut() {
                o.remove("definition");
            }
            let r = serde_json::to_value(record).unwrap_or(Value::Null);
            // Skip the type tag and show the record itself.
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
        LoadedData::Note { rows } => (rows.clone(), Vec::new()),
    }
}

/// The raw bytes to show in the hex view: MIDI records, pack records, and
/// dump files that aren't JSON. `None` for everything else.
pub fn raw_bytes(data: &LoadedData) -> Option<&[u8]> {
    match data {
        LoadedData::Record { record, .. } => match &**record {
            rs2cache::types::record::Record::Midi(b) => Some(b),
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
///
/// `row` is the line number (offset / 16) and has to be in range; it
/// panics past the end of `bytes`.
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
    // 47 = 16 bytes × 2 digits + 15 spaces, so a short last line lines up.
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

/// The record type a field of `of` refers to, by its flattened key, or
/// `None` if the field isn't a reference. Array indices in `key` are
/// ignored, so `models[3].model` matches `models.model`.
pub fn link_type(
    of: rs2cache::types::record::RecordType,
    key: &str,
) -> Option<rs2cache::types::record::RecordType> {
    use rs2cache::types::record::RecordType as T;
    // `key` with every `[…]` cut out.
    let mut k = String::with_capacity(key.len());
    let mut skip = false;
    for c in key.chars() {
        match c {
            '[' => skip = true,
            ']' => skip = false,
            c if !skip => k.push(c),
            _ => {}
        }
    }
    Some(match (of, k.as_str()) {
        (T::Npc, "models" | "chat_head_models") => T::Model,
        (
            T::Npc,
            "stand_anim" | "walk_anim" | "walk_back_anim" | "walk_left_anim" | "walk_right_anim",
        ) => T::Seq,
        (T::Npc, "morph.children") => T::Npc,
        (T::Loc, "models.model") => T::Model,
        (T::Loc, "animation") => T::Seq,
        (T::Loc, "morph.children") => T::Loc,
        (
            T::Obj,
            "inventory_model" | "male_equip1" | "male_equip2" | "male_equip3" | "female_equip1"
            | "female_equip2" | "female_equip3" | "male_chat_head1" | "male_chat_head2"
            | "female_chat_head1" | "female_chat_head2",
        ) => T::Model,
        (T::Obj, "noted_id" | "noted_template" | "stack_variants.item") => T::Obj,
        (T::Idk, "models" | "chat_head_models") => T::Model,
        (T::SpotAnim, "model") => T::Model,
        (T::SpotAnim, "sequence") => T::Seq,
        (T::Map, "locations.id") => T::Loc,
        (T::Varbit, "varp") => T::Varp,
        (T::Interface, "content.children.id") => T::Interface,
        (T::Interface, "content.model" | "content.enabled_model") => T::Model,
        (T::Interface, "content.animation" | "content.enabled_animation") => T::Seq,
        _ => return None,
    })
}

/// The ids in a row's displayed value: one number, or a `[a, b, …]` list.
/// Anything that isn't a number is skipped, so a name gives nothing.
pub fn link_ids(value: &str) -> Vec<u32> {
    value
        .trim_start_matches('[')
        .split(['[', ']', ','])
        // Stop at the `… (N total)` tail that cut-down arrays get.
        .map_while(|p| {
            let p = p.trim();
            if p.starts_with('…') { None } else { Some(p) }
        })
        .filter_map(|p| p.parse().ok())
        .collect()
}

#[cfg(test)]
mod link_tests {
    use super::*;
    use rs2cache::types::record::RecordType as T;

    #[test]
    fn references_resolve() {
        assert_eq!(link_type(T::Npc, "models"), Some(T::Model));
        assert_eq!(link_type(T::Npc, "stand_anim"), Some(T::Seq));
        assert_eq!(link_type(T::Loc, "models[3].model"), Some(T::Model));
        assert_eq!(link_type(T::Map, "locations[0].id"), Some(T::Loc));
        assert_eq!(link_type(T::Npc, "name"), None);
        assert_eq!(link_ids("[12, 34, 56] … (7 total)"), [12, 34, 56]);
        assert_eq!(link_ids("789"), [789]);
        assert!(link_ids("Name").is_empty());
    }
}
