// Pure COM resolution logic over the injectable registry/file-system traits.
// Behavior contract: docs/loadwhat_spec_v2.md sections 4-7.

use std::collections::HashSet;

use super::fs::{ComFileSystem, DepFailure};
use super::manifest::{parse_manifest_com_classes, ManifestComClass};
use super::registry::{ComRegistry, RegLocation, RegValue};
use super::{
    expand_env_value, normalize_path_for_compare, Hive, LookupStatus, RegView, ServerKind,
    ServerStatus,
};
use crate::pe::MachineType;

const CLASSES_ROOT: &str = r"Software\Classes";
/// Upper bound on TreatAs/CurVer hops; visited-set cycle detection is the
/// primary guard, this only bounds pathological registries deterministically.
const MAX_CHAIN_HOPS: usize = 64;
/// Enough to cover DOS header, PE headers, and the full section table.
const HEADER_READ_BYTES: usize = 1 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryKind {
    Clsid,
    Progid,
}

impl QueryKind {
    pub fn as_token(&self) -> &'static str {
        match self {
            QueryKind::Clsid => "clsid",
            QueryKind::Progid => "progid",
        }
    }
}

/// Server health verdict plus supporting detail.
#[derive(Clone, Debug)]
pub struct ServerValidation {
    pub status: ServerStatus,
    pub machine: Option<MachineType>,
    pub failures: Vec<DepFailure>,
    pub safedll: bool,
    /// Set when WOW64 file-system redirection was applied; validation ran
    /// against this path rather than the registered one.
    pub redirected_path: Option<String>,
}

/// Resolution result for `com clsid` / `com progid`.
#[derive(Clone, Debug)]
pub struct ComLookupResult {
    pub status: LookupStatus,
    pub view: RegView,
    pub clsid: Option<String>,
    pub hive: Option<Hive>,
    pub server_kind: Option<ServerKind>,
    /// Resolved server file path (env-expanded; exe extracted for LocalServer32).
    pub server_path: Option<String>,
    /// Raw LocalServer32 command line when it differs from server_path.
    pub server_command: Option<String>,
    pub threading_model: Option<String>,
    /// ProgID registered under the terminal CLSID (trace detail).
    pub progid_of_clsid: Option<String>,
    /// ProgID chain followed via CurVer (input first).
    pub progid_chain: Vec<String>,
    /// CLSID chain followed via TreatAs (redirect targets only).
    pub treatas_chain: Vec<String>,
    pub server: Option<ServerValidation>,
}

impl ComLookupResult {
    fn new(view: RegView, status: LookupStatus) -> Self {
        ComLookupResult {
            status,
            view,
            clsid: None,
            hive: None,
            server_kind: None,
            server_path: None,
            server_command: None,
            threading_model: None,
            progid_of_clsid: None,
            progid_chain: Vec::new(),
            treatas_chain: Vec::new(),
            server: None,
        }
    }

    /// True when this result represents a definitive COM issue (exit 10).
    pub fn is_issue(&self) -> bool {
        if self.status != LookupStatus::Registered {
            return true;
        }
        match &self.server {
            Some(validation) => {
                !matches!(validation.status, ServerStatus::Ok | ServerStatus::Skipped)
            }
            None => false,
        }
    }
}

/// One reverse-lookup registration match for `com server`.
#[derive(Clone, Debug)]
pub struct ComRegistration {
    pub clsid: String,
    pub hive: Hive,
    pub view: RegView,
    pub kind: ServerKind,
    pub path: String,
    pub threading_model: Option<String>,
}

/// Manifest declaration used by `com audit`.
#[derive(Clone, Debug)]
pub struct ManifestHit {
    pub source: &'static str,
    pub file: String,
    pub decl: ManifestComClass,
    pub resolved_server: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditSource {
    Registry,
    Manifest,
}

impl AuditSource {
    pub fn as_token(&self) -> &'static str {
        match self {
            AuditSource::Registry => "registry",
            AuditSource::Manifest => "manifest",
        }
    }
}

/// Overall activation-prerequisite result for `com audit`.
#[derive(Clone, Debug)]
pub struct ComAuditResult {
    pub target_machine: MachineType,
    /// Registry view derived from the target machine type. Not part of the
    /// COM_AUDIT token (target_machine implies it); kept for diagnosis/tests.
    #[allow(dead_code)]
    pub view: RegView,
    pub source: AuditSource,
    pub status: &'static str,
    pub clsid: Option<String>,
    pub server_kind: Option<ServerKind>,
    pub server_path: Option<String>,
    pub manifest: Option<ManifestHit>,
    pub lookup: Option<ComLookupResult>,
    pub server: Option<ServerValidation>,
}

impl ComAuditResult {
    pub fn is_issue(&self) -> bool {
        self.status != "OK"
    }

    pub fn is_access_denied(&self) -> bool {
        self.status == "ACCESS_DENIED"
    }
}

/// Errors that prevent an answer entirely (exit 21/22 at the command layer).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComError {
    /// Required data was inaccessible or unreadable.
    Indeterminate(String),
    /// The target image has an unsupported machine type.
    UnsupportedArchitecture(String),
}

enum KeyState {
    Exists(Hive),
    Denied,
    Absent,
}

enum MergedRead {
    Found { text: String, hive: Hive },
    Absent,
    Denied,
}

pub struct ComResolver<'a> {
    registry: &'a dyn ComRegistry,
    fs: &'a dyn ComFileSystem,
}

impl<'a> ComResolver<'a> {
    pub fn new(registry: &'a dyn ComRegistry, fs: &'a dyn ComFileSystem) -> Self {
        ComResolver { registry, fs }
    }

    /// HKCU-over-HKLM merged key-existence probe within one view.
    fn merged_key_state(&self, view: RegView, subkey: &str) -> KeyState {
        for hive in [Hive::Hkcu, Hive::Hklm] {
            let loc = RegLocation::of(hive, view);
            if let RegValue::AccessDenied = self.registry.read_value(loc, subkey, "") {
                return KeyState::Denied;
            }
            if self.registry.key_exists(loc, subkey) {
                return KeyState::Exists(hive);
            }
        }
        KeyState::Absent
    }

    /// HKCU-over-HKLM merged string read within one view. Non-string values
    /// are treated as absent; REG_EXPAND_SZ values are env-expanded.
    fn merged_read_string(&self, view: RegView, subkey: &str, name: &str) -> MergedRead {
        for hive in [Hive::Hkcu, Hive::Hklm] {
            let loc = RegLocation::of(hive, view);
            match self.registry.read_value(loc, subkey, name) {
                RegValue::String(text) if !text.trim().is_empty() => {
                    return MergedRead::Found { text, hive };
                }
                RegValue::ExpandString(text) if !text.trim().is_empty() => {
                    return MergedRead::Found {
                        text: expand_env_value(&text),
                        hive,
                    };
                }
                RegValue::AccessDenied => return MergedRead::Denied,
                _ => {}
            }
        }
        MergedRead::Absent
    }

    fn read_string_at(&self, loc: RegLocation, subkey: &str, name: &str) -> Option<String> {
        match self.registry.read_value(loc, subkey, name) {
            RegValue::String(text) if !text.trim().is_empty() => Some(text),
            RegValue::ExpandString(text) if !text.trim().is_empty() => {
                Some(expand_env_value(&text))
            }
            _ => None,
        }
    }

