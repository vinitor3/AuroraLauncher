//! Byte-preserving Plasmo 2.1.17 announcement renderer. No filesystem access.
//! TOML validates the document and supplies value spans; the lexical preflight
//! deliberately limits accepted syntax rather than guessing at unsupported TOML.
use super::{RecoveryError, Result, MAX_CONFIG};
use serde::Deserialize;
use std::{collections::BTreeSet, ops::Range};
use toml::{Spanned, Value};

#[path = "connect_voice_plasmo_restore.rs"]
mod restoration;
pub(super) use restoration::reconcile;

#[derive(Deserialize)]
struct Document {
    host: Host,
}
#[derive(Deserialize)]
struct Host {
    public: Option<Public>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Public {
    ip: Spanned<String>,
    port: Spanned<i64>,
}

#[derive(Default)]
struct Shape {
    public: bool,
    ip: Option<Range<usize>>,
    port: Option<Range<usize>>,
    public_header: Option<Range<usize>>,
    ip_code: Option<Range<usize>>,
    port_code: Option<Range<usize>>,
}

fn bare_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
}

fn check_text(bytes: &[u8]) -> Result<&str> {
    if bytes.len() > MAX_CONFIG as usize {
        return Err(RecoveryError::Limit);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| RecoveryError::Integrity)?;
    if text.split_inclusive('\n').count() > 4096 {
        return Err(RecoveryError::Limit);
    }
    if text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        || text.replace("\r\n", "").contains('\r')
    {
        return Err(RecoveryError::Integrity);
    }
    Ok(text)
}

// Locate comments without treating a quoted '#' as a comment. Only single-line
// strings without escapes and flat arrays are supported. This also bounds work
// before invoking the parser: at most 2048 array separators and no inline tables.
fn code_before_comment<'a>(line: &'a str, items: &mut usize) -> Result<&'a str> {
    let mut quote = None;
    let mut brackets = 0_u8;
    let mut end = line.len();
    for (index, c) in line.char_indices() {
        if let Some(active) = quote {
            if c == '\\' {
                return Err(RecoveryError::Integrity);
            }
            if c == active {
                quote = None;
            }
            continue;
        }
        match c {
            '#' => {
                end = index;
                break;
            }
            '\'' | '"' => quote = Some(c),
            '[' => {
                brackets += 1;
                if brackets > 1 {
                    return Err(RecoveryError::Integrity);
                }
            }
            ']' => {
                brackets = brackets.checked_sub(1).ok_or(RecoveryError::Integrity)?;
            }
            '{' | '}' | '\\' => return Err(RecoveryError::Integrity),
            ',' => {
                *items += 1;
                if *items > 2048 {
                    return Err(RecoveryError::Limit);
                }
            }
            _ if c.is_whitespace() && !matches!(c, ' ' | '\t') => {
                return Err(RecoveryError::Integrity);
            }
            _ => {}
        }
    }
    let code = &line[..end];
    if quote.is_some() || brackets != 0 || code.contains("\"\"\"") || code.contains("'''") {
        return Err(RecoveryError::Integrity);
    }
    Ok(code)
}

fn inspect_shape(text: &str) -> Result<Shape> {
    let mut shape = Shape::default();
    let mut tables = BTreeSet::new();
    let mut fields = BTreeSet::new();
    let mut table = String::new();
    let mut offset = 0;
    let mut items = 0;
    for raw in text.split_inclusive('\n') {
        let line = raw.trim_end_matches('\n').trim_end_matches('\r');
        let code = code_before_comment(line, &mut items)?;
        let trimmed = code.trim_matches([' ', '\t']);
        if trimmed.is_empty() {
            offset += raw.len();
            continue;
        }
        let code_start = code.len() - code.trim_start_matches([' ', '\t']).len();
        let code_end = code.trim_end_matches([' ', '\t']).len();
        let code_span = offset + code_start..offset + code_end;
        if trimmed.starts_with('[') {
            let name = trimmed
                .strip_prefix('[')
                .and_then(|s| s.strip_suffix(']'))
                .ok_or(RecoveryError::Integrity)?;
            let parts: Vec<_> = name.split('.').collect();
            if parts.len() > 8 {
                return Err(RecoveryError::Limit);
            }
            if !parts.iter().all(|part| bare_key(part))
                || !tables.insert(name.to_owned())
                || name.starts_with("host.public.")
            {
                return Err(RecoveryError::Integrity);
            }
            if tables.len() > 128 {
                return Err(RecoveryError::Limit);
            }
            table = name.to_owned();
            if name == "host.public" {
                shape.public = true;
                shape.public_header = Some(code_span);
            }
        } else {
            let equals = code.find('=').ok_or(RecoveryError::Integrity)?;
            let key = code[..equals].trim_matches([' ', '\t']);
            if !bare_key(key) || !fields.insert((table.clone(), key.to_owned())) {
                return Err(RecoveryError::Integrity);
            }
            if fields.len() > 512 {
                return Err(RecoveryError::Limit);
            }
            let rhs = code[equals + 1..].trim_start_matches([' ', '\t']);
            let start = code.len() - rhs.len();
            let end = code.trim_end_matches([' ', '\t']).len();
            if start >= end {
                return Err(RecoveryError::Integrity);
            }
            if table == "host.public" {
                let span = Some(offset + start..offset + end);
                match key {
                    "ip" => {
                        shape.ip = span;
                        shape.ip_code = Some(code_span);
                    }
                    "port" => {
                        shape.port = span;
                        shape.port_code = Some(code_span);
                    }
                    _ => return Err(RecoveryError::Integrity),
                }
            }
        }
        offset += raw.len();
    }
    // An explicit host table is required. Inline/dotted aliases and implicit
    // host tables are intentionally outside this renderer's supported subset.
    if !tables.contains("host") {
        return Err(RecoveryError::Integrity);
    }
    Ok(shape)
}

