//! Pure reconciliation of the two Plasmo announcement fields. Authorization,
//! identity, receipt state, snapshots and writes belong to the caller.
use super::*;

struct Parsed<'a> {
    text: &'a str,
    shape: Shape,
    document: Document,
    tree: Value,
}
impl<'a> Parsed<'a> {
    fn read(bytes: &'a [u8]) -> Result<Self> {
        let text = check_text(bytes)?;
        let shape = inspect_shape(text)?;
        let tree = toml::from_str(text).map_err(|_| RecoveryError::Integrity)?;
        let document: Document = toml::from_str(text).map_err(|_| RecoveryError::Integrity)?;
        if let Some(public) = &document.host.public {
            raw_public(text, &shape, public)?;
        } else if shape.public || shape.ip.is_some() || shape.port.is_some() {
            return Err(RecoveryError::Integrity);
        }
        Ok(Self {
            text,
            shape,
            document,
            tree,
        })
    }

    fn public(&self) -> Result<&Public> {
        self.document
            .host
            .public
            .as_ref()
            .ok_or(RecoveryError::Integrity)
    }

    fn raw(&self) -> Result<(&str, &str)> {
        raw_public(self.text, &self.shape, self.public()?)
    }

    fn summary(&self) -> Result<String> {
        let (ip, port) = self.raw()?;
        Ok(format!("ip = {ip}\nport = {port}"))
    }
}

fn host(tree: &mut Value) -> Result<&mut toml::map::Map<String, Value>> {
    tree.get_mut("host")
        .and_then(Value::as_table_mut)
        .ok_or(RecoveryError::Integrity)
}

fn edit_bytes(current: &[u8], mut edits: Vec<(Range<usize>, Vec<u8>)>) -> Result<Vec<u8>> {
    edits.sort_by_key(|(range, _)| range.start);
    let mut end = 0;
    let mut length = current.len();
    for (range, replacement) in &edits {
        if range.start < end || range.start >= range.end || range.end > current.len() {
            return Err(RecoveryError::Integrity);
        }
        end = range.end;
        length = length
            .checked_sub(range.len())
            .and_then(|value| value.checked_add(replacement.len()))
            .ok_or(RecoveryError::Limit)?;
    }
    if length > MAX_CONFIG as usize {
        return Err(RecoveryError::Limit);
    }
    let mut output = Vec::with_capacity(length);
    let mut start = 0;
    for (range, replacement) in edits {
        output.extend_from_slice(&current[start..range.start]);
        output.extend_from_slice(&replacement);
        start = range.end;
    }
    output.extend_from_slice(&current[start..]);
    Ok(output)
}

/// Inputs are private receipt bytes plus the exact newly reviewed current file.
/// Equal announcement values permit reconciliation of formatting/other fields;
/// they never authorize this transformation or a filesystem write by themselves.
pub(in super::super) fn reconcile(
    original: &[u8],
    applied: &[u8],
    current: &[u8],
) -> Result<(Vec<u8>, String, String)> {
    let original = Parsed::read(original)?;
    let applied = Parsed::read(applied)?;
    let current_parsed = Parsed::read(current)?;
    let approved = applied.public()?;
    let actual = current_parsed
        .public()
        .map_err(|_| RecoveryError::StalePreview)?;
    if approved.ip.get_ref() != "127.0.0.1" || *approved.port.get_ref() == 0 {
        return Err(RecoveryError::Integrity);
    }
    if actual.ip.get_ref() != approved.ip.get_ref()
        || actual.port.get_ref() != approved.port.get_ref()
    {
        return Err(RecoveryError::StalePreview);
    }
    // Defend the receipt invariant: application changed only this subtree, and
    // was not a semantic no-op. Current unrelated changes remain unrestricted.
    let applied_public = applied.tree["host"]["public"].clone();
    let mut expected_applied = original.tree.clone();
    host(&mut expected_applied)?.insert("public".into(), applied_public);
    if expected_applied != applied.tree || expected_applied == original.tree {
        return Err(RecoveryError::Integrity);
    }
    let before = current_parsed.summary()?;
    let mut expected = current_parsed.tree.clone();
    let mut edits = Vec::new();
    let after = if let Some(public) = &original.document.host.public {
        let (ip, port) = raw_public(original.text, &original.shape, public)?;
        edits.push((actual.ip.span(), ip.as_bytes().to_vec()));
        edits.push((actual.port.span(), port.as_bytes().to_vec()));
        host(&mut expected)?.insert("public".into(), original.tree["host"]["public"].clone());
        format!("ip = {ip}\nport = {port}")
    } else {
        for range in [
            &current_parsed.shape.public_header,
            &current_parsed.shape.ip_code,
            &current_parsed.shape.port_code,
        ] {
            let range = range.as_ref().ok_or(RecoveryError::Integrity)?;
            let code = current_parsed
                .text
                .get(range.clone())
                .ok_or(RecoveryError::Integrity)?;
            // Keep all indentation/separator whitespace, even inside the code
            // range. Comments and line endings lie outside every edited range.
            let whitespace = code
                .bytes()
                .filter(|byte| matches!(byte, b' ' | b'\t'))
                .collect();
            edits.push((range.clone(), whitespace));
        }
        host(&mut expected)?.remove("public");
        "[host.public] ausente".into()
    };
    let output = edit_bytes(current, edits)?;
    if output == current {
        return Err(RecoveryError::Integrity);
    }
    let verified = Parsed::read(&output)?;
    if verified.tree != expected {
        return Err(RecoveryError::Integrity);
    }
    Ok((output, before, after))
}

