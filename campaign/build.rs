//! Build script for `orbitchain-campaign`.
//!
//! Parses `src/event.rs` to discover every `#[contractevent]` struct and
//! emits a machine-readable JSON Schema file to `codegen/schemas/events.json`
//! (relative to the workspace root).
//!
//! The generated schema is **committed to the repository** so that front-end
//! and indexer projects can consume it without running a Rust toolchain.
//! CI enforces freshness with a `git diff --exit-code` check.
//!
//! ## Why build.rs?
//!
//! A build script guarantees the schema is regenerated whenever event
//! definitions change (via `cargo:rerun-if-changed`).  Contributors who
//! add or edit a `#[contractevent]` struct get an immediate
//! `cargo:warning` if they forget to commit the updated schema.
//!
//! ## Issue
//!
//! [#130] — Generate machine-readable event schema from contract.
//!
//! [#130]: https://github.com/OrbitChainLabs/OrbitChain-Contracts/issues/130

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

// ── JSON Schema types ────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
struct EventSchema {
    #[serde(rename = "$schema")]
    schema: &'static str,
    title: &'static str,
    description: &'static str,
    events: Vec<EventDef>,
}

#[derive(serde::Serialize)]
struct EventDef {
    /// Lower-snake-case event name (the on-chain topic).
    name: String,
    /// Fixed topic list defined in `#[contractevent(topics = [...])]`
    /// or derived from the struct name.
    topics: Vec<String>,
    /// Data serialisation format: `"map"`, `"vec"`, or `"single-value"`.
    data_format: String,
    /// Human-readable description extracted from the doc comment.
    description: String,
    /// Event data fields.
    fields: Vec<FieldDef>,
}

#[derive(serde::Serialize)]
struct FieldDef {
    /// Field name as it appears in the Rust struct.
    name: String,
    /// JSON Schema type for the field.
    #[serde(rename = "type")]
    type_: String,
    /// Human-readable description extracted from the field's doc comment.
    description: String,
    /// Optional Soroban-specific type annotation for downstream codegen.
    #[serde(rename = "sorobanType", skip_serializing_if = "Option::is_none")]
    soroban_type: Option<String>,
}

// ── Type mapping ─────────────────────────────────────────────────────────────

/// Map a Rust/Soroban type to a JSON Schema type string and optional
/// Soroban annotation.
fn map_type(ty: &syn::Type) -> (&'static str, Option<String>) {
    let path_str = type_to_string(ty);
    match path_str.as_str() {
        "Address" => ("string", Some("Address".into())),
        "String" => ("string", Some("String".into())),
        "BytesN < 32 >" | "BytesN<32>" => ("string", Some("BytesN<32>".into())),
        "i128" => ("string", Some("i128".into())),
        "u64" => ("string", Some("u64".into())),
        "u32" => ("integer", Some("u32".into())),
        "bool" => ("boolean", None),
        other => {
            // For unrecognised types, fall back to string and include the
            // original Rust type as the Soroban annotation.
            ("string", Some(other.to_string()))
        }
    }
}

/// Best-effort conversion of a `syn::Type` to a string.
fn type_to_string(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(tp) => {
            let mut s = String::new();
            for (i, seg) in tp.path.segments.iter().enumerate() {
                if i > 0 {
                    s.push_str("::");
                }
                s.push_str(&seg.ident.to_string());
                if let syn::PathArguments::AngleBracketed(ref args) = seg.arguments {
                    s.push('<');
                    for (j, arg) in args.args.iter().enumerate() {
                        if j > 0 {
                            s.push_str(", ");
                        }
                        match arg {
                            syn::GenericArgument::Type(inner) => {
                                s.push_str(&type_to_string(inner));
                            }
                            syn::GenericArgument::Lifetime(lt) => {
                                s.push_str(&lt.ident.to_string());
                            }
                            syn::GenericArgument::Const(expr) => {
                                s.push_str(&expr_to_string(expr));
                            }
                            _ => s.push_str("_"),
                        }
                    }
                    s.push('>');
                }
            }
            s
        }
        syn::Type::Reference(tr) => {
            let mut s = String::from("&");
            if let Some(lt) = &tr.lifetime {
                s.push_str(&format!("{} ", lt.ident));
            }
            s.push_str(&type_to_string(&tr.elem));
            s
        }
        _ => "unknown".to_string(),
    }
}

