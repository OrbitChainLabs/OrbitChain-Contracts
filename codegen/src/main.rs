//! OrbitChain event schema codegen.
//!
//! Extracts event struct definitions from Rust contract source files and
//! emits JSON Schema (Draft-07) files under `codegen/schemas/`.
//!
//! ## Usage
//!
//! ```sh
//! cargo run -p orbitchain-codegen -- \
//!   --src campaign/src/types.rs \
//!   --out codegen/schemas/
//! ```
//!
//! When run without `--out`, validates existing schemas are up-to-date
//! (exit code 0 = clean, non-zero = staleness detected). This mode is
//! used in CI (`make check-schemas`).

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

// ── JSON Schema model ────────────────────────────────────────────────────────

#[derive(Serialize)]
struct JsonSchema {
    #[serde(rename = "$schema")]
    schema: String,
    #[serde(rename = "$id")]
    id: String,
    title: String,
    description: String,
    #[serde(rename = "type")]
    schema_type: String,
    required: Vec<String>,
    properties: BTreeMap<String, Property>,
    #[serde(skip_serializing_if = "Option::is_none")]
    additionalProperties: Option<bool>,
}

#[derive(Serialize)]
struct Property {
    #[serde(rename = "type")]
    prop_type: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    minimum: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    r#enum: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "$ref")]
    ref_path: Option<String>,
}

// ── Struct parser ────────────────────────────────────────────────────────────

#[derive(Debug)]
struct EventStruct {
    name: String,
    fields: Vec<Field>,
}

#[derive(Debug)]
struct Field {
    name: String,
    rust_type: String,
}

/// Parses `#[contracttype]`-annotated structs ending in "Event" from a Rust
/// source file. The parser is deliberately simple (line-oriented, no `syn`
/// dependency) to keep the codegen crate lightweight.
fn parse_event_structs(source: &str) -> Result<Vec<EventStruct>> {
    let mut events = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        // Look for `#[contracttype]` followed by `pub struct ... Event`
        if lines[i].trim() == "#[contracttype]" {
            let mut j = i + 1;

            // Skip any additional attributes (e.g., `#[derive(...)]`)
            while j < lines.len() && lines[j].trim().starts_with("#[") {
                j += 1;
            }

            if j < lines.len() {
                let trimmed = lines[j].trim();
                if let Some(struct_name) = parse_struct_header(trimmed) {
                    if struct_name.contains("Event") || struct_name.ends_with("Event") {
                        // Parse struct body
                        let fields = parse_struct_body(&lines, j + 1)?;
                        let field_count = fields.len();
                        events.push(EventStruct {
                            name: struct_name,
                            fields,
                        });
                        i = j + field_count + 3; // skip past struct body
                        continue;
                    }
                }
            }
        }
        i += 1;
    }

    if events.is_empty() {
        anyhow::bail!("No event structs found in source file");
    }
    Ok(events)
}

