/** Narrow public protocol: no report prose, logs, paths, account or execution IDs. */
export const DOCTOR_ISSUE_IDS = [
  "java_missing", "java_incompatible", "missing_dependency", "incompatible_version",
  "duplicate_mod", "missing_file", "corrupt_file", "memory_failure", "network_failure",
  "mixin_failure", "graphics_failure", "disk_space", "file_permission", "unknown_cause",
] as const;
export type DoctorIssueId = typeof DOCTOR_ISSUE_IDS[number];

// The model selects an existing action. It cannot author executable commands,
// download links or arbitrary repair instructions.
export const DOCTOR_ACTIONS = {
  select_java: "Selecione no launcher o Java exigido pelo perfil e pelo modpack; depois tente iniciar novamente.",
  check_dependency: "Confira a dependência e a versão exigida na mensagem do loader; procure a versão compatível na fonte oficial do modpack.",
  align_versions: "Compare as versões de Minecraft, loader e mods com as exigidas pelo modpack antes de trocar qualquer arquivo.",
  review_duplicates: "Confira o identificador duplicado apontado pelo loader e revise as cópias instaladas antes de alterar os mods.",
  check_memory: "Feche programas para liberar RAM e confira a memória do perfil. Aumente o limite somente se houver RAM livre; falhas de reserva podem exigir reduzi-lo.",
  verify_installation: "Confira o arquivo indicado no log local e a conclusão da instalação. Se necessário, reinstale o componente pela fonte oficial.",
  check_network: "Verifique sua conexão e a disponibilidade do serviço; tente novamente depois. Um serviço opcional também pode registrar falhas de rede.",
  check_graphics: "Confira se a GPU atende à versão do Minecraft e verifique o driver no site oficial do fabricante.",
  check_disk: "Confira o espaço livre no disco da instância e do sistema antes de repetir a instalação ou o início do jogo.",
  check_permissions: "Confira permissões da pasta e se outro programa está usando o arquivo. Evite desativar proteções ou executar como administrador por tentativa.",
  inspect_mixin: "Confira as versões exigidas pelo modpack e a mensagem completa de Mixin. Uma falha de Mixin, sozinha, não identifica o mod responsável.",
  capture_fresh_logs: "Tente iniciar a instância e analise novamente após o encerramento para obter evidências dessa tentativa.",
  review_local_evidence: "Abra as evidências do diagnóstico local e confira o resultado e os limites da análise antes de alterar a instância.",
} as const;
export type DoctorActionId = keyof typeof DOCTOR_ACTIONS;
export const DOCTOR_ISSUE_LABELS: Record<DoctorIssueId, string> = {
  java_missing: "Java ausente", java_incompatible: "Java incompatível",
  missing_dependency: "Dependência obrigatória não atendida", incompatible_version: "Versões incompatíveis",
  duplicate_mod: "Identificador de mod duplicado", missing_file: "Arquivo ausente ou inacessível",
  corrupt_file: "Formato ou integridade inválidos", memory_failure: "Falha de memória",
  network_failure: "Falha de rede registrada, causalidade não comprovada",
  mixin_failure: "Aplicação de Mixin falhou, responsável não confirmado", graphics_failure: "Falha gráfica explícita",
  disk_space: "Falta de espaço em disco", file_permission: "Acesso a arquivo negado", unknown_cause: "Causa não identificada",
};
const ACTIONS_BY_ISSUE: Record<DoctorIssueId, readonly DoctorActionId[]> = {
  java_missing: ["select_java"], java_incompatible: ["select_java", "align_versions"],
  missing_dependency: ["check_dependency", "align_versions"], incompatible_version: ["align_versions"],
  duplicate_mod: ["review_duplicates"], missing_file: ["verify_installation", "check_permissions"],
  corrupt_file: ["verify_installation"], memory_failure: ["check_memory"], network_failure: ["check_network"],
  mixin_failure: ["inspect_mixin", "align_versions"], graphics_failure: ["check_graphics"],
  disk_space: ["check_disk"], file_permission: ["check_permissions"], unknown_cause: ["capture_fresh_logs", "review_local_evidence"],
};
const OUTCOMES = ["preparing", "running", "success", "failed", "cancelled", "unknown"] as const;
const STATUSES = ["issues", "no_issues", "no_logs", "incomplete"] as const;
export type DoctorAdviceInput = {
  schemaVersion: 1;
  status: typeof STATUSES[number];
  profile: {
    minecraftVersion: string | null;
    loader: "fabric" | "forge" | "vanilla" | "unknown";
    javaMajor: number | null;
    requiredJavaMajor: number | null;
    maxMemoryMb: number | null;
  };
  outcome: typeof OUTCOMES[number];
  issues: Array<{ id: DoctorIssueId; confidence: "high" | "medium"; kind: "fact" | "hypothesis" }>;
  coverage: { hasCurrentLogs: boolean; truncated: boolean };
};
export type DoctorAdvice = {
  schemaVersion: 1;
  summary: string;
  cause: string;
  certainty: "supported" | "hypothesis" | "unknown";
  actions: DoctorActionId[];
  evidenceIds: DoctorIssueId[];
};

