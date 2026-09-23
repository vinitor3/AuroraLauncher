//! Recognition of explicit error declarations, not arbitrary keyword mentions.
//! Inputs and identifiers are untrusted. Evidence is fixed prose or bounded
//! numbers; mod names, filenames, URLs and exception payloads never leave here.

pub(super) struct Finding {
    pub id: &'static str,
    pub evidence: String,
}

fn add(findings: &mut Vec<Finding>, id: &'static str, evidence: impl Into<String>) {
    if !findings.iter().any(|item| item.id == id) {
        findings.push(Finding {
            id,
            evidence: evidence.into(),
        });
    }
}

/// Preserve only the message after conventional Minecraft/Log4j prefixes and
/// Minecraft formatting codes. This does not make the remaining text trusted.
fn message(raw: &str) -> String {
    let mut chars = raw.chars().take(8192);
    let mut clean = String::new();
    while let Some(character) = chars.next() {
        if character == '\u{00a7}' {
            chars.next();
        } else {
            clean.push(character);
        }
    }
    let lower = clean.to_lowercase();
    let mut line = lower.trim();
    for _ in 0..6 {
        if !line.starts_with('[') {
            break;
        }
        let Some(end) = line.find(']') else {
            break;
        };
        line = line[end + 1..].trim_start();
    }
    line = line.trim_start_matches([':', '-', '#', ' ', '\t', '\u{feff}']);
    if let Some(rest) = line.strip_prefix("exception in thread \"") {
        if let Some((_, rest)) = rest.split_once('"') {
            line = rest.trim_start();
        }
    }
    line.strip_prefix("caused by: ").unwrap_or(line).to_owned()
}

fn contextual_mention(raw: &str, line: &str) -> bool {
    let lower = raw.to_lowercase();
    lower.contains("[chat]")
        || lower.contains("/chat]")
        || line.starts_with("at ")
        || [
            "example:",
            "documentation:",
            "hint:",
            "test:",
            "checking ",
            "check: ",
            "no error",
            "no exception",
            "not ",
            "no longer ",
            "without ",
            "handled ",
            "recovered ",
            "resolved ",
            "expected ",
            "ignored ",
            "this is not ",
            "não ocorreu",
            "nenhum erro",
            "simulated ",
            "simulação:",
            "previous error:",
            "historical error:",
        ]
        .iter()
        .any(|prefix| line.starts_with(prefix))
}

fn exception_head(value: &str) -> bool {
    let name = value.trim();
    !name.contains(char::is_whitespace)
        && (name.ends_with("exception") || name.ends_with("error"))
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '$' | '_'))
}

fn exception(line: &str, class: &str) -> bool {
    let Some(position) = line.find(class) else {
        return false;
    };
    let suffix = &line[position + class.len()..];
    if !suffix.is_empty() && !suffix.starts_with(':') {
        return false;
    }
    if position == 0 {
        return true;
    }
    let prefix = &line[..position];
    prefix.ends_with(": ")
        && prefix
            .trim_end_matches([':', ' '])
            .split(": ")
            .all(exception_head)
}

fn declaration(line: &str, phrase: &str) -> bool {
    let mut current = line;
    for _ in 0..4 {
        if current.starts_with(phrase) {
            return true;
        }
        let Some((head, body)) = current.split_once(": ") else {
            return false;
        };
        if !exception_head(head) {
            return false;
        }
        current = body;
    }
    false
}

fn io_exception(line: &str) -> bool {
    [
        "java.io.ioexception",
        "java.io.filenotfoundexception",
        "java.nio.file.filesystemexception",
    ]
    .iter()
    .any(|class| exception(line, class))
        || line.starts_with("falha de e/s")
        || line.starts_with("erro de e/s")
}

fn has_any(line: &str, phrases: &[&str]) -> bool {
    phrases.iter().any(|phrase| line.contains(phrase))
}