    /// Resolves a CLSID per spec section 4: TreatAs traversal with cycle
    /// detection, then InprocServer32 / LocalServer32 inspection. Server
    /// validation is left to the caller (`validate_lookup_server`).
    pub fn resolve_clsid(&self, clsid: &str, view: RegView) -> ComLookupResult {
        let mut result = ComLookupResult::new(view, LookupStatus::Registered);
        let mut current = clsid.to_string();
        let mut visited: HashSet<String> = HashSet::new();
        visited.insert(current.to_ascii_uppercase());

        loop {
            let clsid_key = format!(r"{CLASSES_ROOT}\CLSID\{current}");
            match self.merged_key_state(view, &clsid_key) {
                KeyState::Denied => {
                    result.status = LookupStatus::AccessDenied;
                    return result;
                }
                KeyState::Absent => {
                    result.status = if result.treatas_chain.is_empty() {
                        LookupStatus::NotRegistered
                    } else {
                        // A TreatAs redirect pointing at an unregistered CLSID
                        // is a broken redirect chain, not a missing input.
                        LookupStatus::TreatAsBroken
                    };
                    return result;
                }
                KeyState::Exists(_) => {}
            }

            let treatas_key = format!(r"{clsid_key}\TreatAs");
            match self.merged_read_string(view, &treatas_key, "") {
                MergedRead::Denied => {
                    result.status = LookupStatus::AccessDenied;
                    return result;
                }
                MergedRead::Found { text, .. } => {
                    let next = text.trim().to_string();
                    if !visited.insert(next.to_ascii_uppercase())
                        || result.treatas_chain.len() >= MAX_CHAIN_HOPS
                    {
                        result.status = LookupStatus::TreatAsBroken;
                        return result;
                    }
                    result.treatas_chain.push(next.clone());
                    current = next;
                    continue;
                }
                MergedRead::Absent => {}
            }

            break;
        }

        result.clsid = Some(current.clone());
        let clsid_key = format!(r"{CLASSES_ROOT}\CLSID\{current}");

        for kind in [ServerKind::Inproc, ServerKind::Local] {
            let server_key = format!(r"{clsid_key}\{}", kind.subkey());
            match self.merged_read_string(view, &server_key, "") {
                MergedRead::Denied => {
                    result.status = LookupStatus::AccessDenied;
                    return result;
                }
                MergedRead::Found { text, hive } => {
                    result.hive = Some(hive);
                    result.server_kind = Some(kind);
                    let loc = RegLocation::of(hive, view);
                    result.threading_model =
                        self.read_string_at(loc, &server_key, "ThreadingModel");
                    match kind {
                        ServerKind::Inproc => {
                            result.server_path = Some(text.trim().trim_matches('"').to_string());
                        }
                        ServerKind::Local => {
                            let exe = self.extract_local_server_exe(&text);
                            if exe != text {
                                result.server_command = Some(text);
                            }
                            result.server_path = Some(exe);
                        }
                    }
                    break;
                }
                MergedRead::Absent => {}
            }
        }

        if result.server_kind.is_none() {
            // CLSID key exists but exposes no supported server subkey.
            if let KeyState::Exists(hive) = self.merged_key_state(view, &clsid_key) {
                result.hive = Some(hive);
            }
            result.status = LookupStatus::BrokenRegistration;
            return result;
        }

        result.progid_of_clsid =
            match self.merged_read_string(view, &format!(r"{clsid_key}\ProgID"), "") {
                MergedRead::Found { text, .. } => Some(text.trim().to_string()),
                _ => None,
            };

        result
    }

    /// Resolves a ProgID per spec section 4: CurVer traversal with cycle
    /// detection, terminal CLSID value read, then CLSID resolution.
    pub fn resolve_progid(&self, progid: &str, view: RegView) -> ComLookupResult {
        let mut chain = vec![progid.to_string()];
        let mut visited: HashSet<String> = HashSet::new();
        visited.insert(progid.to_ascii_uppercase());
        let mut current = progid.to_string();

        loop {
            let progid_key = format!(r"{CLASSES_ROOT}\{current}");
            match self.merged_key_state(view, &progid_key) {
                KeyState::Denied => {
                    let mut result = ComLookupResult::new(view, LookupStatus::AccessDenied);
                    result.progid_chain = chain;
                    return result;
                }
                KeyState::Absent => {
                    let mut result = ComLookupResult::new(view, LookupStatus::ProgidBroken);
                    result.progid_chain = chain;
                    return result;
                }
                KeyState::Exists(_) => {}
            }

            let curver_key = format!(r"{progid_key}\CurVer");
            match self.merged_read_string(view, &curver_key, "") {
                MergedRead::Denied => {
                    let mut result = ComLookupResult::new(view, LookupStatus::AccessDenied);
                    result.progid_chain = chain;
                    return result;
                }
                MergedRead::Found { text, .. } => {
                    let next = text.trim().to_string();
                    if next.eq_ignore_ascii_case(&current) {
                        // Self-referencing CurVer is terminal, not broken:
                        // the versioned ProgID commonly points at itself.
                    } else {
                        if !visited.insert(next.to_ascii_uppercase())
                            || chain.len() >= MAX_CHAIN_HOPS
                        {
                            let mut result = ComLookupResult::new(view, LookupStatus::ProgidBroken);
                            result.progid_chain = chain;
                            return result;
                        }
                        chain.push(next.clone());
                        current = next;
                        continue;
                    }
                }
                MergedRead::Absent => {}
            }

            break;
        }

        let clsid_key = format!(r"{CLASSES_ROOT}\{current}\CLSID");
        let clsid = match self.merged_read_string(view, &clsid_key, "") {
            MergedRead::Denied => {
                let mut result = ComLookupResult::new(view, LookupStatus::AccessDenied);
                result.progid_chain = chain;
                return result;
            }
            MergedRead::Absent => {
                let mut result = ComLookupResult::new(view, LookupStatus::ProgidBroken);
                result.progid_chain = chain;
                return result;
            }
            MergedRead::Found { text, .. } => text.trim().to_string(),
        };

        let mut result = self.resolve_clsid(&clsid, view);
        if result.status == LookupStatus::NotRegistered {
            // Missing CLSID after a ProgID chain is a broken ProgID.
            result.status = LookupStatus::ProgidBroken;
            result.clsid = Some(clsid);
        }
        result.progid_chain = chain;
        result
    }

    /// Validates the server file resolved by a lookup and attaches the verdict.
    /// `expected_machine` is the caller architecture for InprocServer32
    /// bitness checks; LocalServer32 machine differences are reported, not
    /// classified (spec section 6).
    pub fn validate_lookup_server(
        &self,
        result: &mut ComLookupResult,
        expected_machine: Option<MachineType>,
    ) -> Result<(), ComError> {
        if result.status != LookupStatus::Registered {
            return Ok(());
        }
        let (Some(path), Some(kind)) = (result.server_path.clone(), result.server_kind) else {
            return Ok(());
        };
        let validation = self.validate_server_file(&path, kind, expected_machine)?;
        result.server = Some(validation);
        Ok(())
    }