function object(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === "object" && !Array.isArray(value);
}
function exact(value: Record<string, unknown>, fields: string[]) {
  return Object.keys(value).length === fields.length && fields.every((key) => Object.hasOwn(value, key));
}
function member<T extends string>(value: unknown, options: readonly T[]): value is T {
  return typeof value === "string" && (options as readonly string[]).includes(value);
}
function optionalInteger(value: unknown, low: number, high: number): value is number | null {
  return value === null || (Number.isInteger(value) && Number(value) >= low && Number(value) <= high);
}
function minecraftVersion(value: unknown): value is string | null {
  return value === null || (typeof value === "string" && /^1\.\d{1,2}(?:\.\d{1,2})?$/.test(value));
}

export function parseDoctorAdviceInput(value: unknown): DoctorAdviceInput | null {
  if (!object(value) || !exact(value, ["schemaVersion", "status", "profile", "outcome", "issues", "coverage"])
    || value.schemaVersion !== 1 || !member(value.status, STATUSES) || !member(value.outcome, OUTCOMES)) return null;
  const profile = value.profile;
  if (!object(profile) || !exact(profile, ["minecraftVersion", "loader", "javaMajor", "requiredJavaMajor", "maxMemoryMb"])
    || !minecraftVersion(profile.minecraftVersion) || !member(profile.loader, ["fabric", "forge", "vanilla", "unknown"])
    || !optionalInteger(profile.javaMajor, 8, 99) || !optionalInteger(profile.requiredJavaMajor, 8, 99)
    || !optionalInteger(profile.maxMemoryMb, 1, 1_048_576)) return null;
  if (!object(value.coverage) || !exact(value.coverage, ["hasCurrentLogs", "truncated"])
    || typeof value.coverage.hasCurrentLogs !== "boolean" || typeof value.coverage.truncated !== "boolean") return null;
  if (!Array.isArray(value.issues) || value.issues.length > DOCTOR_ISSUE_IDS.length) return null;
  const seen = new Set<string>();
  for (const issue of value.issues) {
    if (!object(issue) || !exact(issue, ["id", "confidence", "kind"]) || !member(issue.id, DOCTOR_ISSUE_IDS)
      || !member(issue.confidence, ["high", "medium"]) || !member(issue.kind, ["fact", "hypothesis"]) || seen.has(issue.id)) return null;
    seen.add(issue.id);
  }
  return value as DoctorAdviceInput;
}

