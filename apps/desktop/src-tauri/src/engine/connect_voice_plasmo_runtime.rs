//! Pure validation of the closed Plasmo 2.1.17 runtime profile. No filesystem,
//! mutation, announcement rendering or proof of a running voice service.
use super::*;
use serde::de::IgnoredAny;

#[derive(Deserialize)]
struct RuntimeDocument {
    debug: bool,
    check_for_updates: bool,
    use_crowdin_translations: bool,
    host: RuntimeHost,
}

#[derive(Deserialize)]
struct RuntimeHost {
    ip: Spanned<String>,
    port: Spanned<i64>,
    public: Option<IgnoredAny>,
}

/// Returns only a closed success/error. Unknown defaults and identifiers are
/// bounded and validated as TOML, but never interpreted or returned as proof.
pub(in super::super) fn validate_runtime_configuration(bytes: &[u8]) -> Result<()> {
    let text = check_text(bytes)?;
    let shape = inspect_shape(text)?;
    if shape.public {
        return Err(RecoveryError::Integrity);
    }
    let document: RuntimeDocument = toml::from_str(text).map_err(|_| RecoveryError::Integrity)?;
    // Keep the complete semantic validation used by the legacy inspection,
    // including unknown fields, without exposing their keys/values in errors.
    let _: Value = toml::from_str(text).map_err(|_| RecoveryError::Integrity)?;
    if document.debug
        || document.check_for_updates
        || document.use_crowdin_translations
        || document.host.public.is_some()
        || document.host.ip.get_ref() != "127.0.0.1"
        || *document.host.port.get_ref() != 43003
    {
        return Err(RecoveryError::Integrity);
    }
    let raw_ip = text
        .get(document.host.ip.span())
        .ok_or(RecoveryError::Integrity)?;
    let raw_port = text
        .get(document.host.port.span())
        .ok_or(RecoveryError::Integrity)?;
    // The shared lexical preflight already rejects escapes, dotted/quoted
    // aliases, inline tables and unsupported TOML. Bind fields additionally
    // retain one canonical literal spelling rather than numeric coercions.
    if !matches!(raw_ip, "'127.0.0.1'" | "\"127.0.0.1\"") || raw_port != "43003" {
        return Err(RecoveryError::Integrity);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! assert_matches {
        ($actual:expr, $expected:pat $(,)?) => {
            assert!(
                matches!(&$actual, $expected),
                "unexpected Plasmo runtime result; expected {}",
                stringify!($expected)
            );
        };
    }

    const BASELINE: &str = "debug = false\ncheck_for_updates = false\nuse_crowdin_translations = false\n[host]\nip = '127.0.0.1'\nport = 43003\n";

    #[test]
    fn compiled_values_and_bounded_generated_defaults_are_accepted_without_output() {
        assert_matches!(validate_runtime_configuration(BASELINE.as_bytes()), Ok(()));
        let generated = concat!(
            "# Generated defaults are not an announcement or service proof.\r\n",
            "server_id = 'fixture-secret-not-exported'\r\n",
            "title = 'ação 日本語 # inside string'\r\n",
            "debug=false\r\n",
            "check_for_updates = false\r\n",
            "use_crowdin_translations=false\r\n",
            "\r\n",
            "[host] # fixed bind only\r\n",
            "\tip = \"127.0.0.1\" # comment\r\n",
            "port=43003\r\n",
            "public_ip = 'unrelated-key'\r\n",
            "[voice]\r\n",
            "mtu_size = 1024\r\n",
            "distances = [8, 16, 32]\r\n",
            "[voice.proximity]\r\n",
            "enabled = true\r\n",
            "[other.host.public]\r\n",
            "ip = 'unrelated.example.invalid'\r\n",
            "port = 35554"
        );
        assert_matches!(validate_runtime_configuration(generated.as_bytes()), Ok(()));
        assert_matches!(
            validate_runtime_configuration(BASELINE.trim_end_matches('\n').as_bytes()),
            Ok(())
        );
    }

    #[test]
    fn all_three_flags_are_required_typed_false_without_defaults() {
        for flag in ["debug", "check_for_updates", "use_crowdin_translations"] {
            let line = format!("{flag} = false\n");
            assert_matches!(
                validate_runtime_configuration(BASELINE.replace(&line, "").as_bytes()),
                Err(RecoveryError::Integrity)
            );
            for value in ["true", "'false'", "0", "0.0", "[]", "['false']"] {
                let changed = BASELINE.replace(&line, &format!("{flag} = {value}\n"));
                assert_matches!(
                    validate_runtime_configuration(changed.as_bytes()),
                    Err(RecoveryError::Integrity)
                );
            }
        }
    }

    #[test]
    fn bind_and_port_require_the_closed_literals_and_integer_type() {
        for ip in [
            "''",
            "'0.0.0.0'",
            "'::'",
            "'::1'",
            "'::ffff:127.0.0.1'",
            "'localhost'",
            "'127.0.0.2'",
            "'127.0.0.1:43003'",
            "'127.0.0.1 '",
            "'outside.example.invalid'",
            "127",
            "true",
            "[]",
            "\"127.0.0.\\u0031\"",
        ] {
            let changed = BASELINE.replace("ip = '127.0.0.1'", &format!("ip = {ip}"));
            assert_matches!(
                validate_runtime_configuration(changed.as_bytes()),
                Err(RecoveryError::Integrity)
            );
        }
        for port in [
            "0", "-1", "35554", "43002", "65536", "43003.0", "4.3003e4", "'43003'", "true", "[]",
            "+43003", "4_3003",
        ] {
            let changed = BASELINE.replace("port = 43003", &format!("port = {port}"));
            assert_matches!(
                validate_runtime_configuration(changed.as_bytes()),
                Err(RecoveryError::Integrity)
            );
        }
        for required in ["ip = '127.0.0.1'\n", "port = 43003\n", "[host]\n"] {
            assert_matches!(
                validate_runtime_configuration(BASELINE.replace(required, "").as_bytes()),
                Err(RecoveryError::Integrity)
            );
        }
    }

    #[test]
    fn any_host_public_presence_is_rejected_including_empty_or_aliased_forms() {
        for tail in [
            "public = 'fixture-secret'\n",
            "public = []\n",
            "public = false\n",
            "public = 0\n",
            "public = {}\n",
            "public = { ip = '127.0.0.1', port = 35554 }\n",
            "public.ip = '127.0.0.1'\npublic.port = 35554\n",
            "'public' = []\n",
            "\"public\" = []\n",
            "[host.public]\n",
            "[host.public]\nip = '127.0.0.1'\nport = 35554\n",
            "[host.public.ip]\nvalue = '127.0.0.1'\n",
            "[host.public.extra]\n",
            "['host'.public]\n",
            "[host.'public']\n",
            "[\"host\".\"public\"]\n",
            "[[host.public]]\n",
        ] {
            assert_matches!(
                validate_runtime_configuration(format!("{BASELINE}{tail}").as_bytes()),
                Err(RecoveryError::Integrity)
            );
        }
        for prefix in [
            "host.public.ip = '127.0.0.1'\n",
            "host.public = []\n",
            "'host'.public = []\n",
        ] {
            assert_matches!(
                validate_runtime_configuration(format!("{prefix}{BASELINE}").as_bytes()),
                Err(RecoveryError::Integrity)
            );
        }
    }

    #[test]
    fn duplicates_and_legacy_unsupported_syntax_are_not_normalized() {
        for changed in [
            BASELINE.replace("debug = false", "debug = false\ndebug = false"),
            BASELINE.replace("ip = '127.0.0.1'", "ip = '127.0.0.1'\nip = '127.0.0.1'"),
            BASELINE.replace("port = 43003", "port = 43003\nport = 43003"),
            format!("{BASELINE}[host]\n"),
            format!("{BASELINE}other = 1\nother = 1\n"),
            format!("{BASELINE}value = '''multiline'''\n"),
            format!("{BASELINE}value = \"escaped\\nvalue\"\n"),
            format!("{BASELINE}value = [[1]]\n"),
            format!("{BASELINE}value = [\n1,\n2]\n"),
            format!("{BASELINE}value = {{ nested = {{ key = 1 }} }}\n"),
            format!("{BASELINE}value = '\0'\n"),
            format!("{BASELINE}value = 1\rnext = 1\n"),
            format!("{BASELINE}\u{00a0}value = 1\n"),
            format!("{BASELINE}value = \u{2003}1\n"),
        ] {
            assert_matches!(
                validate_runtime_configuration(changed.as_bytes()),
                Err(RecoveryError::Integrity)
            );
        }
        assert_matches!(
            validate_runtime_configuration(b"\xff"),
            Err(RecoveryError::Integrity)
        );
    }

    #[test]
    fn legacy_byte_line_key_table_depth_and_array_limits_keep_positive_boundaries() {
        let mut bytes = BASELINE.as_bytes().to_vec();
        bytes.push(b'#');
        bytes.resize(MAX_CONFIG as usize, b'x');
        assert_matches!(validate_runtime_configuration(&bytes), Ok(()));
        bytes.push(b'x');
        assert_matches!(
            validate_runtime_configuration(&bytes),
            Err(RecoveryError::Limit)
        );

        let remaining_lines = 4096 - BASELINE.split_inclusive('\n').count();
        let lines = format!("{BASELINE}{}", "# bounded line\n".repeat(remaining_lines));
        assert_matches!(validate_runtime_configuration(lines.as_bytes()), Ok(()));
        assert_matches!(
            validate_runtime_configuration(format!("{lines}# one extra\n").as_bytes()),
            Err(RecoveryError::Limit)
        );

        let mut keys = BASELINE.to_owned();
        for index in 0..507 {
            keys.push_str(&format!("key{index} = 0\n"));
        }
        assert_matches!(validate_runtime_configuration(keys.as_bytes()), Ok(()));
        keys.push_str("one_extra_key = 0\n");
        assert_matches!(
            validate_runtime_configuration(keys.as_bytes()),
            Err(RecoveryError::Limit)
        );

        let mut tables = BASELINE.to_owned();
        for index in 0..127 {
            tables.push_str(&format!("[table{index}]\n"));
        }
        assert_matches!(validate_runtime_configuration(tables.as_bytes()), Ok(()));
        tables.push_str("[one_extra_table]\n");
        assert_matches!(
            validate_runtime_configuration(tables.as_bytes()),
            Err(RecoveryError::Limit)
        );

        assert_matches!(
            validate_runtime_configuration(format!("{BASELINE}[a.b.c.d.e.f.g.h]\n").as_bytes()),
            Ok(())
        );
        assert_matches!(
            validate_runtime_configuration(format!("{BASELINE}[a.b.c.d.e.f.g.h.i]\n").as_bytes()),
            Err(RecoveryError::Limit)
        );

        let separators = format!("{BASELINE}values = [{}0]\n", "0,".repeat(2048));
        assert_matches!(
            validate_runtime_configuration(separators.as_bytes()),
            Ok(())
        );
        assert_matches!(
            validate_runtime_configuration(
                format!("{separators}more_values = [0, 1]\n").as_bytes()
            ),
            Err(RecoveryError::Limit)
        );
    }

    #[test]
    fn unknown_identifiers_and_credentials_never_appear_in_errors_or_results() {
        let secret = "fixture-credential-not-a-runtime-proof";
        let allowed = format!("server_id = '{secret}'\n{BASELINE}");
        assert_matches!(validate_runtime_configuration(allowed.as_bytes()), Ok(()));
        let denied = format!("{allowed}public = '{secret}'\n");
        let error = validate_runtime_configuration(denied.as_bytes()).unwrap_err();
        assert_matches!(error, RecoveryError::Integrity);
        let diagnostic = format!("{error:?}");
        assert!(!diagnostic.contains(secret));
        assert!(!diagnostic.contains("server_id"));
        assert!(!diagnostic.contains("127.0.0.1"));
    }
}