    pub fn validate_server_file(
        &self,
        path: &str,
        kind: ServerKind,
        expected_machine: Option<MachineType>,
    ) -> Result<ServerValidation, ComError> {
        // A 32-bit caller sees System32 paths redirected to SysWOW64;
        // validate what that caller would actually load.
        let redirected_path = if expected_machine == Some(MachineType::X86) {
            super::wow64_redirect(path)
        } else {
            None
        };
        let path = redirected_path.as_deref().unwrap_or(path);

        let done = |status: ServerStatus, machine: Option<MachineType>| ServerValidation {
            status,
            machine,
            failures: Vec::new(),
            safedll: false,
            redirected_path: redirected_path.clone(),
        };

        if !self.fs.file_exists(path) {
            return Ok(done(ServerStatus::Missing, None));
        }

        let Some(header) = self.fs.read_file_header(path, HEADER_READ_BYTES) else {
            return Ok(done(ServerStatus::AccessDenied, None));
        };

        let machine = match crate::pe::machine_type_from_bytes(&header) {
            Ok(machine) => machine,
            Err(_) => return Ok(done(ServerStatus::BadImage, None)),
        };

        if kind == ServerKind::Inproc {
            if let Some(expected) = expected_machine {
                if machine != MachineType::Unknown
                    && expected != MachineType::Unknown
                    && machine != expected
                {
                    return Ok(done(ServerStatus::BitnessMismatch, Some(machine)));
                }
            }
        }

        // The v1 dependency walk models the x64 loader; walking an x86 image
        // with x64 search semantics would fabricate results.
        if machine == MachineType::X86 {
            return Ok(done(ServerStatus::Skipped, Some(machine)));
        }

        let walk = self
            .fs
            .walk_dependencies(path)
            .map_err(ComError::Indeterminate)?;

        Ok(ServerValidation {
            status: if walk.failures.is_empty() {
                ServerStatus::Ok
            } else {
                ServerStatus::DepsMissing
            },
            machine: Some(machine),
            failures: walk.failures,
            safedll: walk.safedll,
            redirected_path,
        })
    }

    /// Extracts the executable path from a LocalServer32 command line.
    /// Quoted commands take the quoted prefix; unquoted commands try
    /// progressively longer space-delimited prefixes against the file system
    /// (CreateProcess semantics), falling back to the first token.
    fn extract_local_server_exe(&self, command: &str) -> String {
        let trimmed = command.trim();
        if let Some(rest) = trimmed.strip_prefix('"') {
            return match rest.find('"') {
                Some(end) => rest[..end].to_string(),
                None => rest.to_string(),
            };
        }

        for (idx, ch) in trimmed.char_indices() {
            if ch == ' ' && self.fs.file_exists(&trimmed[..idx]) {
                return trimmed[..idx].to_string();
            }
        }
        if self.fs.file_exists(trimmed) {
            return trimmed.to_string();
        }
        trimmed.split(' ').next().unwrap_or(trimmed).to_string()
    }

    /// Reverse lookup for `com server`: scans CLSID registrations in the given
    /// views for servers matching `input_path` (already absolute). Output
    /// order is deterministic: view (as given), hive (HKCU then HKLM), CLSID
    /// lexicographic, then InprocServer32 before LocalServer32.
    pub fn reverse_lookup(
        &self,
        input_path: &str,
        views: &[RegView],
    ) -> Result<Vec<ComRegistration>, ComError> {
        let target = normalize_path_for_compare(input_path);
        let mut matches = Vec::new();

        for &view in views {
            for hive in [Hive::Hkcu, Hive::Hklm] {
                let loc = RegLocation::of(hive, view);
                let clsid_root = format!(r"{CLASSES_ROOT}\CLSID");
                let mut clsids = match self.registry.enum_subkeys(loc, &clsid_root) {
                    Ok(names) => names,
                    Err(crate::win::ERROR_ACCESS_DENIED) => {
                        return Err(ComError::Indeterminate(format!(
                            "access denied enumerating {} {} CLSID registrations",
                            hive.as_token(),
                            view.as_token()
                        )));
                    }
                    Err(_) => Vec::new(),
                };
                clsids.sort();

                for clsid in clsids {
                    for kind in [ServerKind::Inproc, ServerKind::Local] {
                        let server_key = format!(r"{clsid_root}\{clsid}\{}", kind.subkey());
                        let Some(raw) = self.read_string_at(loc, &server_key, "") else {
                            continue;
                        };
                        let candidate = match kind {
                            ServerKind::Inproc => raw.trim().trim_matches('"').to_string(),
                            ServerKind::Local => self.extract_local_server_exe(&raw),
                        };
                        if normalize_path_for_compare(&candidate) == target {
                            matches.push(ComRegistration {
                                clsid: clsid.clone(),
                                hive,
                                view,
                                kind,
                                path: candidate,
                                threading_model: self.read_string_at(
                                    loc,
                                    &server_key,
                                    "ThreadingModel",
                                ),
                            });
                        }
                    }
                }
            }
        }

        Ok(matches)
    }

    /// Target-scoped audit per spec section 4: derive the registry view from
    /// the target machine type, prefer manifest declarations, fall back to
    /// registry resolution, and validate the resolved server.
    pub fn audit(
        &self,
        target_path: &str,
        query: &str,
        query_kind: QueryKind,
    ) -> Result<ComAuditResult, ComError> {
        if !self.fs.file_exists(target_path) {
            return Err(ComError::Indeterminate(format!(
                "target does not exist: {target_path}"
            )));
        }
        let Some(header) = self.fs.read_file_header(target_path, HEADER_READ_BYTES) else {
            return Err(ComError::Indeterminate(format!(
                "failed to read target: {target_path}"
            )));
        };
        let target_machine = crate::pe::machine_type_from_bytes(&header)
            .map_err(|e| ComError::Indeterminate(format!("target is not a valid PE image: {e}")))?;

        let view = match target_machine {
            MachineType::X64 => RegView::V64,
            MachineType::X86 => RegView::V32,
            MachineType::Unknown => {
                return Err(ComError::UnsupportedArchitecture(format!(
                    "unsupported target machine type for {target_path}"
                )));
            }
        };

        if let Some(hit) = self.manifest_hit(target_path, query, query_kind) {
            return self.audit_from_manifest(target_machine, view, hit);
        }

        let mut lookup = match query_kind {
            QueryKind::Clsid => self.resolve_clsid(query, view),
            QueryKind::Progid => self.resolve_progid(query, view),
        };
        self.validate_lookup_server(&mut lookup, Some(target_machine))?;

        let status = if lookup.status != LookupStatus::Registered {
            lookup.status.as_token()
        } else {
            match &lookup.server {
                Some(validation) => match validation.status {
                    ServerStatus::Ok | ServerStatus::Skipped => "OK",
                    other => other.as_token(),
                },
                None => "OK",
            }
        };

        Ok(ComAuditResult {
            target_machine,
            view,
            source: AuditSource::Registry,
            status,
            clsid: lookup.clsid.clone(),
            server_kind: lookup.server_kind,
            server_path: lookup.server_path.clone(),
            manifest: None,
            server: lookup.server.clone(),
            lookup: Some(lookup),
        })
    }