fn raw_public<'a>(text: &'a str, shape: &Shape, public: &Public) -> Result<(&'a str, &'a str)> {
    if !shape.public
        || shape.ip.as_ref() != Some(&public.ip.span())
        || shape.port.as_ref() != Some(&public.port.span())
        || !(0..=65535).contains(public.port.get_ref())
    {
        return Err(RecoveryError::Integrity);
    }
    let raw_ip = text.get(public.ip.span()).ok_or(RecoveryError::Integrity)?;
    let raw_port = text
        .get(public.port.span())
        .ok_or(RecoveryError::Integrity)?;
    let quote = raw_ip
        .as_bytes()
        .first()
        .copied()
        .ok_or(RecoveryError::Integrity)?;
    if !matches!(quote, b'\'' | b'"')
        || raw_ip.len() < 2
        || raw_ip.as_bytes().last() != Some(&quote)
        || raw_ip[1..raw_ip.len() - 1] != *public.ip.get_ref()
        || public.ip.get_ref().is_empty()
        || public.ip.get_ref().len() > 255
        || !public.ip.get_ref().bytes().all(|c| c.is_ascii_graphic())
        || raw_port.is_empty()
        || !raw_port.bytes().all(|c| c.is_ascii_digit())
    {
        return Err(RecoveryError::Integrity);
    }
    Ok((raw_ip, raw_port))
}

