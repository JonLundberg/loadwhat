// Formats and emits the public line-oriented token output contract.

pub const TOKEN_DEBUG_STRING: &str = "DEBUG_STRING";
pub const TOKEN_DYNAMIC_MISSING: &str = "DYNAMIC_MISSING";
pub const TOKEN_FIRST_BREAK: &str = "FIRST_BREAK";
pub const TOKEN_NOTE: &str = "NOTE";
pub const TOKEN_RUN_END: &str = "RUN_END";
pub const TOKEN_RUN_START: &str = "RUN_START";
pub const TOKEN_RUNTIME_LOADED: &str = "RUNTIME_LOADED";
pub const TOKEN_SEARCH_ORDER: &str = "SEARCH_ORDER";
pub const TOKEN_SEARCH_PATH: &str = "SEARCH_PATH";
pub const TOKEN_STATIC_BAD_IMAGE: &str = "STATIC_BAD_IMAGE";
pub const TOKEN_STATIC_END: &str = "STATIC_END";
pub const TOKEN_STATIC_FOUND: &str = "STATIC_FOUND";
pub const TOKEN_STATIC_IMPORT: &str = "STATIC_IMPORT";
pub const TOKEN_STATIC_MISSING: &str = "STATIC_MISSING";
pub const TOKEN_STATIC_START: &str = "STATIC_START";
pub const TOKEN_SUCCESS: &str = "SUCCESS";
pub const TOKEN_SUMMARY: &str = "SUMMARY";

#[derive(Clone, Copy, Default)]
pub struct SummaryCounts {
    pub static_missing: usize,
    pub static_bad_image: usize,
    pub dynamic_missing: usize,
    pub runtime_loaded: usize,
    pub com_issues: usize,
}

pub fn emit(token: &str, fields: &[(String, String)]) {
    let mut line = String::with_capacity(128);
    line.push_str(token);
    for (key, value) in fields {
        line.push(' ');
        line.push_str(key);
        line.push('=');
        line.push_str(value);
    }
    println!("{line}");
}

pub fn field<K: Into<String>, V: Into<String>>(key: K, value: V) -> (String, String) {
    (key.into(), value.into())
}

pub fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

pub fn hex_u32(value: u32) -> String {
    format!("0x{value:08X}")
}

pub fn hex_usize(value: usize) -> String {
    format!("0x{value:016X}")
}

pub fn summary_fields(first_break: bool, counts: SummaryCounts) -> Vec<(String, String)> {
    vec![
        field("first_break", if first_break { "true" } else { "false" }),
        field("static_missing", counts.static_missing.to_string()),
        field("static_bad_image", counts.static_bad_image.to_string()),
        field("dynamic_missing", counts.dynamic_missing.to_string()),
        field("runtime_loaded", counts.runtime_loaded.to_string()),
        field("com_issues", counts.com_issues.to_string()),
    ]
}