    fn manifest_hit(
        &self,
        target_path: &str,
        query: &str,
        query_kind: QueryKind,
    ) -> Option<ManifestHit> {
        let (source, file, xml) = if let Some(xml) = self.fs.embedded_manifest(target_path) {
            ("embedded", target_path.to_string(), xml)
        } else {
            let sidecar = format!("{target_path}.manifest");
            if !self.fs.file_exists(&sidecar) {
                return None;
            }
            let bytes = self.fs.read_file_header(&sidecar, HEADER_READ_BYTES)?;
            let xml = String::from_utf8_lossy(&bytes).into_owned();
            ("sidecar", sidecar, xml)
        };

        let decl = parse_manifest_com_classes(&xml)
            .into_iter()
            .find(|class| match query_kind {
                QueryKind::Clsid => class.clsid.eq_ignore_ascii_case(query),
                QueryKind::Progid => class
                    .progid
                    .as_deref()
                    .is_some_and(|p| p.eq_ignore_ascii_case(query)),
            })?;

        let resolved_server = decl.server_dll.as_deref().map(|dll| {
            if dll.contains(':') || dll.starts_with('\\') {
                dll.to_string()
            } else {
                let dir = match target_path.rfind('\\') {
                    Some(idx) => &target_path[..idx],
                    None => "",
                };
                format!("{dir}\\{dll}")
            }
        });

        Some(ManifestHit {
            source,
            file,
            decl,
            resolved_server,
        })
    }

    fn audit_from_manifest(
        &self,
        target_machine: MachineType,
        view: RegView,
        hit: ManifestHit,
    ) -> Result<ComAuditResult, ComError> {
        let clsid = Some(hit.decl.clsid.clone());
        let Some(server_path) = hit.resolved_server.clone() else {
            // A manifest declaration without a server file cannot activate.
            return Ok(ComAuditResult {
                target_machine,
                view,
                source: AuditSource::Manifest,
                status: "BROKEN_REGISTRATION",
                clsid,
                server_kind: None,
                server_path: None,
                manifest: Some(hit),
                lookup: None,
                server: None,
            });
        };

        // Registration-free COM servers load in-process.
        let validation =
            self.validate_server_file(&server_path, ServerKind::Inproc, Some(target_machine))?;
        let status = match validation.status {
            ServerStatus::Ok | ServerStatus::Skipped => "OK",
            other => other.as_token(),
        };

        Ok(ComAuditResult {
            target_machine,
            view,
            source: AuditSource::Manifest,
            status,
            clsid,
            server_kind: Some(ServerKind::Inproc),
            server_path: Some(server_path),
            manifest: Some(hit),
            lookup: None,
            server: Some(validation),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::fs::{DepStatus, MockFileSystem};
    use super::super::registry::{MockRegistry, RegLocation, RegValue};
    use super::*;

    const MACHINE_X86: u16 = 0x014C;

    fn set_str(reg: &mut MockRegistry, loc: RegLocation, subkey: &str, name: &str, value: &str) {
        reg.set(loc, subkey, name, RegValue::String(value.to_string()));
    }

    fn set_inproc(reg: &mut MockRegistry, loc: RegLocation, clsid: &str, path: &str) {
        set_str(
            reg,
            loc,
            &format!(r"Software\Classes\CLSID\{clsid}\InprocServer32"),
            "",
            path,
        );
    }

    fn resolver_parts() -> (MockRegistry, MockFileSystem) {
        (MockRegistry::new(), MockFileSystem::new())
    }

    // ---- CLSID resolution ----

    #[test]
    fn clsid_basic_inprocserver32() {
        let (mut reg, fs) = resolver_parts();
        set_inproc(
            &mut reg,
            RegLocation::Hklm64,
            "{TEST-0001}",
            r"C:\Vendor\server.dll",
        );
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\CLSID\{TEST-0001}\InprocServer32",
            "ThreadingModel",
            "Both",
        );

        let resolver = ComResolver::new(&reg, &fs);
        let result = resolver.resolve_clsid("{TEST-0001}", RegView::V64);

        assert_eq!(result.status, LookupStatus::Registered);
        assert_eq!(result.clsid.as_deref(), Some("{TEST-0001}"));
        assert_eq!(result.hive, Some(Hive::Hklm));
        assert_eq!(result.server_kind, Some(ServerKind::Inproc));
        assert_eq!(result.server_path.as_deref(), Some(r"C:\Vendor\server.dll"));
        assert_eq!(result.threading_model.as_deref(), Some("Both"));
    }

    #[test]
    fn clsid_basic_localserver32_with_quoted_command() {
        let (mut reg, fs) = resolver_parts();
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\CLSID\{TEST-0002}\LocalServer32",
            "",
            r#""C:\Program Files\app.exe" /Embedding"#,
        );

        let resolver = ComResolver::new(&reg, &fs);
        let result = resolver.resolve_clsid("{TEST-0002}", RegView::V64);

        assert_eq!(result.status, LookupStatus::Registered);
        assert_eq!(result.server_kind, Some(ServerKind::Local));
        assert_eq!(
            result.server_path.as_deref(),
            Some(r"C:\Program Files\app.exe")
        );
        assert_eq!(
            result.server_command.as_deref(),
            Some(r#""C:\Program Files\app.exe" /Embedding"#)
        );
    }

    #[test]
    fn localserver32_unquoted_path_with_arguments() {
        let (mut reg, mut fs) = resolver_parts();
        fs.add_pe(r"C:\tools\app.exe", &[]);
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\CLSID\{TEST-0003}\LocalServer32",
            "",
            r"C:\tools\app.exe /flag",
        );

        let resolver = ComResolver::new(&reg, &fs);
        let result = resolver.resolve_clsid("{TEST-0003}", RegView::V64);

        assert_eq!(result.server_path.as_deref(), Some(r"C:\tools\app.exe"));
    }

    #[test]
    fn localserver32_unquoted_spaced_path_resolves_via_progressive_prefixes() {
        let (mut reg, mut fs) = resolver_parts();
        fs.add_pe(r"C:\Program Files\app.exe", &[]);
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\CLSID\{TEST-0004}\LocalServer32",
            "",
            r"C:\Program Files\app.exe /Embedding",
        );

        let resolver = ComResolver::new(&reg, &fs);
        let result = resolver.resolve_clsid("{TEST-0004}", RegView::V64);

        assert_eq!(
            result.server_path.as_deref(),
            Some(r"C:\Program Files\app.exe")
        );
    }

    #[test]
    fn localserver32_bare_path_without_arguments() {
        let (mut reg, fs) = resolver_parts();
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\CLSID\{TEST-0005}\LocalServer32",
            "",
            r"C:\tools\app.exe",
        );

        let resolver = ComResolver::new(&reg, &fs);
        let result = resolver.resolve_clsid("{TEST-0005}", RegView::V64);

