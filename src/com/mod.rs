// COM registration and activation-prerequisite diagnosis (spec: docs/loadwhat_spec_v2.md).
//
// Layering follows docs/com_testing_strategy.md:
// - data acquisition sits behind injectable traits (`ComRegistry`, `ComFileSystem`)
// - diagnostic logic in `resolver` is pure and exhaustively testable with mocks

pub mod fs;
pub mod manifest;
pub mod registry;
pub mod resolver;

/// Registry view selected for a COM lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegView {
    V64,
    V32,
}

impl RegView {
    pub fn as_token(&self) -> &'static str {
        match self {
            RegView::V64 => "64",
            RegView::V32 => "32",
        }
    }
}

/// Registry hive that supplied a resolved value (HKCU overrides HKLM).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hive {
    Hkcu,
    Hklm,
}

impl Hive {
    pub fn as_token(&self) -> &'static str {
        match self {
            Hive::Hkcu => "HKCU",
            Hive::Hklm => "HKLM",
        }
    }
}

/// Supported COM server registration kinds in V2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerKind {
    Inproc,
    Local,
}

impl ServerKind {
    pub fn as_token(&self) -> &'static str {
        match self {
            ServerKind::Inproc => "InprocServer32",
            ServerKind::Local => "LocalServer32",
        }
    }

    pub fn subkey(&self) -> &'static str {
        self.as_token()
    }
}

/// Lookup result state: was the CLSID or ProgID resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookupStatus {
    Registered,
    NotRegistered,
    ProgidBroken,
    TreatAsBroken,
    BrokenRegistration,
    AccessDenied,
}

impl LookupStatus {
    pub fn as_token(&self) -> &'static str {
        match self {
            LookupStatus::Registered => "REGISTERED",
            LookupStatus::NotRegistered => "NOT_REGISTERED",
            LookupStatus::ProgidBroken => "PROGID_BROKEN",
            LookupStatus::TreatAsBroken => "TREATAS_BROKEN",
            LookupStatus::BrokenRegistration => "BROKEN_REGISTRATION",
            LookupStatus::AccessDenied => "ACCESS_DENIED",
        }
    }
}

/// Server health state, separate from lookup state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerStatus {
    Ok,
    Missing,
    BadImage,
    DepsMissing,
    BitnessMismatch,
    AccessDenied,
    Skipped,
}

impl ServerStatus {
    pub fn as_token(&self) -> &'static str {
        match self {
            ServerStatus::Ok => "OK",
            ServerStatus::Missing => "SERVER_MISSING",
            ServerStatus::BadImage => "SERVER_BAD_IMAGE",
            ServerStatus::DepsMissing => "SERVER_DEPS_MISSING",
            ServerStatus::BitnessMismatch => "BITNESS_MISMATCH",
            ServerStatus::AccessDenied => "ACCESS_DENIED",
            ServerStatus::Skipped => "SKIPPED",
        }
    }
}

/// Normalizes a path string for case-insensitive comparison. Absolute-ness
/// is the caller's responsibility; this only canonicalizes case, separators,
/// and surrounding quotes/whitespace.
pub fn normalize_path_for_compare(path: &str) -> String {
    path.trim()
        .trim_matches('"')
        .replace('/', "\\")
        .to_ascii_lowercase()
}

/// Applies WOW64 file-system redirection: a 32-bit caller loading a path
/// under %SystemRoot%\System32 actually receives the %SystemRoot%\SysWOW64
/// file. Returns None when redirection does not apply.
pub fn wow64_redirect(path: &str) -> Option<String> {
    let system_root = std::env::var("SystemRoot").ok()?;
    let prefix = format!(r"{}\system32\", system_root.to_ascii_lowercase());
    let lower = path.to_ascii_lowercase();
    let rest = lower.strip_prefix(&prefix)?;
    if rest.is_empty() {
        return None;
    }
    // Prefix is ASCII, so byte offsets line up with the lowercased copy.
    Some(format!(r"{system_root}\SysWOW64\{}", &path[prefix.len()..]))
}

/// Expands %NAME% environment references in a REG_EXPAND_SZ value.
/// Unknown variables are preserved literally, matching ExpandEnvironmentStringsW.
pub fn expand_env_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match std::env::var(name) {
                    Ok(replacement) if !name.is_empty() => out.push_str(&replacement),
                    _ => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::{expand_env_value, normalize_path_for_compare};

    #[test]
    fn normalize_lowercases_and_unifies_separators() {
        assert_eq!(
            normalize_path_for_compare(r#""C:/Vendor/Foo.DLL""#),
            r"c:\vendor\foo.dll"
        );
    }

    #[test]
    fn normalize_trims_whitespace() {
        assert_eq!(normalize_path_for_compare("  C:\\a.dll  "), r"c:\a.dll");
    }

    #[test]
    fn expand_preserves_unknown_variables() {
        assert_eq!(
            expand_env_value(r"%LOADWHAT_DOES_NOT_EXIST%\x.dll"),
            r"%LOADWHAT_DOES_NOT_EXIST%\x.dll"
        );
    }

    #[test]
    fn expand_replaces_known_variables() {
        let _guard = crate::win::TEST_ENV_LOCK.lock().unwrap();
        std::env::set_var("LOADWHAT_COM_TEST_VAR", r"C:\Base");
        assert_eq!(
            expand_env_value(r"%LOADWHAT_COM_TEST_VAR%\x.dll"),
            r"C:\Base\x.dll"
        );
        std::env::remove_var("LOADWHAT_COM_TEST_VAR");
    }

    #[test]
    fn expand_keeps_unpaired_percent_literal() {
        assert_eq!(expand_env_value("100% pure"), "100% pure");
    }

    #[test]
    fn wow64_redirect_maps_system32_to_syswow64() {
        let _lock = crate::win::TEST_ENV_LOCK.lock().unwrap();
        let _guard = crate::test_util::EnvVarGuard::set("SystemRoot", r"C:\TESTWIN");
        assert_eq!(
            super::wow64_redirect(r"C:\TestWin\System32\shell32.dll").as_deref(),
            Some(r"C:\TESTWIN\SysWOW64\shell32.dll")
        );
    }

    #[test]
    fn wow64_redirect_ignores_other_paths() {
        let _lock = crate::win::TEST_ENV_LOCK.lock().unwrap();
        let _guard = crate::test_util::EnvVarGuard::set("SystemRoot", r"C:\TESTWIN");
        assert_eq!(super::wow64_redirect(r"C:\Vendor\foo.dll"), None);
        assert_eq!(super::wow64_redirect(r"C:\TESTWIN\SysWOW64\x.dll"), None);
    }
}