fn expr_to_string(expr: &syn::Expr) -> String {
    match expr {
        syn::Expr::Lit(lit) => match &lit.lit {
            syn::Lit::Int(n) => n.base10_digits().to_string(),
            _ => "_".to_string(),
        },
        _ => "_".to_string(),
    }
}

// ── Doc comment extraction ───────────────────────────────────────────────────

/// Extract the first sentence of a doc comment from a list of attributes.
fn extract_doc(attrs: &[syn::Attribute]) -> String {
    let mut lines: Vec<String> = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(mnv) = &attr.meta {
            if let syn::Expr::Lit(expr_lit) = &mnv.value {
                if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                    let line = lit_str.value().trim().to_string();
                    if !line.is_empty() {
                        lines.push(line);
                    }
                }
            }
        }
    }
    let joined = lines.join(" ");
    // Take up to the first period followed by whitespace or end of string
    // as a single-sentence summary.
    if let Some(pos) = joined.find(". ") {
        joined[..=pos].to_string()
    } else {
        joined
    }
}

// ── Attribute argument extraction ────────────────────────────────────────────

/// Parse `#[contractevent(topics = ["a", "b"], data_format = "vec")]`.
struct ContractEventMeta {
    topics: Vec<String>,
    data_format: String,
}

fn parse_contractevent_meta(attrs: &[syn::Attribute]) -> Option<ContractEventMeta> {
    for attr in attrs {
        if !attr.path().is_ident("contractevent") {
            continue;
        }

        let mut topics: Vec<String> = Vec::new();
        let mut data_format = "map".to_string();

        // Parse the attribute's token stream manually.
        // `attr` looks like: `#[contractevent(topics = ["a", "b"], data_format = "vec")]`
        // We parse the token stream as a sequence of `ident = value` pairs
        // inside parentheses.
        //
        // LIMITATION: We split on comma characters, which would break if a
        // topic string contained a literal comma.  All current topics use
        // simple snake_case identifiers, so this is acceptable.

        // Use syn's meta parsing
        if let syn::Meta::List(ml) = &attr.meta {
            let tokens = &ml.tokens;
            // Parse as: `topics = [...], data_format = "..."` or just empty
            if tokens.is_empty() {
                return Some(ContractEventMeta {
                    topics,
                    data_format,
                });
            }

            // Parse comma-separated name = value pairs
            let pairs: Vec<String> = tokens
                .to_string()
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();

            for pair in pairs {
                let parts: Vec<&str> = pair.splitn(2, '=').map(|s| s.trim()).collect();
                if parts.len() != 2 {
                    continue;
                }

                match parts[0] {
                    "topics" => {
                        // Parse ["a", "b"] — extract quoted strings
                        let val = parts[1];
                        topics = val
                            .trim_matches(|c| c == '[' || c == ']')
                            .split(',')
                            .filter_map(|s| {
                                let trimmed = s.trim().trim_matches('"');
                                if trimmed.is_empty() {
                                    None
                                } else {
                                    Some(trimmed.to_string())
                                }
                            })
                            .collect();
                    }
                    "data_format" => {
                        let val = parts[1].trim().trim_matches('"');
                        data_format = val.to_string();
                    }
                    _ => {}
                }
            }
        }

        return Some(ContractEventMeta {
            topics,
            data_format,
        });
    }
    None
}

// ── Main extraction logic ────────────────────────────────────────────────────