        assert_eq!(result.server_path.as_deref(), Some(r"C:\tools\app.exe"));
        assert_eq!(result.server_command, None);
    }

    #[test]
    fn clsid_not_registered() {
        let (reg, fs) = resolver_parts();
        let resolver = ComResolver::new(&reg, &fs);
        let result = resolver.resolve_clsid("{MISSING}", RegView::V64);
        assert_eq!(result.status, LookupStatus::NotRegistered);
    }

    #[test]
    fn empty_clsid_is_not_registered_without_panic() {
        let (reg, fs) = resolver_parts();
        let resolver = ComResolver::new(&reg, &fs);
        assert_eq!(
            resolver.resolve_clsid("", RegView::V64).status,
            LookupStatus::NotRegistered
        );
    }

    #[test]
    fn clsid_without_server_subkey_is_broken_registration() {
        let (mut reg, fs) = resolver_parts();
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\CLSID\{TEST-0006}",
            "",
            "Friendly Name",
        );

        let resolver = ComResolver::new(&reg, &fs);
        let result = resolver.resolve_clsid("{TEST-0006}", RegView::V64);

        assert_eq!(result.status, LookupStatus::BrokenRegistration);
        assert_eq!(result.clsid.as_deref(), Some("{TEST-0006}"));
    }

    // ---- TreatAs ----

    #[test]
    fn treatas_redirect_resolves_target_server() {
        let (mut reg, fs) = resolver_parts();
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\CLSID\{OLD}\TreatAs",
            "",
            "{NEW}",
        );
        set_inproc(&mut reg, RegLocation::Hklm64, "{NEW}", r"C:\Vendor\new.dll");

        let resolver = ComResolver::new(&reg, &fs);
        let result = resolver.resolve_clsid("{OLD}", RegView::V64);

        assert_eq!(result.status, LookupStatus::Registered);
        assert_eq!(result.clsid.as_deref(), Some("{NEW}"));
        assert_eq!(result.treatas_chain, vec!["{NEW}".to_string()]);
    }

    #[test]
    fn treatas_deep_chain_resolves() {
        let (mut reg, fs) = resolver_parts();
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\CLSID\{A}\TreatAs",
            "",
            "{B}",
        );
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\CLSID\{B}\TreatAs",
            "",
            "{C}",
        );
        set_inproc(&mut reg, RegLocation::Hklm64, "{C}", r"C:\Vendor\c.dll");

        let resolver = ComResolver::new(&reg, &fs);
        let result = resolver.resolve_clsid("{A}", RegView::V64);

        assert_eq!(result.status, LookupStatus::Registered);
        assert_eq!(result.clsid.as_deref(), Some("{C}"));
        assert_eq!(result.treatas_chain.len(), 2);
    }

    #[test]
    fn treatas_cycle_returns_broken() {
        let (mut reg, fs) = resolver_parts();
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\CLSID\{CYCLE-A}\TreatAs",
            "",
            "{CYCLE-B}",
        );
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\CLSID\{CYCLE-B}\TreatAs",
            "",
            "{CYCLE-A}",
        );

        let resolver = ComResolver::new(&reg, &fs);
        let result = resolver.resolve_clsid("{CYCLE-A}", RegView::V64);

        assert_eq!(result.status, LookupStatus::TreatAsBroken);
    }

    #[test]
    fn treatas_to_unregistered_clsid_is_broken() {
        let (mut reg, fs) = resolver_parts();
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\CLSID\{OLD}\TreatAs",
            "",
            "{GONE}",
        );

        let resolver = ComResolver::new(&reg, &fs);
        let result = resolver.resolve_clsid("{OLD}", RegView::V64);

        assert_eq!(result.status, LookupStatus::TreatAsBroken);
    }

    // ---- Hive merge and views ----

    #[test]
    fn hkcu_overrides_hklm_for_same_clsid() {
        let (mut reg, fs) = resolver_parts();
        set_inproc(
            &mut reg,
            RegLocation::Hklm64,
            "{OVERRIDE}",
            r"C:\Old\server.dll",
        );
        set_inproc(
            &mut reg,
            RegLocation::Hkcu64,
            "{OVERRIDE}",
            r"C:\New\server.dll",
        );

        let resolver = ComResolver::new(&reg, &fs);
        let result = resolver.resolve_clsid("{OVERRIDE}", RegView::V64);

        assert_eq!(result.status, LookupStatus::Registered);
        assert_eq!(result.hive, Some(Hive::Hkcu));
        assert_eq!(result.server_path.as_deref(), Some(r"C:\New\server.dll"));
    }

    #[test]
    fn hklm_fallback_when_hkcu_absent() {
        let (mut reg, fs) = resolver_parts();
        set_inproc(
            &mut reg,
            RegLocation::Hklm64,
            "{HKLM-ONLY}",
            r"C:\M\server.dll",
        );

        let resolver = ComResolver::new(&reg, &fs);
        let result = resolver.resolve_clsid("{HKLM-ONLY}", RegView::V64);

        assert_eq!(result.status, LookupStatus::Registered);
        assert_eq!(result.hive, Some(Hive::Hklm));
    }

    #[test]
    fn hkcu_only_registration_is_found() {
        let (mut reg, fs) = resolver_parts();
        set_inproc(
            &mut reg,
            RegLocation::Hkcu64,
            "{HKCU-ONLY}",
            r"C:\U\server.dll",
        );

        let resolver = ComResolver::new(&reg, &fs);
        let result = resolver.resolve_clsid("{HKCU-ONLY}", RegView::V64);

        assert_eq!(result.status, LookupStatus::Registered);
        assert_eq!(result.hive, Some(Hive::Hkcu));
    }

    #[test]
    fn views_are_isolated() {
        let (mut reg, fs) = resolver_parts();
        set_inproc(
            &mut reg,
            RegLocation::Hklm64,
            "{VIEWED}",
            r"C:\x64\server.dll",
        );
        set_inproc(
            &mut reg,
            RegLocation::Hklm32,
            "{VIEWED}",
            r"C:\x86\server.dll",
        );

        let resolver = ComResolver::new(&reg, &fs);
        let v64 = resolver.resolve_clsid("{VIEWED}", RegView::V64);
        let v32 = resolver.resolve_clsid("{VIEWED}", RegView::V32);

        assert_eq!(v64.server_path.as_deref(), Some(r"C:\x64\server.dll"));
        assert_eq!(v32.server_path.as_deref(), Some(r"C:\x86\server.dll"));
    }

    #[test]
    fn clsid_only_in_other_view_is_not_registered() {
        let (mut reg, fs) = resolver_parts();
        set_inproc(
            &mut reg,
            RegLocation::Hklm64,
            "{V64-ONLY}",
            r"C:\x64\server.dll",
        );

        let resolver = ComResolver::new(&reg, &fs);
        assert_eq!(
            resolver.resolve_clsid("{V64-ONLY}", RegView::V32).status,
            LookupStatus::NotRegistered
        );
    }

    #[test]
    fn expand_sz_server_path_is_expanded() {
        let _guard = crate::win::TEST_ENV_LOCK.lock().unwrap();
        std::env::set_var("LOADWHAT_COM_RESOLVER_TEST_BASE", r"C:\Expanded");
        let (mut reg, fs) = resolver_parts();
        reg.set(
            RegLocation::Hklm64,
            r"Software\Classes\CLSID\{EXPAND}\InprocServer32",
            "",
            RegValue::ExpandString(r"%LOADWHAT_COM_RESOLVER_TEST_BASE%\server.dll".to_string()),
        );

        let resolver = ComResolver::new(&reg, &fs);
        let result = resolver.resolve_clsid("{EXPAND}", RegView::V64);
        std::env::remove_var("LOADWHAT_COM_RESOLVER_TEST_BASE");

        assert_eq!(
            result.server_path.as_deref(),
            Some(r"C:\Expanded\server.dll")
        );
    }

    // ---- ProgID resolution ----

    #[test]
    fn progid_simple_resolution() {
        let (mut reg, fs) = resolver_parts();
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\Vendor.Widget\CLSID",
            "",
            "{WIDGET}",
        );
        set_inproc(
            &mut reg,
            RegLocation::Hklm64,
            "{WIDGET}",
            r"C:\Vendor\widget.dll",
        );

        let resolver = ComResolver::new(&reg, &fs);
        let result = resolver.resolve_progid("Vendor.Widget", RegView::V64);

        assert_eq!(result.status, LookupStatus::Registered);
        assert_eq!(result.clsid.as_deref(), Some("{WIDGET}"));
        assert_eq!(result.hive, Some(Hive::Hklm));
        assert_eq!(result.server_kind, Some(ServerKind::Inproc));
    }

    #[test]
    fn progid_resolves_through_curver_to_clsid() {
        let (mut reg, fs) = resolver_parts();
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\Vendor.Widget\CurVer",
            "",
            "Vendor.Widget.3",
        );
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\Vendor.Widget.3\CLSID",
            "",
            "{AAAA-BBBB-CCCC-DDDD}",
        );
        set_inproc(
            &mut reg,
            RegLocation::Hklm64,
            "{AAAA-BBBB-CCCC-DDDD}",
            r"C:\Vendor\widget.dll",
        );

        let resolver = ComResolver::new(&reg, &fs);
        let result = resolver.resolve_progid("Vendor.Widget", RegView::V64);

        assert_eq!(result.status, LookupStatus::Registered);
        assert_eq!(result.clsid.as_deref(), Some("{AAAA-BBBB-CCCC-DDDD}"));
        assert_eq!(
            result.progid_chain,
            vec!["Vendor.Widget".to_string(), "Vendor.Widget.3".to_string()]
        );
    }

    #[test]
    fn progid_multi_hop_curver_chain() {
        let (mut reg, fs) = resolver_parts();
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\P.A\CurVer",
            "",
            "P.B",
        );
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\P.B\CurVer",
            "",
            "P.C",
        );
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\P.C\CLSID",
            "",
            "{P}",
        );
        set_inproc(&mut reg, RegLocation::Hklm64, "{P}", r"C:\p.dll");

        let resolver = ComResolver::new(&reg, &fs);
        let result = resolver.resolve_progid("P.A", RegView::V64);

        assert_eq!(result.status, LookupStatus::Registered);
        assert_eq!(result.progid_chain.len(), 3);
    }

    #[test]
    fn progid_curver_cycle_is_broken() {
        let (mut reg, fs) = resolver_parts();
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\P.A\CurVer",
            "",
            "P.B",
        );
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\P.B\CurVer",
            "",
            "P.A",
        );

        let resolver = ComResolver::new(&reg, &fs);
        assert_eq!(
            resolver.resolve_progid("P.A", RegView::V64).status,
            LookupStatus::ProgidBroken
        );
    }

    #[test]
    fn progid_self_referencing_curver_is_terminal() {
        let (mut reg, fs) = resolver_parts();
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\P.X\CurVer",
            "",
            "P.X",
        );
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\P.X\CLSID",
            "",
            "{PX}",
        );
        set_inproc(&mut reg, RegLocation::Hklm64, "{PX}", r"C:\px.dll");

        let resolver = ComResolver::new(&reg, &fs);
        assert_eq!(
            resolver.resolve_progid("P.X", RegView::V64).status,
            LookupStatus::Registered
        );
    }

    #[test]
    fn missing_progid_is_broken() {
        let (reg, fs) = resolver_parts();
        let resolver = ComResolver::new(&reg, &fs);
        assert_eq!(
            resolver
                .resolve_progid("No.Such.ProgId", RegView::V64)
                .status,
            LookupStatus::ProgidBroken
        );
    }

    #[test]
    fn progid_without_clsid_value_is_broken() {
        let (mut reg, fs) = resolver_parts();
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\P.NoClsid",
            "",
            "desc",
        );

        let resolver = ComResolver::new(&reg, &fs);
        assert_eq!(
            resolver.resolve_progid("P.NoClsid", RegView::V64).status,
            LookupStatus::ProgidBroken
        );
    }

    #[test]
    fn progid_pointing_at_missing_clsid_is_broken() {
        let (mut reg, fs) = resolver_parts();
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\P.Gone\CLSID",
            "",
            "{GONE}",
        );

        let resolver = ComResolver::new(&reg, &fs);
        let result = resolver.resolve_progid("P.Gone", RegView::V64);

        assert_eq!(result.status, LookupStatus::ProgidBroken);
        assert_eq!(result.clsid.as_deref(), Some("{GONE}"));
    }

    // ---- Access denied ----

    #[test]
    fn access_denied_on_clsid_key() {
        let (mut reg, fs) = resolver_parts();
        set_inproc(
            &mut reg,
            RegLocation::Hklm64,
            "{DENIED}",
            r"C:\Vendor\server.dll",
        );
        reg.deny_access(RegLocation::Hklm64, r"Software\Classes\CLSID\{DENIED}");

        let resolver = ComResolver::new(&reg, &fs);
        assert_eq!(
            resolver.resolve_clsid("{DENIED}", RegView::V64).status,
            LookupStatus::AccessDenied
        );
    }

    #[test]
    fn access_denied_on_partial_progid_chain() {
        let (mut reg, fs) = resolver_parts();
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\P.D\CLSID",
            "",
            "{D}",
        );
        set_inproc(&mut reg, RegLocation::Hklm64, "{D}", r"C:\d.dll");
        reg.deny_access(RegLocation::Hklm64, r"Software\Classes\CLSID\{D}");

        let resolver = ComResolver::new(&reg, &fs);
        assert_eq!(
            resolver.resolve_progid("P.D", RegView::V64).status,
            LookupStatus::AccessDenied
        );
    }

    // ---- Server validation ----

    fn registered_with_server(server_path: &str) -> (MockRegistry, MockFileSystem) {
        let mut reg = MockRegistry::new();
        set_inproc(&mut reg, RegLocation::Hklm64, "{SRV}", server_path);
        (reg, MockFileSystem::new())
    }

    #[test]
    fn server_valid_pe_reports_ok() {
        let (reg, mut fs) = registered_with_server(r"C:\Vendor\server.dll");
        fs.add_pe(r"C:\Vendor\server.dll", &[]);

        let resolver = ComResolver::new(&reg, &fs);
        let mut result = resolver.resolve_clsid("{SRV}", RegView::V64);
        resolver
            .validate_lookup_server(&mut result, Some(MachineType::X64))
            .unwrap();

        assert!(!result.is_issue());
        let server = result.server.unwrap();
        assert_eq!(server.status, ServerStatus::Ok);
        assert_eq!(server.machine, Some(MachineType::X64));
    }

    #[test]
    fn server_missing_file() {
        let (reg, fs) = registered_with_server(r"C:\Vendor\server.dll");

        let resolver = ComResolver::new(&reg, &fs);
        let mut result = resolver.resolve_clsid("{SRV}", RegView::V64);
        resolver
            .validate_lookup_server(&mut result, Some(MachineType::X64))
            .unwrap();

        assert!(result.is_issue());
        assert_eq!(result.server.unwrap().status, ServerStatus::Missing);
    }

    #[test]
    fn server_bad_image() {
        let (reg, mut fs) = registered_with_server(r"C:\Vendor\server.dll");
        fs.add_raw(r"C:\Vendor\server.dll", vec![0xDE, 0xAD, 0xBE, 0xEF]);

        let resolver = ComResolver::new(&reg, &fs);
        let mut result = resolver.resolve_clsid("{SRV}", RegView::V64);
        resolver
            .validate_lookup_server(&mut result, Some(MachineType::X64))
            .unwrap();

        assert_eq!(result.server.unwrap().status, ServerStatus::BadImage);
    }

    #[test]
    fn inproc_x86_server_for_x64_caller_is_bitness_mismatch() {
        let (reg, mut fs) = registered_with_server(r"C:\Vendor\server.dll");
        fs.add_pe_with_machine(r"C:\Vendor\server.dll", MACHINE_X86, &[]);

        let resolver = ComResolver::new(&reg, &fs);
        let mut result = resolver.resolve_clsid("{SRV}", RegView::V64);
        resolver
            .validate_lookup_server(&mut result, Some(MachineType::X64))
            .unwrap();

        let server = result.server.unwrap();
        assert_eq!(server.status, ServerStatus::BitnessMismatch);
        assert_eq!(server.machine, Some(MachineType::X86));
    }

    #[test]
    fn server_with_missing_direct_dependency() {
        let (reg, mut fs) = registered_with_server(r"C:\Vendor\server.dll");
        fs.add_pe(r"C:\Vendor\server.dll", &["helper.dll"]);

        let resolver = ComResolver::new(&reg, &fs);
        let mut result = resolver.resolve_clsid("{SRV}", RegView::V64);
        resolver
            .validate_lookup_server(&mut result, Some(MachineType::X64))
            .unwrap();

        let server = result.server.unwrap();
        assert_eq!(server.status, ServerStatus::DepsMissing);
        assert_eq!(server.failures.len(), 1);
        assert_eq!(server.failures[0].dll, "helper.dll");
        assert_eq!(server.failures[0].status, DepStatus::Missing);
        assert_eq!(server.failures[0].depth, 1);
    }

    #[test]
    fn server_with_missing_transitive_dependency() {
        let (reg, mut fs) = registered_with_server(r"C:\Vendor\server.dll");
        fs.add_pe(r"C:\Vendor\server.dll", &["helper.dll"]);
        fs.add_pe(r"C:\Vendor\helper.dll", &["leaf.dll"]);

        let resolver = ComResolver::new(&reg, &fs);
        let mut result = resolver.resolve_clsid("{SRV}", RegView::V64);
        resolver
            .validate_lookup_server(&mut result, Some(MachineType::X64))
            .unwrap();

        let server = result.server.unwrap();
        assert_eq!(server.status, ServerStatus::DepsMissing);
        assert_eq!(server.failures.len(), 1);
        assert_eq!(server.failures[0].dll, "leaf.dll");
        assert_eq!(server.failures[0].via, "helper.dll");
        assert_eq!(server.failures[0].depth, 2);
    }

    #[test]
    fn server_with_bad_image_dependency() {
        let (reg, mut fs) = registered_with_server(r"C:\Vendor\server.dll");
        fs.add_pe(r"C:\Vendor\server.dll", &["corrupt.dll"]);
        fs.add_raw(r"C:\Vendor\corrupt.dll", vec![0u8; 64]);

        let resolver = ComResolver::new(&reg, &fs);
        let mut result = resolver.resolve_clsid("{SRV}", RegView::V64);
        resolver
            .validate_lookup_server(&mut result, Some(MachineType::X64))
            .unwrap();

        let server = result.server.unwrap();
        assert_eq!(server.status, ServerStatus::DepsMissing);
        assert_eq!(server.failures[0].status, DepStatus::BadImage);
    }

    #[test]
    fn x86_server_for_x86_caller_skips_dependency_walk() {
        let (reg, mut fs) = registered_with_server(r"C:\Vendor\server.dll");
        fs.add_pe_with_machine(r"C:\Vendor\server.dll", MACHINE_X86, &[]);

        let resolver = ComResolver::new(&reg, &fs);
        let mut result = resolver.resolve_clsid("{SRV}", RegView::V64);
        resolver
            .validate_lookup_server(&mut result, Some(MachineType::X86))
            .unwrap();

        assert!(!result.is_issue());
        let server = result.server.unwrap();
        assert_eq!(server.status, ServerStatus::Skipped);
    }

    #[test]
    fn x86_expected_caller_validates_wow64_redirected_file() {
        let _lock = crate::win::TEST_ENV_LOCK.lock().unwrap();
        let _guard = crate::test_util::EnvVarGuard::set("SystemRoot", r"C:\TESTWIN");

        let mut reg = MockRegistry::new();
        let mut fs = MockFileSystem::new();
        set_inproc(
            &mut reg,
            RegLocation::Hklm32,
            "{WOW}",
            r"C:\TESTWIN\system32\srv.dll",
        );
        // Only the SysWOW64 copy exists (as an x86 image), mirroring a real
        // 64-bit Windows: the registered System32 path is what a 32-bit
        // caller would be redirected away from.
        fs.add_pe_with_machine(r"C:\TESTWIN\SysWOW64\srv.dll", MACHINE_X86, &[]);

        let resolver = ComResolver::new(&reg, &fs);
        let mut result = resolver.resolve_clsid("{WOW}", RegView::V32);
        resolver
            .validate_lookup_server(&mut result, Some(MachineType::X86))
            .unwrap();

        assert!(!result.is_issue());
        let server = result.server.unwrap();
        assert_eq!(server.status, ServerStatus::Skipped);
        assert_eq!(
            server.redirected_path.as_deref(),
            Some(r"C:\TESTWIN\SysWOW64\srv.dll")
        );
    }

    // ---- Reverse lookup ----

    #[test]
    fn reverse_lookup_orders_deterministically() {
        let (mut reg, mut fs) = resolver_parts();
        fs.add_pe(r"C:\Vendor\foo.dll", &[]);
        set_inproc(&mut reg, RegLocation::Hklm64, "{ZZZ}", r"C:\Vendor\FOO.DLL");
        set_inproc(&mut reg, RegLocation::Hklm64, "{AAA}", r"c:\vendor\foo.dll");
        set_inproc(&mut reg, RegLocation::Hkcu64, "{MMM}", r"C:\Vendor\foo.dll");
        set_inproc(&mut reg, RegLocation::Hklm32, "{K32}", r"C:\Vendor\foo.dll");
        set_inproc(
            &mut reg,
            RegLocation::Hklm64,
            "{OTHER}",
            r"C:\Other\bar.dll",
        );

        let resolver = ComResolver::new(&reg, &fs);
        let regs = resolver
            .reverse_lookup(r"C:\Vendor\foo.dll", &[RegView::V64, RegView::V32])
            .unwrap();

        let order: Vec<(String, Hive, RegView)> = regs
            .iter()
            .map(|r| (r.clsid.clone(), r.hive, r.view))
            .collect();
        assert_eq!(
            order,
            vec![
                ("{MMM}".to_string(), Hive::Hkcu, RegView::V64),
                ("{AAA}".to_string(), Hive::Hklm, RegView::V64),
                ("{ZZZ}".to_string(), Hive::Hklm, RegView::V64),
                ("{K32}".to_string(), Hive::Hklm, RegView::V32),
            ]
        );
    }

    #[test]
    fn reverse_lookup_matches_localserver_command_lines() {
        let (mut reg, mut fs) = resolver_parts();
        fs.add_pe(r"C:\tools\app.exe", &[]);
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\CLSID\{LOCAL}\LocalServer32",
            "",
            r#""C:\tools\app.exe" /Embedding"#,
        );

        let resolver = ComResolver::new(&reg, &fs);
        let regs = resolver
            .reverse_lookup(r"C:\tools\app.exe", &[RegView::V64])
            .unwrap();

        assert_eq!(regs.len(), 1);
        assert_eq!(regs[0].kind, ServerKind::Local);
        assert_eq!(regs[0].clsid, "{LOCAL}");
    }

    #[test]
    fn reverse_lookup_empty_registry_finds_nothing() {
        let (reg, fs) = resolver_parts();
        let resolver = ComResolver::new(&reg, &fs);
        assert!(resolver
            .reverse_lookup(r"C:\none.dll", &[RegView::V64, RegView::V32])
            .unwrap()
            .is_empty());
    }

    // ---- Audit ----

    fn audit_target(fs: &mut MockFileSystem) -> &'static str {
        let target = r"C:\app\target.exe";
        fs.add_pe(target, &[]);
        target
    }

    #[test]
    fn audit_manifest_wins_over_registry() {
        let (mut reg, mut fs) = resolver_parts();
        let target = audit_target(&mut fs);
        set_inproc(
            &mut reg,
            RegLocation::Hklm64,
            "{DUAL}",
            r"C:\Registry\server.dll",
        );
        fs.set_embedded_manifest(
            target,
            r#"<assembly><file name="manifest_server.dll"><comClass clsid="{DUAL}"/></file></assembly>"#,
        );
        fs.add_pe(r"C:\app\manifest_server.dll", &[]);

        let resolver = ComResolver::new(&reg, &fs);
        let audit = resolver.audit(target, "{DUAL}", QueryKind::Clsid).unwrap();

        assert_eq!(audit.source, AuditSource::Manifest);
        assert_eq!(audit.status, "OK");
        assert_eq!(
            audit.server_path.as_deref(),
            Some(r"C:\app\manifest_server.dll")
        );
    }

    #[test]
    fn audit_falls_back_to_registry() {
        let (mut reg, mut fs) = resolver_parts();
        let target = audit_target(&mut fs);
        set_inproc(
            &mut reg,
            RegLocation::Hklm64,
            "{REGONLY}",
            r"C:\Registry\server.dll",
        );
        fs.add_pe(r"C:\Registry\server.dll", &[]);

        let resolver = ComResolver::new(&reg, &fs);
        let audit = resolver
            .audit(target, "{REGONLY}", QueryKind::Clsid)
            .unwrap();

        assert_eq!(audit.source, AuditSource::Registry);
        assert_eq!(audit.status, "OK");
    }

    #[test]
    fn audit_x86_target_uses_32_bit_view() {
        let (mut reg, mut fs) = resolver_parts();
        let target = r"C:\app\target32.exe";
        fs.add_pe_with_machine(target, MACHINE_X86, &[]);
        set_inproc(
            &mut reg,
            RegLocation::Hklm32,
            "{V32}",
            r"C:\Vendor\server32.dll",
        );
        fs.add_pe_with_machine(r"C:\Vendor\server32.dll", MACHINE_X86, &[]);

        let resolver = ComResolver::new(&reg, &fs);
        let audit = resolver.audit(target, "{V32}", QueryKind::Clsid).unwrap();

        assert_eq!(audit.view, RegView::V32);
        assert_eq!(audit.target_machine, MachineType::X86);
        assert_eq!(audit.status, "OK");
    }

    #[test]
    fn audit_bitness_mismatch_for_x86_target_with_x64_inproc() {
        let (mut reg, mut fs) = resolver_parts();
        let target = r"C:\app\target32.exe";
        fs.add_pe_with_machine(target, MACHINE_X86, &[]);
        set_inproc(
            &mut reg,
            RegLocation::Hklm32,
            "{MIX}",
            r"C:\Vendor\server64.dll",
        );
        fs.add_pe(r"C:\Vendor\server64.dll", &[]);

        let resolver = ComResolver::new(&reg, &fs);
        let audit = resolver.audit(target, "{MIX}", QueryKind::Clsid).unwrap();

        assert_eq!(audit.status, "BITNESS_MISMATCH");
        assert!(audit.is_issue());
    }

    #[test]
    fn audit_localserver_does_not_flag_bitness() {
        let (mut reg, mut fs) = resolver_parts();
        let target = r"C:\app\target32.exe";
        fs.add_pe_with_machine(target, MACHINE_X86, &[]);
        set_str(
            &mut reg,
            RegLocation::Hklm32,
            r"Software\Classes\CLSID\{LOCAL64}\LocalServer32",
            "",
            r"C:\tools\server64.exe",
        );
        fs.add_pe(r"C:\tools\server64.exe", &[]);

        let resolver = ComResolver::new(&reg, &fs);
        let audit = resolver
            .audit(target, "{LOCAL64}", QueryKind::Clsid)
            .unwrap();

        assert_eq!(audit.status, "OK");
    }

    #[test]
    fn audit_not_registered_query() {
        let (reg, mut fs) = resolver_parts();
        let target = audit_target(&mut fs);

        let resolver = ComResolver::new(&reg, &fs);
        let audit = resolver.audit(target, "{NOPE}", QueryKind::Clsid).unwrap();

        assert_eq!(audit.status, "NOT_REGISTERED");
        assert!(audit.is_issue());
    }

    #[test]
    fn audit_progid_query_through_registry() {
        let (mut reg, mut fs) = resolver_parts();
        let target = audit_target(&mut fs);
        set_str(
            &mut reg,
            RegLocation::Hklm64,
            r"Software\Classes\P.A\CLSID",
            "",
            "{PA}",
        );
        set_inproc(&mut reg, RegLocation::Hklm64, "{PA}", r"C:\pa.dll");
        fs.add_pe(r"C:\pa.dll", &[]);

        let resolver = ComResolver::new(&reg, &fs);
        let audit = resolver.audit(target, "P.A", QueryKind::Progid).unwrap();

        assert_eq!(audit.status, "OK");
        assert_eq!(audit.clsid.as_deref(), Some("{PA}"));
    }

    #[test]
    fn audit_sidecar_manifest_is_consulted() {
        let (reg, mut fs) = resolver_parts();
        let target = audit_target(&mut fs);
        fs.add_raw(
            r"C:\app\target.exe.manifest",
            br#"<assembly><file name="side.dll"><comClass clsid="{SIDE}"/></file></assembly>"#
                .to_vec(),
        );
        fs.add_pe(r"C:\app\side.dll", &[]);

        let resolver = ComResolver::new(&reg, &fs);
        let audit = resolver.audit(target, "{SIDE}", QueryKind::Clsid).unwrap();

        assert_eq!(audit.source, AuditSource::Manifest);
        assert_eq!(audit.manifest.as_ref().unwrap().source, "sidecar");
        assert_eq!(audit.status, "OK");
    }

    #[test]
    fn audit_manifest_missing_server_file_reports_missing() {
        let (reg, mut fs) = resolver_parts();
        let target = audit_target(&mut fs);
        fs.set_embedded_manifest(
            target,
            r#"<assembly><file name="ghost.dll"><comClass clsid="{GHOST}"/></file></assembly>"#,
        );

        let resolver = ComResolver::new(&reg, &fs);
        let audit = resolver.audit(target, "{GHOST}", QueryKind::Clsid).unwrap();

        assert_eq!(audit.status, "SERVER_MISSING");
    }

    #[test]
    fn audit_missing_target_is_indeterminate() {
        let (reg, fs) = resolver_parts();
        let resolver = ComResolver::new(&reg, &fs);
        assert!(matches!(
            resolver.audit(r"C:\gone.exe", "{X}", QueryKind::Clsid),
            Err(ComError::Indeterminate(_))
        ));
    }

    #[test]
    fn audit_unknown_target_machine_is_unsupported() {
        let (reg, mut fs) = resolver_parts();
        let target = r"C:\app\weird.exe";
        fs.add_pe_with_machine(target, 0x01C4, &[]);

        let resolver = ComResolver::new(&reg, &fs);
        assert!(matches!(
            resolver.audit(target, "{X}", QueryKind::Clsid),
            Err(ComError::UnsupportedArchitecture(_))
        ));
    }
}
