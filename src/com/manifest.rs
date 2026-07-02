// Targeted registration-free COM manifest scanner. This is intentionally not
// a general XML parser: it extracts <comClass> declarations (and their
// enclosing <file> server DLL) from application manifests, tolerating
// malformed input by returning whatever parses cleanly.

/// A registration-free COM class declaration from an application manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestComClass {
    pub clsid: String,
    pub progid: Option<String>,
    pub threading_model: Option<String>,
    pub server_dll: Option<String>,
}

/// Extracts comClass declarations from manifest XML.
pub fn parse_manifest_com_classes(xml: &str) -> Vec<ManifestComClass> {
    let mut classes = Vec::new();
    let mut current_file: Option<String> = None;

    for tag in TagIter::new(xml) {
        match tag.name.as_str() {
            "file" => {
                current_file = attr_value(&tag, "name");
                if tag.self_closing {
                    current_file = None;
                }
            }
            "/file" => {
                current_file = None;
            }
            "comClass" => {
                if let Some(clsid) = attr_value(&tag, "clsid") {
                    classes.push(ManifestComClass {
                        clsid,
                        progid: attr_value(&tag, "progid"),
                        threading_model: attr_value(&tag, "threadingModel"),
                        server_dll: current_file.clone(),
                    });
                }
            }
            _ => {}
        }
    }

    classes
}

struct Tag {
    name: String,
    attrs: Vec<(String, String)>,
    self_closing: bool,
}

fn attr_value(tag: &Tag, name: &str) -> Option<String> {
    tag.attrs
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.clone())
}

struct TagIter<'a> {
    rest: &'a str,
}

impl<'a> TagIter<'a> {
    fn new(xml: &'a str) -> Self {
        TagIter { rest: xml }
    }
}

impl<'a> Iterator for TagIter<'a> {
    type Item = Tag;

    fn next(&mut self) -> Option<Tag> {
        loop {
            let open = self.rest.find('<')?;
            self.rest = &self.rest[open + 1..];

            // Skip comments, processing instructions, and declarations.
            if let Some(after_comment) = self.rest.strip_prefix("!--") {
                match after_comment.find("-->") {
                    Some(end) => {
                        self.rest = &after_comment[end + 3..];
                        continue;
                    }
                    None => return None,
                }
            }
            if self.rest.starts_with('?') || self.rest.starts_with('!') {
                match self.rest.find('>') {
                    Some(end) => {
                        self.rest = &self.rest[end + 1..];
                        continue;
                    }
                    None => return None,
                }
            }

            let close = self.rest.find('>')?;
            let body = &self.rest[..close];
            self.rest = &self.rest[close + 1..];

            if let Some(tag) = parse_tag(body) {
                return Some(tag);
            }
        }
    }
}

fn parse_tag(body: &str) -> Option<Tag> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (raw, self_closing) = match trimmed.strip_suffix('/') {
        Some(stripped) => (stripped.trim_end(), true),
        None => (trimmed, false),
    };

    let mut chars = raw.char_indices();
    let closing = raw.starts_with('/');
    let name_start = if closing { 1 } else { 0 };
    let name_end = chars
        .find(|(_, c)| c.is_whitespace())
        .map(|(i, _)| i)
        .unwrap_or(raw.len());
    if name_end <= name_start {
        return None;
    }

    // Strip namespace prefixes so "asmv1:comClass" matches "comClass".
    let full_name = &raw[name_start..name_end];
    let local_name = full_name.rsplit(':').next().unwrap_or(full_name);
    let name = if closing {
        format!("/{local_name}")
    } else {
        local_name.to_string()
    };

    let attrs = parse_attrs(&raw[name_end..]);
    Some(Tag {
        name,
        attrs,
        self_closing,
    })
}