fn extract_events(source: &str) -> Vec<EventDef> {
    let file = match syn::parse_file(source) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("cargo:warning=failed to parse event.rs: {e}");
            return Vec::new();
        }
    };

    let mut events: Vec<EventDef> = Vec::new();

    for item in &file.items {
        let syn::Item::Struct(item_struct) = item else {
            continue;
        };

        // Check for #[contractevent] attribute
        let meta = match parse_contractevent_meta(&item_struct.attrs) {
            Some(m) => m,
            None => continue,
        };

        // Derive event name from struct name (PascalCase → snake_case)
        let struct_name = item_struct.ident.to_string();
        let event_name = to_snake_case(&struct_name);

        // Use custom topics if provided, otherwise derive from struct name
        let topics = if meta.topics.is_empty() {
            vec![event_name.clone()]
        } else {
            meta.topics.clone()
        };

        let description = extract_doc(&item_struct.attrs);

        // Extract fields
        let mut fields: Vec<FieldDef> = Vec::new();
        match &item_struct.fields {
            syn::Fields::Named(named) => {
                for field in &named.named {
                    let field_name = field.ident.as_ref().map(|i| i.to_string()).unwrap_or_default();
                    let (json_type, soroban_type) = map_type(&field.ty);
                    let field_desc = extract_doc(&field.attrs);
                    fields.push(FieldDef {
                        name: field_name,
                        type_: json_type.to_string(),
                        description: field_desc,
                        soroban_type: soroban_type.map(String::from),
                    });
                }
            }
            syn::Fields::Unit => {
                // Unit struct — no data fields.
            }
            syn::Fields::Unnamed(_) => {
                // Tuple struct — not expected for contract events.
            }
        }

        events.push(EventDef {
            name: event_name,
            topics,
            data_format: meta.data_format,
            description,
            fields,
        });
    }

    events
}

/// Convert PascalCase to snake_case.
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, ch) in s.chars().enumerate() {
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

// ── Schema output ────────────────────────────────────────────────────────────

fn write_schema(events: &[EventDef], out_path: &Path) {
    let schema = EventSchema {
        schema: "https://json-schema.org/draft/2020-12/schema",
        title: "OrbitChain Campaign Contract Events",
        description: "Machine-readable event schemas for the OrbitChain campaign contract. Generated by campaign/build.rs from #[contractevent] structs.",
        events: events.to_vec(),
    };

    let json = serde_json::to_string_pretty(&schema).unwrap_or_else(|e| {
        panic!("failed to serialise event schema: {e}");
    });

    // Ensure the parent directory exists.
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|e| {
            panic!("failed to create schema output directory {:?}: {e}", parent);
        });
    }

    fs::write(out_path, json.as_bytes()).unwrap_or_else(|e| {
        panic!("failed to write event schema to {:?}: {e}", out_path);
    });
}

// ── Entry point ──────────────────────────────────────────────────────────────

fn main() {
    // Only regenerate if the event source changes.
    println!("cargo:rerun-if-changed=src/event.rs");
    println!("cargo:rerun-if-changed=build.rs");

    // Resolve paths relative to the campaign crate root.
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let event_rs = manifest_dir.join("src").join("event.rs");

    // Output path: <workspace_root>/codegen/schemas/events.json
    let out_path = manifest_dir
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("codegen")
        .join("schemas")
        .join("events.json");

    // Read and parse the event source.
    let source = match fs::read_to_string(&event_rs) {
        Ok(s) => s,
        Err(e) => {
            println!("cargo:warning=build.rs: cannot read {:?}: {e}", event_rs);
            return;
        }
    };

    let events = extract_events(&source);

    if events.is_empty() {
        println!(
            "cargo:warning=build.rs: no #[contractevent] structs found in {:?}",
            event_rs
        );
        return;
    }

    write_schema(&events, &out_path);

    // Print a friendly summary so it's visible in build output.
    let relative = out_path
        .strip_prefix(manifest_dir.parent().unwrap_or(Path::new(".")))
        .unwrap_or(&out_path);
    println!(
        "cargo:warning=build.rs: event schema written to {} (commit this file)",
        relative.display()
    );
}