#[cfg(test)]
mod tests {
    use super::*;

    const WITHOUT_PUBLIC: &str = "[host]\nip='0.0.0.0'\nport=25565\n[voice]\nmtu_size=1024\n";
    const ORIGINAL: &str = "[host]\nip='0.0.0.0'\nport=25565\n[host.public]\nip=\"old.example.invalid\"\nport=24454\n[voice]\nmtu_size=1024\n";

    fn applied(original: &str) -> Vec<u8> {
        render(original.as_bytes(), 35555).unwrap().0
    }
    fn tree(bytes: &[u8]) -> Value {
        toml::from_str(std::str::from_utf8(bytes).unwrap()).unwrap()
    }

    #[test]
    fn reconcile_existing_public_restores_original_rhs_preserving_current_layout_and_other_values()
    {
        let current = "# comentários 日本語\r\n[host]\nip='192.0.2.10'\nport=30000\r\n[voice]\nmtu_size=1111\r\n[host.public]\r\n  port = 35555 # current port\r\n\tip = '127.0.0.1'\t# current ip\r\n[other.host.public]\nip='127.0.0.1'\nport=35555\n";
        let (output, before, after) =
            reconcile(ORIGINAL.as_bytes(), &applied(ORIGINAL), current.as_bytes()).unwrap();
        let expected = current
            .replacen("port = 35555", "port = 24454", 1)
            .replacen("ip = '127.0.0.1'", "ip = \"old.example.invalid\"", 1);
        assert_eq!(output, expected.as_bytes());
        assert_eq!(before, "ip = '127.0.0.1'\nport = 35555");
        assert_eq!(after, "ip = \"old.example.invalid\"\nport = 24454");
        assert_eq!(tree(&output)["voice"]["mtu_size"].as_integer(), Some(1111));
        assert_eq!(
            tree(&output)["other"]["host"]["public"]["port"].as_integer(),
            Some(35555)
        );
    }

    #[test]
    fn reconcile_absent_public_removes_only_code_and_preserves_all_comments_whitespace_and_eol() {
        for eol in ["\n", "\r\n"] {
            for final_eol in [false, true] {
                let current = [
                    "# retained unicode ação",
                    "[host]",
                    "ip='192.0.2.10'",
                    "port=30000",
                    " \t[host.public]\t # header",
                    " \tip \t=  \"127.0.0.1\"\t# address",
                    "\tport=35555  # port",
                    "# after public",
                    "[voice]",
                    "mtu_size=1111",
                    "[other.host.public]",
                    "ip='127.0.0.1'",
                    "port=35555",
                ]
                .join(eol)
                    + if final_eol { eol } else { "" };
                let (output, _, after) = reconcile(
                    WITHOUT_PUBLIC.as_bytes(),
                    &applied(WITHOUT_PUBLIC),
                    current.as_bytes(),
                )
                .unwrap();
                let expected = current
                    .replacen("[host.public]", "", 1)
                    .replacen("ip \t=  \"127.0.0.1\"", " \t  ", 1)
                    .replacen("port=35555", "", 1);
                assert_eq!(output, expected.as_bytes());
                assert_eq!(after, "[host.public] ausente");
                assert!(tree(&output)["host"].get("public").is_none());
                assert_eq!(tree(&output)["host"]["port"].as_integer(), Some(30000));
                assert_eq!(
                    tree(&output)["other"]["host"]["public"]["port"].as_integer(),
                    Some(35555)
                );
            }
        }
    }

    #[test]
    fn reconcile_also_handles_final_public_block_without_final_newline() {
        let applied = applied(WITHOUT_PUBLIC);
        let current = std::str::from_utf8(&applied)
            .unwrap()
            .trim_end_matches('\n');
        let (output, _, _) =
            reconcile(WITHOUT_PUBLIC.as_bytes(), &applied, current.as_bytes()).unwrap();
        assert_eq!(output, format!("{WITHOUT_PUBLIC}\n  \n  ").as_bytes());
        assert_eq!(tree(&output), tree(WITHOUT_PUBLIC.as_bytes()));
    }

