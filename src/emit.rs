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

#[cfg(test)]
mod tests {
    use super::{field, hex_u32, hex_usize, quote, summary_fields, SummaryCounts};

    #[test]
    fn quote_wraps_plain_text() {
        assert_eq!(quote("kernel32.dll"), r#""kernel32.dll""#);
    }

    #[test]
    fn quote_handles_empty_string() {
        assert_eq!(quote(""), r#""""#);
    }

    #[test]
    fn quote_escapes_backslashes() {
        assert_eq!(quote(r"C:\Windows\System32"), r#""C:\\Windows\\System32""#);
    }

    #[test]
    fn quote_escapes_double_quotes() {
        assert_eq!(quote(r#"load "quoted" dll"#), r#""load \"quoted\" dll""#);
    }

    #[test]
    fn quote_escapes_control_characters() {
        assert_eq!(quote("line\nreturn\rtab\t"), r#""line\nreturn\rtab\t""#);
    }

    #[test]
    fn quote_escapes_mixed_sequences() {
        assert_eq!(
            quote("C:\\tmp\\\"x\"\nnext\tend\r"),
            r#""C:\\tmp\\\"x\"\nnext\tend\r""#
        );
    }

    #[test]
    fn field_builds_string_tuple_from_strs() {
        assert_eq!(
            field("dll", "kernel32.dll"),
            ("dll".to_string(), "kernel32.dll".to_string())
        );
    }

    #[test]
    fn field_accepts_mixed_owned_and_borrowed_inputs() {
        let key = String::from("reason");
        assert_eq!(
            field(key, "NOT_FOUND".to_string()),
            ("reason".to_string(), "NOT_FOUND".to_string())
        );
    }

    #[test]
    fn hex_u32_formats_zero() {
        assert_eq!(hex_u32(0), "0x00000000");
    }

    #[test]
    fn hex_u32_formats_max() {
        assert_eq!(hex_u32(u32::MAX), "0xFFFFFFFF");
    }

    #[test]
    fn hex_u32_formats_typical_value() {
        assert_eq!(hex_u32(0xC000_0135), "0xC0000135");
    }

    #[test]
    fn hex_usize_formats_zero() {
        assert_eq!(hex_usize(0), "0x0000000000000000");
    }

    #[test]
    fn hex_usize_formats_typical_value() {
        assert_eq!(hex_usize(0x1234_ABCD), "0x000000001234ABCD");
    }

    #[test]
    fn hex_usize_uses_fixed_width_x64_style() {
        assert_eq!(hex_usize(0xFEDC_BA98_7654_3210usize), "0xFEDCBA9876543210");
    }

    #[test]
    fn summary_fields_reports_all_zero_counts() {
        assert_eq!(
            summary_fields(false, SummaryCounts::default()),
            vec![
                field("first_break", "false"),
                field("static_missing", "0"),
                field("static_bad_image", "0"),
                field("dynamic_missing", "0"),
                field("runtime_loaded", "0"),
                field("com_issues", "0"),
            ]
        );
    }

    #[test]
    fn summary_fields_reports_non_zero_counts() {
        assert_eq!(
            summary_fields(
                true,
                SummaryCounts {
                    static_missing: 1,
                    static_bad_image: 2,
                    dynamic_missing: 3,
                    runtime_loaded: 4,
                    com_issues: 5,
                },
            ),
            vec![
                field("first_break", "true"),
                field("static_missing", "1"),
                field("static_bad_image", "2"),
                field("dynamic_missing", "3"),
                field("runtime_loaded", "4"),
                field("com_issues", "5"),
            ]
        );
    }

    #[test]
    fn summary_fields_preserves_contract_field_order() {
        let keys: Vec<String> = summary_fields(true, SummaryCounts::default())
            .into_iter()
            .map(|(key, _)| key)
            .collect();
        assert_eq!(
            keys,
            vec![
                "first_break",
                "static_missing",
                "static_bad_image",
                "dynamic_missing",
                "runtime_loaded",
                "com_issues",
            ]
        );
    }
}
