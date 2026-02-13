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