/** Constructs a new object explicitly; never spread a local report into a request. */
export function buildDoctorAdviceInput(report: unknown): DoctorAdviceInput {
  if (!object(report) || !object(report.instance) || !Array.isArray(report.issues) || !Array.isArray(report.logs)) {
    throw new Error("Analise a instância novamente antes de consultar o Gemini.");
  }
  const profile = report.instance;
  const version = minecraftVersion(profile.minecraftVersion) ? profile.minecraftVersion : null;
  const loader = String(profile.loader).toLowerCase();
  const result = parseDoctorAdviceInput({
    schemaVersion: 1, status: report.status,
    profile: {
      minecraftVersion: version,
      loader: loader === "fabric" || loader === "forge" || loader === "vanilla" ? loader : "unknown",
      javaMajor: optionalInteger(profile.javaMajor, 8, 99) ? profile.javaMajor : null,
      // The native engine uses a baseline fallback for unknown Minecraft. That
      // fallback is not a verified requirement and must not reach the model.
      requiredJavaMajor: version !== null && optionalInteger(profile.requiredJavaMajor, 8, 99) ? profile.requiredJavaMajor : null,
      maxMemoryMb: optionalInteger(profile.maxMemoryMb, 1, 1_048_576) ? profile.maxMemoryMb : null,
    },
    outcome: object(report.lastRun) ? report.lastRun.outcome : "unknown",
    issues: report.issues.map((issue: unknown) => object(issue) ? { id: issue.id, confidence: issue.confidence, kind: issue.kind } : null),
    coverage: {
      hasCurrentLogs: report.logs.some((log: unknown) => object(log) && log.association === "last_run" && Number(log.bytesRead) > 0),
      truncated: report.logs.some((log: unknown) => object(log) && log.truncated === true),
    },
  });
  if (!result) throw new Error("Este relatório precisa de uma nova análise antes de consultar o Gemini.");
  return result;
}

export function allowedDoctorActions(input: DoctorAdviceInput): DoctorActionId[] {
  const actions = new Set<DoctorActionId>(["review_local_evidence"]);
  for (const issue of input.issues) for (const action of ACTIONS_BY_ISSUE[issue.id]) actions.add(action);
  if (!input.issues.length || input.issues.some((issue) => issue.id === "unknown_cause")) actions.add("capture_fresh_logs");
  return [...actions];
}

function shortPlainText(value: unknown, maximum: number): value is string {
  return typeof value === "string" && value.trim().length > 0 && value.length <= maximum
    && !/[\u0000-\u001f\u007f<>`%]|https?:\/\/|\\\\|www\.|[A-Za-z]:[\\/]|\/(?:Users|home)\/|\b(?:Bearer|powershell|cmd\.exe|sudo|curl|wget|Remove-Item|rm|del|format|apague|apagar|delete|deletar|remova|remover|desative|desativar|execute|executar)\b|[\w.+-]+@[\w.-]+\.[a-z]{2,}|(?:token|password|senha|api.?key)\s*[:=]/i.test(value);
}

export function parseDoctorAdvice(value: unknown, input: DoctorAdviceInput): DoctorAdvice | null {
  if (!object(value) || !exact(value, ["schemaVersion", "summary", "cause", "certainty", "actions", "evidenceIds"])
    || value.schemaVersion !== 1 || !shortPlainText(value.summary, 160) || !shortPlainText(value.cause, 240)
    || !member(value.certainty, ["supported", "hypothesis", "unknown"])) return null;
  const allowed = allowedDoctorActions(input);
  if (!Array.isArray(value.actions) || value.actions.length < 1 || value.actions.length > 3
    || new Set(value.actions).size !== value.actions.length || value.actions.some((action) => !member(action, allowed))) return null;
  const known = input.issues.filter((issue) => issue.id !== "unknown_cause");
  if (!Array.isArray(value.evidenceIds) || value.evidenceIds.length > 3 || new Set(value.evidenceIds).size !== value.evidenceIds.length
    || value.evidenceIds.some((id) => !known.some((issue) => issue.id === id))) return null;
  if (value.certainty !== "unknown" && !value.evidenceIds.length) return null;
  if (value.certainty === "supported" && !known.some((issue) => issue.kind === "fact" && issue.confidence === "high"
    && !["network_failure", "mixin_failure"].includes(issue.id) && (value.evidenceIds as unknown[]).includes(issue.id))) return null;
  if (!known.length && value.certainty !== "unknown") return null;
  const advice = value as DoctorAdvice;
  // No model wording can promote missing evidence to a new causal assertion.
  if (!known.length) return { ...advice,
    summary: input.outcome === "success" || input.outcome === "cancelled"
      ? "Esta execução não foi classificada como crash." : "Causa não identificada com os dados disponíveis.",
    cause: "Sem uma evidência específica, não é possível afirmar o motivo nem apontar um mod responsável.",
  };
  return advice;
}