fn number_after(line: &str, label: &str) -> Option<u32> {
    let tail = line.split_once(label)?.1.trim_start();
    let digits = tail
        .chars()
        .take_while(char::is_ascii_digit)
        .take(4)
        .collect::<String>();
    if digits.len() > 3 {
        return None;
    }
    digits.parse().ok()
}

fn java_evidence(line: &str) -> String {
    let required = number_after(line, "class file version ");
    let accepted = number_after(line, "class file versions up to ");
    if let (Some(required), Some(accepted)) = (required, accepted) {
        if (52..=74).contains(&required) && (52..=74).contains(&accepted) && required > accepted {
            return format!("A classe exige Java {} (formato {required}); a JVM registrada aceita até Java {} (formato {accepted}).", required - 44, accepted - 44);
        }
    }
    if declaration(line, "esta instância precisa do java") {
        "O launcher comparou o Java selecionado com o requisito da instância e recusou a combinação.".into()
    } else {
        "A JVM declarou que não aceita o formato de uma classe carregada.".into()
    }
}

fn dependency_target(line: &str) -> Option<&str> {
    let (_, rest) = line.split_once(" requires ")?;
    let token = rest.split_whitespace().next()?;
    (!token.is_empty()
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')))
    .then_some(token)
}

pub(super) fn detect(text: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut forge_mandatory = false;
    let mut forge_requirement: Option<String> = None;
    let mut generic_incompatibility = false;
    // Caller bounds the file; this additional bound also protects direct error
    // inputs. A declaration never spans more than one adjacent Forge pair.
    for raw in text.lines().take(32768) {
        let line = message(raw);
        if line.is_empty() || contextual_mention(raw, &line) {
            forge_requirement = None;
            forge_mandatory = false;
            continue;
        }
        if line == "missing or unsupported mandatory dependencies:"
            || line == "missing mandatory dependencies:"
        {
            forge_mandatory = true;
            forge_requirement = None;
            continue;
        }
        if forge_mandatory
            && line.starts_with("mod id:")
            && line.contains(", requested by:")
            && line.contains(", expected range:")
        {
            if let Some((_, actual)) = line.split_once(", actual version:") {
                let actual = actual.trim().trim_matches(['\'', '"']);
                if actual == "[missing]" {
                    add(&mut findings, "missing_dependency", "O Forge declarou uma dependência obrigatória com versão instalada marcada como ausente.");
                } else if !actual.is_empty() && !matches!(actual, "null" | "unknown" | "none") {
                    add(&mut findings, "incompatible_version", "O Forge declarou uma dependência obrigatória instalada fora da faixa de versões aceita.");
                }
            }
            continue;
        }
        forge_mandatory = false;
        if let Some(target) = forge_requirement.take() {
            if let Some(status) = line.strip_prefix(&format!("currently, {target} is ")) {
                if status.trim_end_matches(['.', '!']) == "not installed" {
                    add(&mut findings, "missing_dependency", "A mensagem de falha do Forge exige uma dependência e informa, na linha seguinte, que ela não está instalada.");
                } else if !status.is_empty()
                    && status.chars().next().is_some_and(|c| c.is_ascii_digit())
                {
                    add(&mut findings, "incompatible_version", "A mensagem de falha do Forge contrapõe a versão exigida à versão instalada da mesma dependência.");
                }
            }
        }
        let failure_message = line.strip_prefix("failure message: ");
        if let Some(required_line) = failure_message.filter(|line| line.starts_with("mod ")) {
            forge_requirement = dependency_target(required_line).map(str::to_owned);
        }
        let line = failure_message.unwrap_or(&line);
        let mod_requirement = (line.starts_with("mod ") || line.starts_with("'minecraft'"))
            && line.contains(" requires ");
        if mod_requirement
            && has_any(
                line,
                &[
                    ", which is missing!",
                    ", which is missing.",
                    "but it is not installed",
                ],
            )
        {
            add(&mut findings, "missing_dependency", "O Fabric declarou uma dependência exigida como ausente; recomendações opcionais não entram nesta regra.");
        }
        if mod_requirement
            && (has_any(
                line,
                &[
                    "but only the wrong version",
                    "but a non-matching version is present",
                    "which is disabled for this environment",
                ],
            ) || (line.contains("but only") && line.contains("is present")))
        {
            add(&mut findings, "incompatible_version", "O loader declarou que uma dependência obrigatória está presente em versão ou ambiente incompatível.");
        }
        if line.starts_with("mod ")
            && line.contains(" is incompatible with ")
            && line.contains("conflicting version")
            && (line.contains("is present") || line.contains("are present"))
        {
            add(&mut findings, "incompatible_version", "O Fabric declarou uma combinação proibida e confirmou uma versão conflitante instalada.");
        }
        if line.starts_with("file ")
            && has_any(
                line,
                &[
                    "is a fabric mod and cannot be loaded",
                    "is for an older version of forge and cannot be loaded",
                ],
            )
        {
            add(&mut findings, "incompatible_version", "O Forge recusou um arquivo por ser destinado a outro loader ou a uma versão incompatível do loader.");
        }
        if [
            "incompatible mods found!",
            "incompatible mod set!",
            "mod resolution encountered an incompatible mod set!",
        ]
        .iter()
        .any(|phrase| line.ends_with(phrase) && declaration(line, phrase))
        {
            generic_incompatibility = true;
        }

        if [
            "o executável java não existe",
            "no java runtime present",
            "java executable not found",
        ]
        .iter()
        .any(|phrase| declaration(line, phrase))
        {
            add(
                &mut findings,
                "java_missing",
                "A preparação declarou que não encontrou o executável Java selecionado.",
            );
        }
        if exception(line, "java.lang.unsupportedclassversionerror")
            || exception(line, "unsupportedclassversionerror")
            || declaration(line, "esta instância precisa do java")
        {
            add(&mut findings, "java_incompatible", java_evidence(line));
        }
        if declaration(line, "unsupported class file major version") {
            add(&mut findings, "incompatible_version", "O leitor de bytecode recusou o formato de uma classe; esse erro indica incompatibilidade do leitor ou loader e não comprova Java selecionado incorreto.");
        }
        let duplicate_declaration = [
            "duplicate versions for mod id",
            "duplicate mod id",
            "duplicate modids",
            "found duplicate mods",
        ]
        .iter()
        .any(|phrase| declaration(line, phrase));
        let empty_duplicate_result = [": none", ": 0", ": []", ": false", "=false", "=0"]
            .iter()
            .any(|suffix| line.ends_with(suffix));
        if duplicate_declaration && !empty_duplicate_result
            || exception(
                line,
                "net.minecraftforge.fml.common.duplicatemodsfoundexception",
            )
        {
            add(&mut findings, "duplicate_mod", "O loader declarou um identificador duplicado. Nomes de arquivos parecidos não são usados como prova.");
        }
        if exception(line, "java.lang.outofmemoryerror")
            || [
                "could not reserve enough space",
                "native memory allocation (malloc) failed",
                "there is insufficient memory for the java runtime environment",
            ]
            .iter()
            .any(|phrase| declaration(line, phrase))
        {
            add(
                &mut findings,
                "memory_failure",
                "A JVM declarou falta de memória ou falha ao reservar memória.",
            );
        }
        if [
            "java.net.unknownhostexception",
            "java.net.connectexception",
            "java.net.sockettimeoutexception",
            "javax.net.ssl.sslhandshakeexception",
            "java.net.ssl.sslhandshakeexception",
        ]
        .iter()
        .any(|class| exception(line, class))
            || [
                "failed to download",
                "download failed",
                "não foi possível baixar",
                "falha de rede em",
                "o servidor não aceitou a retomada",
                "error sending request for url",
            ]
            .iter()
            .any(|phrase| declaration(line, phrase))
        {
            add(&mut findings, "network_failure", "A tentativa registrou falha explícita de rede ou transferência; um serviço opcional também pode produzir esse erro.");
        }
        if ["mixin apply failed ", "mixin prepare failed "]
            .iter()
            .any(|phrase| declaration(line, phrase))
            || [
                "org.spongepowered.asm.mixin.throwables.mixinapplyerror",
                "org.spongepowered.asm.mixin.transformer.throwables.mixintransformererror",
                "org.spongepowered.asm.mixin.injection.throwables.injectionerror",
            ]
            .iter()
            .any(|class| exception(line, class))
        {
            add(&mut findings, "mixin_failure", "Foi observada uma falha de transformação ou aplicação de mixins; o registro não confirma qual mod a causou.");
        }
        let graphics_context = declaration(line, "glfw error ")
            || declaration(line, "wgl:")
            || declaration(line, "glx:")
            || exception(line, "org.lwjgl.lwjglexception");
        if graphics_context
            && has_any(
                line,
                &[
                    "driver does not appear to support opengl",
                    "driver does not support opengl",
                    "failed to create opengl context",
                    "failed to find a suitable pixel format",
                    "pixel format not accelerated",
                    "glx: failed to create context",
                ],
            )
            || declaration(line, "failed to create window") && line.contains("opengl")
        {
            add(&mut findings, "graphics_failure", "A biblioteca gráfica declarou falha ao obter o contexto OpenGL ou o formato gráfico exigido.");
        }
        if [
            "no space left on device",
            "there is not enough space on the disk",
            "espaço insuficiente no disco",
            "não há espaço suficiente no disco",
        ]
        .iter()
        .any(|phrase| declaration(line, phrase) || (io_exception(line) && line.contains(phrase)))
        {
            add(
                &mut findings,
                "disk_space",
                "Uma operação de arquivo foi recusada por falta de espaço no disco.",
            );
        }
        let permission = exception(line, "java.nio.file.accessdeniedexception")
            || (io_exception(line)
                && has_any(
                    line,
                    &[
                        "permission denied",
                        "access is denied",
                        "access denied",
                        "acesso negado",
                        "operação não permitida",
                        "operation not permitted",
                    ],
                ));
        if permission {
            add(
                &mut findings,
                "file_permission",
                "Uma operação de arquivo foi recusada por falta de permissão de acesso.",
            );
        }
        if !permission
            && (exception(line, "java.nio.file.nosuchfileexception")
                || [
                    "arquivo obrigatório não encontrado",
                    "perfil instalado não encontrado",
                    "unable to access jarfile",
                ]
                .iter()
                .any(|phrase| declaration(line.trim_start_matches("error: "), phrase)))
        {
            add(
                &mut findings,
                "missing_file",
                "A preparação declarou um arquivo obrigatório ausente ou inacessível.",
            );
        }
        if [
            "invalid or corrupt jarfile",
            "zip end header not found",
            "invalid loc header",
            "checksum mismatch",
            "hash mismatch",
            "não passou na verificação de integridade",
            "integridade inválida para",
            "perfil minecraft inválido",
            "invalid zip archive",
        ]
        .iter()
        .any(|phrase| declaration(line.trim_start_matches("error: "), phrase))
        {
            add(&mut findings, "corrupt_file", "Uma verificação declarou formato inválido ou divergência de integridade de arquivo.");
        }
    }
    // Generic Fabric headers cover several dependency problems. Prefer the
    // specific declaration instead of duplicating/misclassifying it.
    if generic_incompatibility
        && !findings
            .iter()
            .any(|item| matches!(item.id, "missing_dependency" | "incompatible_version"))
    {
        add(&mut findings, "incompatible_version", "O loader declarou um conjunto incompatível, mas o trecho disponível não especifica a restrição violada.");
    }
    findings
}
