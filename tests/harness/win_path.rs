use std::path::Path;

pub fn normalize_for_compare(input: &str) -> String {
    let layer0 = normalize_text(input);
    #[cfg(windows)]
    let layer1 = collapse_drive_alias(&layer0).unwrap_or(layer0);
    #[cfg(not(windows))]
    let layer1 = layer0;

    maybe_canonicalize(&layer1)
}

fn maybe_canonicalize(input: &str) -> String {
    let path = Path::new(input);
    if !path.exists() {
        return input.to_string();
    }

    match std::fs::canonicalize(path) {
        Ok(p) => normalize_text(&p.to_string_lossy()),
        Err(_) => input.to_string(),
    }
}

fn normalize_text(input: &str) -> String {
    let mut out = input.trim().replace('/', "\\");
    out = strip_prefixes(&out).to_string();

    if out.len() >= 2 && out.as_bytes()[1] == b':' {
        let mut chars = out.chars();
        if let Some(first) = chars.next() {
            let mut with_drive = first.to_ascii_uppercase().to_string();
            with_drive.push_str(chars.as_str());
            out = with_drive;
        }
    }

    out.to_lowercase()
}

fn strip_prefixes(input: &str) -> &str {
    if let Some(rest) = input.strip_prefix("\\\\?\\") {
        return rest;
    }
    if let Some(rest) = input.strip_prefix("\\??\\") {
        return rest;
    }
    input
}

#[cfg(windows)]
fn collapse_drive_alias(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    if bytes.len() < 3 || bytes[1] != b':' || bytes[2] != b'\\' || !bytes[0].is_ascii_alphabetic() {
        return None;
    }

    let drive = input[..2].to_ascii_uppercase();
    let mapping = query_dos_device(&drive)?;
    let mapped = mapping.strip_prefix("\\??\\")?;
    if !is_dos_path(mapped) {
        return None;
    }

    let rest = &input[2..];
    let mut rewritten = mapped.trim_end_matches('\\').to_string();
    rewritten.push_str(rest);
    Some(normalize_text(&rewritten))
}

#[cfg(windows)]
fn is_dos_path(input: &str) -> bool {
    let b = input.as_bytes();
    b.len() >= 3 && b[1] == b':' && b[2] == b'\\' && b[0].is_ascii_alphabetic()
}

#[cfg(windows)]
fn query_dos_device(drive: &str) -> Option<String> {
    use std::iter;

    const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
    const MAX_QUERY_RETRY: usize = 6;

    #[link(name = "Kernel32")]
    extern "system" {
        fn QueryDosDeviceW(lpDeviceName: *const u16, lpTargetPath: *mut u16, ucchMax: u32) -> u32;
        fn GetLastError() -> u32;
    }

    let name: Vec<u16> = drive.encode_utf16().chain(iter::once(0)).collect();
    let mut size = 1024usize;

    for _ in 0..MAX_QUERY_RETRY {
        let mut buf = vec![0u16; size];
        let copied = unsafe { QueryDosDeviceW(name.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
        if copied != 0 {
            let slice = &buf[..copied as usize];
            let first = slice
                .split(|ch| *ch == 0)
                .next()
                .filter(|part| !part.is_empty())?;
            return Some(String::from_utf16_lossy(first));
        }

        let err = unsafe { GetLastError() };
        if err != ERROR_INSUFFICIENT_BUFFER {
            return None;
        }
        size *= 2;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::normalize_for_compare;

    #[test]
    fn strips_long_path_prefix_and_normalizes_case() {
        let got = normalize_for_compare(r"\\?\C:\Foo\Bar");
        assert_eq!(got, r"c:\foo\bar");
    }

    #[test]
    fn normalizes_slashes() {
        let got = normalize_for_compare(r"C:/Foo/Bar");
        assert_eq!(got, r"c:\foo\bar");
    }

    #[test]
    fn strips_nt_prefix_variant() {
        let got = normalize_for_compare(r"\??\C:\Foo\Bar");
        assert_eq!(got, r"c:\foo\bar");
    }

    #[test]
    fn compares_case_insensitive_paths() {
        let a = normalize_for_compare(r"c:\Foo\BAR");
        let b = normalize_for_compare(r"C:\foo\bar");
        assert_eq!(a, b);
    }
}