    #[test]
    fn reconcile_refuses_changed_owned_values_missing_public_and_semantic_noop() {
        let applied = applied(ORIGINAL);
        let current = std::str::from_utf8(&applied).unwrap();
        for changed in [
            current.replace("127.0.0.1", "192.0.2.10"),
            current.replace("35555", "35556"),
            WITHOUT_PUBLIC.to_owned(),
        ] {
            assert!(matches!(
                reconcile(ORIGINAL.as_bytes(), &applied, changed.as_bytes()),
                Err(RecoveryError::StalePreview)
            ));
        }
        assert!(reconcile(&applied, &applied, &applied).is_err());
        let unrelated = current.replace("mtu_size=1024", "mtu_size=1111");
        assert!(reconcile(
            ORIGINAL.as_bytes(),
            unrelated.as_bytes(),
            unrelated.as_bytes()
        )
        .is_err());
        let not_ours = current.replace("127.0.0.1", "192.0.2.10");
        assert!(reconcile(
            ORIGINAL.as_bytes(),
            not_ours.as_bytes(),
            not_ours.as_bytes()
        )
        .is_err());
    }

    #[test]
    fn reconcile_refuses_ambiguous_or_expanded_public_in_any_input() {
        let applied = applied(ORIGINAL);
        for public in [
            "[host.public]\nip='127.0.0.1'\n",
            "[host.public]\nport=35555\n",
            "[host.public]\nip='127.0.0.1'\nport=35555\nextra=true\n",
            "[host.public]\nip='127.0.0.1'\nport=35555\n[host.public.child]\na=1\n",
            "[host.public]\nip='127.0.0.1'\nport=35555\nip='other'\n",
            "[host.public]\nip='127.0.0.1'\nport=35555\n[host.public]\n",
            "[host.public]\nip='127.0.0.1'\nport='35555'\n",
            "[host.public]\nip='127.0.0.1'\nport=+35555\n",
            "public={ip='127.0.0.1',port=35555}\n",
            "public.ip='127.0.0.1'\npublic.port=35555\n",
            "[\"host\".public]\nip='127.0.0.1'\nport=35555\n",
            "[[host.public]]\nip='127.0.0.1'\nport=35555\n",
            "[host.public]\nip=\"127.0.0.\\u0031\"\nport=35555\n",
            "[host.public]\nip=\u{00a0}'127.0.0.1'\nport=35555\n",
        ] {
            let invalid = format!("[host]\nip='0.0.0.0'\nport=25565\n{public}");
            assert!(
                reconcile(ORIGINAL.as_bytes(), &applied, invalid.as_bytes()).is_err(),
                "{public}"
            );
            assert!(reconcile(invalid.as_bytes(), &applied, &applied).is_err());
            assert!(reconcile(ORIGINAL.as_bytes(), invalid.as_bytes(), &applied).is_err());
        }
    }

    #[test]
    fn reconcile_respects_input_complexity_and_output_size_bounds() {
        let applied = applied(ORIGINAL);
        let too_large = vec![b' '; MAX_CONFIG as usize + 1];
        assert!(reconcile(&too_large, &applied, &applied).is_err());
        assert!(reconcile(ORIGINAL.as_bytes(), &too_large, &applied).is_err());
        assert!(reconcile(ORIGINAL.as_bytes(), &applied, &too_large).is_err());
        for suffix in [
            "# line\n".repeat(4096),
            (0..513).map(|i| format!("key{i}=0\n")).collect::<String>(),
            (0..128)
                .map(|i| format!("[table{i}]\n"))
                .collect::<String>(),
            "[a.b.c.d.e.f.g.h.i]\na=0\n".into(),
            "value=\"escaped\\nvalue\"\n".into(),
            "value=[[1]]\n".into(),
        ] {
            let current = [applied.as_slice(), suffix.as_bytes()].concat();
            assert!(reconcile(ORIGINAL.as_bytes(), &applied, &current).is_err());
        }
        // Restoring a longer original address must not exceed the same limit.
        let original = ORIGINAL.replace("old.example.invalid", &"a".repeat(255));
        let applied = render(original.as_bytes(), 35555).unwrap().0;
        let growth = 255 - "127.0.0.1".len();
        for (padding, accepted) in [(growth, true), (0, false)] {
            let mut current = [applied.as_slice(), b"#"].concat();
            current.resize(MAX_CONFIG as usize - padding, b'x');
            let result = reconcile(original.as_bytes(), &applied, &current);
            if accepted {
                assert_eq!(result.unwrap().0.len(), MAX_CONFIG as usize);
            } else {
                assert!(matches!(result, Err(RecoveryError::Limit)));
            }
        }
    }
}