fn parse_attrs(mut rest: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let eq = match rest.find('=') {
            Some(i) => i,
            None => break,
        };
        let key = rest[..eq].trim().to_string();
        rest = rest[eq + 1..].trim_start();
        let quote = match rest.chars().next() {
            Some(c @ ('"' | '\'')) => c,
            _ => break,
        };
        let value_body = &rest[1..];
        let end = match value_body.find(quote) {
            Some(i) => i,
            None => break,
        };
        let value = value_body[..end].to_string();
        rest = &value_body[end + 1..];
        if !key.is_empty() {
            attrs.push((key, value));
        }
    }
    attrs
}

#[cfg(test)]
mod tests {
    use super::parse_manifest_com_classes;

    #[test]
    fn extracts_basic_comclass_with_file_scope() {
        let xml = r#"
        <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
          <file name="vendor.dll">
            <comClass clsid="{MANIFEST-001}"
                      progid="Vendor.Widget"
                      threadingModel="Both" />
          </file>
        </assembly>
        "#;

        let classes = parse_manifest_com_classes(xml);
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].clsid, "{MANIFEST-001}");
        assert_eq!(classes[0].progid.as_deref(), Some("Vendor.Widget"));
        assert_eq!(classes[0].threading_model.as_deref(), Some("Both"));
        assert_eq!(classes[0].server_dll.as_deref(), Some("vendor.dll"));
    }

    #[test]
    fn extracts_multiple_comclasses_across_files() {
        let xml = r#"
        <assembly>
          <file name="a.dll">
            <comClass clsid="{A}" />
            <comClass clsid="{B}" progid="P.B" />
          </file>
          <file name="b.dll">
            <comClass clsid="{C}" />
          </file>
        </assembly>
        "#;

        let classes = parse_manifest_com_classes(xml);
        assert_eq!(classes.len(), 3);
        assert_eq!(classes[0].server_dll.as_deref(), Some("a.dll"));
        assert_eq!(classes[1].progid.as_deref(), Some("P.B"));
        assert_eq!(classes[2].clsid, "{C}");
        assert_eq!(classes[2].server_dll.as_deref(), Some("b.dll"));
    }

    #[test]
    fn comclass_outside_file_has_no_server() {
        let xml = r#"<assembly><comClass clsid="{X}"/></assembly>"#;
        let classes = parse_manifest_com_classes(xml);
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].server_dll, None);
    }

    #[test]
    fn manifest_without_comclass_yields_empty() {
        let xml = r#"<assembly><file name="a.dll"/></assembly>"#;
        assert!(parse_manifest_com_classes(xml).is_empty());
    }

    #[test]
    fn malformed_xml_is_handled_gracefully() {
        let xml = r#"<assembly><file name="a.dll"><comClass clsid="{X}" <broken"#;
        // Must not panic; partial data is acceptable.
        let _ = parse_manifest_com_classes(xml);
    }

    #[test]
    fn comclass_without_clsid_is_ignored() {
        let xml = r#"<file name="a.dll"><comClass progid="P"/></file>"#;
        assert!(parse_manifest_com_classes(xml).is_empty());
    }

    #[test]
    fn namespace_prefixed_tags_match() {
        let xml = r#"<asmv1:file name="n.dll"><asmv1:comClass clsid="{N}"/></asmv1:file>"#;
        let classes = parse_manifest_com_classes(xml);
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].server_dll.as_deref(), Some("n.dll"));
    }

    #[test]
    fn single_quoted_attributes_parse() {
        let xml = r#"<file name='q.dll'><comClass clsid='{Q}' threadingModel='Apartment'/></file>"#;
        let classes = parse_manifest_com_classes(xml);
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].threading_model.as_deref(), Some("Apartment"));
    }

    #[test]
    fn comments_are_skipped() {
        let xml = r#"<!-- <comClass clsid="{FAKE}"/> --><comClass clsid="{REAL}"/>"#;
        let classes = parse_manifest_com_classes(xml);
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].clsid, "{REAL}");
    }

    #[test]
    fn self_closing_file_does_not_scope_following_comclass() {
        let xml = r#"<file name="a.dll"/><comClass clsid="{X}"/>"#;
        let classes = parse_manifest_com_classes(xml);
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].server_dll, None);
    }
}