fn parse_struct_header(line: &str) -> Option<String> {
    let line = line.trim();
    if line.starts_with("pub struct ") {
        let rest = &line["pub struct ".len()..];
        let name = rest
            .split(|c: char| c == '{' || c == '<' || c == '(')
            .next()
            .unwrap_or(rest)
            .trim()
            .to_string();
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

fn parse_struct_body(lines: &[&str], start_idx: usize) -> Result<Vec<Field>> {
    let mut fields = Vec::new();
    // The opening `{` is on the struct header line (already consumed by the
    // caller), so we start with depth=1 and already inside the body.
    let mut depth = 1u32;

    for line in lines.iter().skip(start_idx) {
        let trimmed = line.trim();

        // Track brace nesting: nested structs / blocks may open/close braces
        if trimmed.contains('{') {
            depth += trimmed.matches('{').count() as u32;
        }
        if trimmed.contains('}') {
            let close_count = trimmed.matches('}').count() as u32;
            if close_count >= depth {
                break;
            }
            depth -= close_count;
        }

        // Skip empty lines, comments, and doc comments
        if trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed.starts_with("///")
            || trimmed.starts_with("#[")
        {
            continue;
        }

        // Parse field: `pub field_name: Type,`
        if let Some(field) = parse_field(trimmed) {
            fields.push(field);
        }
    }

    Ok(fields)
}

fn parse_field(line: &str) -> Option<Field> {
    let line = line.trim();
    // Strip trailing comma
    let line = line.strip_suffix(',').unwrap_or(line).trim();

    if line.starts_with("pub ") {
        let after_pub = &line["pub ".len()..];

        // Find the colon separator
        if let Some(colon_pos) = after_pub.find(':') {
            let name = after_pub[..colon_pos].trim().to_string();
            let rust_type = after_pub[colon_pos + 1..].trim().to_string();

            if !name.is_empty() && !rust_type.is_empty() {
                return Some(Field { name, rust_type });
            }
        }
    }

    None
}

// ── Type mapping ─────────────────────────────────────────────────────────────

fn map_type(rust_type: &str) -> Property {
    let t = rust_type.trim();

    // Handle Option<T> — for events, Option means nullable
    if t.starts_with("Option<") {
        let inner = &t["Option<".len()..t.len() - 1];
        let mut prop = map_type(inner);
        prop.prop_type = format!("[{}, \"null\"]", prop.prop_type);
        return prop;
    }

    // Handle Vec<T>
    if t.starts_with("Vec<") {
        return Property {
            prop_type: "array".into(),
            description: format!("Array of {}", &t["Vec<".len()..t.len() - 1]),
            format: None,
            minimum: None,
            r#enum: None,
            ref_path: None,
        };
    }

    match t {
        "Address" => Property {
            prop_type: "string".into(),
            description: "Stellar address (G...)".into(),
            format: None,
            minimum: None,
            r#enum: None,
            ref_path: None,
        },
        "i128" => Property {
            prop_type: "string".into(),
            description: "128-bit signed integer, encoded as a decimal string".into(),
            format: Some("int128".into()),
            minimum: None,
            r#enum: None,
            ref_path: None,
        },
        "u64" => Property {
            prop_type: "integer".into(),
            description: "64-bit unsigned integer (UNIX timestamp or counter)".into(),
            format: Some("uint64".into()),
            minimum: Some(0),
            r#enum: None,
            ref_path: None,
        },
        "u32" => Property {
            prop_type: "integer".into(),
            description: "32-bit unsigned integer".into(),
            format: Some("uint32".into()),
            minimum: Some(0),
            r#enum: None,
            ref_path: None,
        },
        "String" => Property {
            prop_type: "string".into(),
            description: "UTF-8 string".into(),
            format: None,
            minimum: None,
            r#enum: None,
            ref_path: None,
        },
        "bool" => Property {
            prop_type: "boolean".into(),
            description: "Boolean value".into(),
            format: None,
            minimum: None,
            r#enum: None,
            ref_path: None,
        },
        "BytesN<32>" => Property {
            prop_type: "string".into(),
            description: "32-byte value, hex-encoded".into(),
            format: Some("hex32".into()),
            minimum: None,
            r#enum: None,
            ref_path: None,
        },
        "CampaignStatus" => Property {
            prop_type: "string".into(),
            description: "Campaign lifecycle status".into(),
            format: None,
            minimum: None,
            r#enum: Some(vec![
                "Active".into(),
                "GoalReached".into(),
                "Ended".into(),
                "Cancelled".into(),
            ]),
            ref_path: None,
        },
        "MilestoneStatus" => Property {
            prop_type: "string".into(),
            description: "Milestone lifecycle status".into(),
            format: None,
            minimum: None,
            r#enum: Some(vec![
                "Locked".into(),
                "Unlocked".into(),
                "Released".into(),
            ]),
            ref_path: None,
        },
        "AssetInfo" => Property {
            prop_type: "string".into(),
            description: "Asset selector: \"Native\" or Stellar contract address".into(),
            format: None,
            minimum: None,
            r#enum: None,
            ref_path: None,
        },
        other => Property {
            prop_type: "string".into(),
            description: format!("Value of type `{}`", other),
            format: None,
            minimum: None,
            r#enum: None,
            ref_path: None,
        },
    }
}

// ── Schema generation ────────────────────────────────────────────────────────

fn event_name_to_snake(name: &str) -> String {
    // Strip "Event" suffix if present, then convert CamelCase to snake_case
    let base = name.strip_suffix("Event").unwrap_or(name);
    let mut result = String::new();
    for (i, ch) in base.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch);
        }
    }
    result
}

fn generate_schema(event: &EventStruct) -> JsonSchema {
    let snake_name = event_name_to_snake(&event.name);
    let mut required = Vec::new();
    let mut properties = BTreeMap::new();

    for field in &event.fields {
        let prop = map_type(&field.rust_type);
        // Fields are required unless they're Option<T>
        if !field.rust_type.starts_with("Option<") {
            required.push(field.name.clone());
        }
        properties.insert(field.name.clone(), prop);
    }

    JsonSchema {
        schema: "http://json-schema.org/draft-07/schema#".into(),
        id: format!(
            "https://schemas.orbitchain.org/events/{}.json",
            snake_name
        ),
        title: format!("OrbitChain Event: {}", snake_name),
        description: format!(
            "Event emitted by the OrbitChain campaign contract. Topic: `{}`",
            snake_name
        ),
        schema_type: "object".into(),
        required,
        properties,
        additionalProperties: Some(false),
    }
}