pub(super) fn render(bytes: &[u8], port: u16) -> Result<(Vec<u8>, String, String)> {
    if port == 0 {
        return Err(RecoveryError::Integrity);
    }
    let text = check_text(bytes)?;
    let shape = inspect_shape(text)?;
    let mut expected: Value = toml::from_str(text).map_err(|_| RecoveryError::Integrity)?;
    let document: Document = toml::from_str(text).map_err(|_| RecoveryError::Integrity)?;
    let mut rendered = bytes.to_vec();
    let (before, after) = if let Some(public) = document.host.public {
        let (raw_ip, raw_port) = raw_public(text, &shape, &public)?;
        let ip_span = public.ip.span();
        let port_span = public.port.span();
        let quote = raw_ip.as_bytes()[0];
        if public.ip.get_ref() == "127.0.0.1" && *public.port.get_ref() == i64::from(port) {
            return Err(RecoveryError::Integrity);
        }
        let new_ip = format!("{}127.0.0.1{}", quote as char, quote as char);
        let new_port = port.to_string();
        let before = format!("ip = {raw_ip}\nport = {raw_port}");
        let after = format!("ip = {new_ip}\nport = {new_port}");
        // Apply higher ranges first so earlier byte offsets cannot shift. Each
        // selected range was cross-checked against the independent lexical pass.
        let mut edits = [(ip_span, new_ip), (port_span, new_port)];
        edits.sort_by_key(|(range, _)| range.start);
        if edits[0].0.end > edits[1].0.start {
            return Err(RecoveryError::Integrity);
        }
        for (span, replacement) in edits.into_iter().rev() {
            rendered.splice(span, replacement.bytes());
        }
        (before, after)
    } else {
        if shape.public || shape.ip.is_some() || shape.port.is_some() {
            return Err(RecoveryError::Integrity);
        }
        let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
        if !text.ends_with('\n') {
            rendered.extend_from_slice(newline.as_bytes());
        }
        let block =
            format!("[host.public]{newline}ip = \"127.0.0.1\"{newline}port = {port}{newline}");
        rendered.extend_from_slice(block.as_bytes());
        (
            "[host.public] ausente".to_owned(),
            format!("ip = \"127.0.0.1\"\nport = {port}"),
        )
    };
    // Reparse and compare the complete semantic tree with only the two approved
    // fields replaced. This catches accidental scope changes while preserving all
    // original bytes outside the selected spans (or the appended block).
    let host = expected
        .get_mut("host")
        .and_then(Value::as_table_mut)
        .ok_or(RecoveryError::Integrity)?;
    host.insert(
        "public".into(),
        Value::Table(toml::map::Map::from_iter([
            ("ip".into(), Value::String("127.0.0.1".into())),
            ("port".into(), Value::Integer(i64::from(port))),
        ])),
    );
    let output = check_text(&rendered)?;
    // Appending the optional table must respect the same bounds as input;
    // otherwise the next review could reject our own output.
    inspect_shape(output)?;
    let actual: Value = toml::from_str(output).map_err(|_| RecoveryError::Integrity)?;
    if actual != expected {
        return Err(RecoveryError::Integrity);
    }
    Ok((rendered, before, after))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGINAL: &str = "# comentário ✓\r\nserver_id = 'fixture'\r\n[host]\r\nip = '127.0.0.1'\r\nport = 25565\r\n[host.public] # approved table\r\n  port = 24454  # keep this comment\r\n  ip = 'old.example.invalid'\t# address\r\n[voice]\r\nmtu_size = 1024\r\n[voice.proximity]\r\ndistances = [8, 16, 32]\r\n[other.host.public]\r\nip = 'old.example.invalid'\r\nport = 24454\r\n";

    #[test]
    fn replaces_only_two_parser_spans_preserving_crlf_comments_and_other_sections() {
        let (output, before, after) = render(ORIGINAL.as_bytes(), 35555).unwrap();
        let expected = ORIGINAL
            .replacen("port = 24454  #", "port = 35555  #", 1)
            .replacen("ip = 'old.example.invalid'\t#", "ip = '127.0.0.1'\t#", 1);
        assert_eq!(output, expected.as_bytes());
        assert_eq!(before, "ip = 'old.example.invalid'\nport = 24454");
        assert_eq!(after, "ip = '127.0.0.1'\nport = 35555");
        let parsed: Value = toml::from_str(std::str::from_utf8(&output).unwrap()).unwrap();
        assert_eq!(parsed["host"]["public"]["ip"].as_str(), Some("127.0.0.1"));
        assert_eq!(parsed["host"]["port"].as_integer(), Some(25565));
        assert_eq!(
            parsed["other"]["host"]["public"]["port"].as_integer(),
            Some(24454)
        );
        assert!(render(&output, 35555).is_err());
    }

    #[test]
    fn missing_public_appends_a_block_without_activating_or_editing_commented_template() {
        for newline in ["\n", "\r\n"] {
            for final_newline in [true, false] {
                let mut input = format!("[host]{newline}ip = \"0.0.0.0\"{newline}port = 0{newline}# [host.public]{newline}# ip = \"127.0.0.1\"{newline}# port = 0{newline}[voice]{newline}mtu_size = 1024{newline}# final comment");
                if final_newline {
                    input.push_str(newline);
                }
                let (output, before, _) = render(input.as_bytes(), 35555).unwrap();
                assert!(output.starts_with(input.as_bytes()));
                assert_eq!(before, "[host.public] ausente");
                let expected = format!("{input}{}[host.public]{newline}ip = \"127.0.0.1\"{newline}port = 35555{newline}", if final_newline { "" } else { newline });
                assert_eq!(output, expected.as_bytes());
            }
        }
    }

    #[test]
    fn public_must_be_complete_typed_and_unambiguous() {
        for tail in [
            "[host.public]\nip = 'a'\n",
            "[host.public]\nport = 1\n",
            "[host.public]\nip = 1\nport = 1\n",
            "[host.public]\nip = 'a'\nport = '1'\n",
            "[host.public]\nip = 'a'\nport = -1\n",
            "[host.public]\nip = 'a'\nport = 65536\n",
            "[host.public]\nip = 'a'\nport = 1.0\n",
            "[host.public]\nip = 'a'\nport = 1\nextra = true\n",
            "[host.public]\nip = 'a'\nport = 1\n[host.public]\n",
            "[host.public]\nip = 'a'\nip = 'b'\nport = 1\n",
            "[host.public.ip]\nvalue = 'a'\n",
            "public = 'a'\n",
            "public = []\n",
            "public = { ip = 'a', port = 1 }\n",
            "public.ip = 'a'\npublic.port = 1\n",
            "['host'.public]\nip = 'a'\nport = 1\n",
            "[[host.public]]\nip = 'a'\nport = 1\n",
        ] {
            assert!(
                render(format!("[host]\nip = '0.0.0.0'\n{tail}").as_bytes(), 35555).is_err(),
                "{tail}"
            );
        }
        assert!(render(b"[voice]\nmtu_size = 1024\n", 35555).is_err());
        assert!(render(ORIGINAL.as_bytes(), 0).is_err());
    }

    #[test]
    fn unicode_in_comments_and_unrelated_strings_does_not_shift_byte_ranges() {
        let input = "title = 'ação 日本語 # not a comment'\n[host]\nip='0.0.0.0'\n[host.public]\nip=\"example.invalid\" # comentário\nport=0";
        let (output, _, _) = render(input.as_bytes(), 65535).unwrap();
        assert_eq!(
            std::str::from_utf8(&output).unwrap(),
            input
                .replace("\"example.invalid\"", "\"127.0.0.1\"")
                .replace("port=0", "port=65535")
        );
        assert!(!output.ends_with(b"\n"));
        for bad in [
            "\u{00a0}[host]\n",
            "[host]\n\u{2003}ip='x'\n",
            "[host]\n[host.public]\nip=\u{00a0}'x'\nport=1\n",
            "[host]\n[host.public]\nip='\u{2003}x'\nport=1\n",
        ] {
            assert!(render(bad.as_bytes(), 35555).is_err());
        }
    }

    #[test]
    fn unsupported_syntax_and_resource_limits_fail_without_output() {
        for bad in [
            b"\xff".as_slice(),
            b"[host]\rport=0",
            b"[host]\n\0",
            b"[host]\nvalue='''multi'''\n",
            b"[host]\nvalue=\"escaped\\nvalue\"\n",
            b"[host]\nvalue=[[1]]\n",
            b"[host]\nvalue=[\n1,\n2]\n",
            b"[host]\nvalue={nested={key=1}}\n",
            b"[host]\n[a.b.c.d.e.f.g.h.i]\nvalue=1\n",
        ] {
            assert!(render(bad, 35555).is_err());
        }
        assert!(render(&vec![b' '; MAX_CONFIG as usize + 1], 35555).is_err());
        assert!(render(
            format!("[host]\n{}", "# line\n".repeat(4096)).as_bytes(),
            35555
        )
        .is_err());
        assert!(render(
            format!("[host]\nvalues=[{}0]\n", "0,".repeat(2049)).as_bytes(),
            35555
        )
        .is_err());
        let mut many_keys = "[host]\n".to_owned();
        for i in 0..513 {
            many_keys.push_str(&format!("key{i}=0\n"));
        }
        assert!(render(many_keys.as_bytes(), 35555).is_err());
        let mut many_tables = "[host]\n".to_owned();
        for i in 0..128 {
            many_tables.push_str(&format!("[table{i}]\n"));
        }
        assert!(render(many_tables.as_bytes(), 35555).is_err());
        // Valid input at the boundary must not produce an over-limit output.
        let mut boundary_keys = "[host]\n".to_owned();
        for i in 0..511 {
            boundary_keys.push_str(&format!("key{i}=0\n"));
        }
        assert!(render(boundary_keys.as_bytes(), 35555).is_err());
        let mut boundary_tables = "[host]\n".to_owned();
        for i in 0..127 {
            boundary_tables.push_str(&format!("[table{i}]\n"));
        }
        assert!(render(boundary_tables.as_bytes(), 35555).is_err());
    }

    #[test]
    fn semantic_roundtrip_changes_only_announcement_and_supports_second_port() {
        let (first, _, _) = render(ORIGINAL.as_bytes(), 35555).unwrap();
        let (second, before, after) = render(&first, 35556).unwrap();
        assert_eq!(before, "ip = '127.0.0.1'\nport = 35555");
        assert_eq!(after, "ip = '127.0.0.1'\nport = 35556");
        assert_eq!(
            std::str::from_utf8(&second).unwrap(),
            std::str::from_utf8(&first)
                .unwrap()
                .replacen("port = 35555", "port = 35556", 1)
        );
        let mut old: Value = toml::from_str(ORIGINAL).unwrap();
        let mut new: Value = toml::from_str(std::str::from_utf8(&second).unwrap()).unwrap();
        old["host"].as_table_mut().unwrap().remove("public");
        new["host"].as_table_mut().unwrap().remove("public");
        assert_eq!(old, new);
    }
}