// ── Main logic ───────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mut src_path = None;
    let mut out_dir = None;
    let mut check_mode = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--src" => {
                i += 1;
                src_path = Some(PathBuf::from(&args[i]));
            }
            "--out" => {
                i += 1;
                out_dir = Some(PathBuf::from(&args[i]));
            }
            "--check" => {
                check_mode = true;
            }
            _ => {
                anyhow::bail!("Unknown argument: {}", args[i]);
            }
        }
        i += 1;
    }

    let src_path = src_path.unwrap_or_else(|| PathBuf::from("campaign/src/types.rs"));
    let out_dir = out_dir.unwrap_or_else(|| PathBuf::from("codegen/schemas"));

    let source = fs::read_to_string(&src_path)
        .with_context(|| format!("Failed to read source file: {}", src_path.display()))?;

    let events = parse_event_structs(&source)
        .with_context(|| "Failed to parse event structs from source")?;

    eprintln!("Found {} event struct(s):", events.len());
    for event in &events {
        eprintln!("  - {}", event.name);
    }

    let schemas: Vec<(String, JsonSchema)> = events
        .iter()
        .map(|e| (event_name_to_snake(&e.name), generate_schema(e)))
        .collect();

    if check_mode {
        return check_schemas(&schemas, &out_dir);
    }

    // Generate schemas
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("Failed to create output directory: {}", out_dir.display()))?;

    for (name, schema) in &schemas {
        let filename = format!("{}.json", name);
        let path = out_dir.join(&filename);
        let json = serde_json::to_string_pretty(schema)?;
        fs::write(&path, json)
            .with_context(|| format!("Failed to write schema: {}", path.display()))?;
        eprintln!("  Wrote: {}", path.display());
    }

    eprintln!("Done. {} schemas generated.", schemas.len());
    Ok(())
}

fn check_schemas(schemas: &[(String, JsonSchema)], out_dir: &Path) -> Result<()> {
    let mut errors = Vec::new();

    for (name, schema) in schemas {
        let path = out_dir.join(format!("{}.json", name));
        let new_json = serde_json::to_string_pretty(schema)?;
        let new_value: Value = serde_json::from_str(&new_json)
            .with_context(|| format!("Failed to parse generated schema for {}", name))?;

        match fs::read_to_string(&path) {
            Ok(existing_json) => {
                let existing_value: Value = serde_json::from_str(&existing_json)
                    .with_context(|| format!("Failed to parse committed schema: {}", path.display()))?;
                if !schemas_are_structurally_equal(&new_value, &existing_value) {
                    // Write regenerated schema to a temp file for diff inspection
                    let tmp_path = out_dir.join(format!("{}.generated.json", name));
                    fs::write(&tmp_path, &new_json).ok();
                    eprintln!("\n--- Mismatch for {} ---", name);
                    eprintln!("Generated schema written to: {}", tmp_path.display());
                    eprintln!("Run: diff {} {}", tmp_path.display(), path.display());
                    errors.push(format!(
                        "Schema mismatch for {} — run `cargo run -p orbitchain-codegen` to regenerate",
                        path.display()
                    ));
                }
            }
            Err(_) => {
                errors.push(format!(
                    "Missing schema file: {} — run `cargo run -p orbitchain-codegen` to generate",
                    path.display()
                ));
            }
        }
    }

    if errors.is_empty() {
        eprintln!("All {} struct-based schemas are up-to-date.", schemas.len());
        Ok(())
    } else {
        for err in &errors {
            eprintln!("ERROR: {}", err);
        }
        anyhow::bail!(
            "{} schema(s) are stale or missing. Run `cargo run -p orbitchain-codegen` to regenerate.",
            errors.len()
        );
    }
}

/// Compare two JSON Schema values structurally, ignoring cosmetic fields
/// (title, description, $id) that don't affect machine-readability.
fn schemas_are_structurally_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(a_map), Value::Object(b_map)) => {
            // Collect keys to compare, skipping cosmetic fields
            let cosmetic: &[&str] = &["title", "description", "$id"];
            let a_keys: Vec<&String> = a_map.keys().filter(|k| !cosmetic.contains(&k.as_str())).collect();
            let b_keys: Vec<&String> = b_map.keys().filter(|k| !cosmetic.contains(&k.as_str())).collect();

            if a_keys.len() != b_keys.len() {
                return false;
            }

            for key in &a_keys {
                let a_val = match a_map.get(*key) {
                    Some(v) => v,
                    None => return false,
                };
                let b_val = match b_map.get(*key) {
                    Some(v) => v,
                    None => return false,
                };

                // For the `properties` object, recursively strip description from each property
                if *key == "properties" {
                    if !properties_are_structurally_equal(a_val, b_val) {
                        return false;
                    }
                } else if !schemas_are_structurally_equal(a_val, b_val) {
                    return false;
                }
            }
            true
        }
        (Value::Array(a_arr), Value::Array(b_arr)) => {
            if a_arr.len() != b_arr.len() {
                return false;
            }
            a_arr.iter().zip(b_arr.iter()).all(|(a, b)| schemas_are_structurally_equal(a, b))
        }
        _ => a == b,
    }
}

/// Compare properties objects, ignoring `description` fields within each property.
fn properties_are_structurally_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(a_map), Value::Object(b_map)) => {
            if a_map.len() != b_map.len() {
                return false;
            }
            for (key, a_prop) in a_map {
                let b_prop = match b_map.get(key) {
                    Some(v) => v,
                    None => return false,
                };
                // Strip description from each property object before comparing
                let a_clean = strip_description(a_prop);
                let b_clean = strip_description(b_prop);
                if a_clean != b_clean {
                    return false;
                }
            }
            true
        }
        _ => a == b,
    }
}

/// Remove the `description` field from a JSON object.
fn strip_description(v: &Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut cleaned = map.clone();
            cleaned.remove("description");
            Value::Object(cleaned)
        }
        _ => v.clone(),
    }
}
