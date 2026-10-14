export type VoiceMod = "simple_voice_chat" | "plasmo_voice";
export const MODS = {
  simple_voice_chat: { name: "Simple Voice Chat", version: "2.6.22", sha256: "972a01592f670aa57f2dc2e3587a656b798106037468c61e4bfb49eba7d6ae36", bytes: 5369475, path: "config/voicechat/voicechat-server.properties" },
  plasmo_voice: { name: "Plasmo Voice", version: "2.1.17", sha256: "46e1a4ae51786457712991ba05bf573e2d11331aeadb63b7d108839def022a2b", bytes: 12365966, path: "config/plasmovoice/server/config.toml" },
} as const;
export const LIMITATIONS = ["inspection_only_no_approval", "configured_announcement_not_effective_bind", "empty_absent_or_zero_values_not_resolved", "dedicated_aurora_only_requires_separate_review", "artifact_presence_does_not_prove_mod_loaded", "voice_audio_not_verified", "transport_not_observed"] as const;
const ERRORS = {
  voice_unavailable: "O experimento de configuração de voz está desligado nesta versão.",
  voice_owner_mismatch: "Esta janela não pode alterar o anúncio desta operação.",
  voice_invalid_request: "O pedido não corresponde ao experimento de configuração.",
  voice_busy: "Aguarde a operação de arquivos terminar.",
  voice_context_active: "Finalize a operação anterior antes de selecionar outra cópia ou mod.",
  voice_context_changed: "A cópia ou sua identidade mudou. Inspecione novamente.",
  voice_stale_review: "A revisão venceu ou os arquivos mudaram. Prepare uma nova revisão.",
  voice_java_running: "Feche todos os processos Java antes de inspecionar ou alterar o anúncio.",
  voice_process_unavailable: "Não foi possível verificar a ausência de processos Java.",
  voice_instance_busy: "A cópia está ocupada. Aguarde o jogo ou a outra operação terminar.",
  voice_incompatible: "A versão do jogo, o loader ou o mod não corresponde ao adaptador selecionado.",
  voice_unsafe_path: "O arquivo de configuração está ausente ou seu caminho não pôde ser validado.",
  voice_integrity: "Não foi possível verificar a configuração com segurança.",
  voice_limit: "A configuração ou o inventário excede os limites deste experimento.",
  voice_storage: "Não foi possível concluir a leitura ou gravação. Confira espaço e permissões.",
  voice_needs_recovery: "Há uma operação que exige Recuperação. Não repita a alteração.",
  voice_close_pending: "Conclua ou cancele o pedido de saída antes de continuar.",
  voice_stale_close: "A revisão de saída foi consumida ou venceu. Cancele e revise novamente; o recibo foi preservado.",
  voice_internal: "Não foi possível concluir a operação. Atualize o estado antes de continuar.",
} as const;
export type VoiceErrorCode = keyof typeof ERRORS;
export function voiceError(cause: unknown) { return typeof cause === "string" && Object.hasOwn(ERRORS, cause) ? ERRORS[cause as VoiceErrorCode] : ERRORS.voice_internal; }
export type VoiceCapabilities = {
  available: boolean; experimental: true; diagnosticCopyOnly: true; mode: "dedicatedAuroraOnly";
  guestAddress: "127.0.0.1:35554"; mods: VoiceMod[]; restoration: "strictAndExplicitReconciled";
  audioState: "notVerified"; transportState: "notObserved";
  durableReceipts?: { schemaVersion: 1; maxReceipts: 64 };
};
export type DurableReceipt = { state: "planned" | "saved" | "uncertain"; receiptId: string | null; generation: 0 | 1 | null };
export type VoiceReceiptSummary = { receiptId: string; voiceMod: VoiceMod; state: "ready" | "restored"; generation: 0 | 1 };
export type VoiceReceiptList = { instanceId: string; receipts: VoiceReceiptSummary[] };
export type VoiceObservation = {
  voiceMod: VoiceMod; artifactVersion: string; artifactSha256: string; artifactBytes: number;
  configPath: string; sourceSha256: string; sourceBytes: number; limitations: string[];
  announcement: { type: "simpleVoiceChat"; voiceHost: string } | { type: "plasmoAbsent" } | { type: "plasmoPublic"; ip: string; port: number };
};
export type VoiceReview = {
  action: "setAnnouncement" | "restoreOriginal" | "restoreReconciled"; voiceMod: VoiceMod;
  planId: string; digest: string; configPath: string; before: string; after: string;
  sourceSha256: string; replacementSha256: string; expiresAtMs: number; warning: string;
};
export type VoiceClose = {
  closeId: string; intent: "panel" | "logout" | "exit"; state: "waiting" | "reviewRequired" | "ready";
  review: { digest: string; expiresAtMs: number; instanceId: string; voiceMod: VoiceMod; state: "applied" | "needsRecovery"; selectiveRestorationLost: boolean; durableReceipt?: DurableReceipt } | null;
};
export type VoicePhase = "idle" | "resumingReceipt" | "inspecting" | "observed" | "preparingApply" | "reviewApply" | "applying" | "applied" | "preparingRestore" | "reviewRestore" | "restoring" | "restored" | "needsRecovery" | "failed" | "closing" | "closeReview" | "closed";
export type VoiceSnapshot = {
  generation: number; operationId: string | null; instanceId: string | null; voiceMod: VoiceMod | null;
  phase: VoicePhase; busy: boolean; observation: VoiceObservation | null; review: VoiceReview | null;
  restorationState: "ready" | "needs_recovery" | "restored" | null;
  lastRecoveryResult: { restoredPointId: string; undoAvailable: boolean; recoveredInterrupted: boolean } | null;
  lastErrorCode: VoiceErrorCode | null; close: VoiceClose | null;
  durableReceipt?: DurableReceipt;
};
export const phaseText: Record<VoicePhase, string> = {
  idle: "Selecione uma cópia e um mod", resumingReceipt: "Verificando recibo salvo…", inspecting: "Inspecionando configuração…", observed: "Anúncio configurado observado",
  preparingApply: "Preparando revisão…", reviewApply: "Aplicação aguardando sua revisão", applying: "Aplicando configuração…",
  applied: "Anúncio aplicado somente na cópia", preparingRestore: "Preparando restauração…", reviewRestore: "Restauração aguardando sua revisão",
  restoring: "Restaurando anúncio…", restored: "Anúncio restaurado", needsRecovery: "Verificação na Recuperação necessária",
  failed: "Operação não concluída", closing: "Aguardando arquivos antes de encerrar…", closeReview: "Saída aguardando sua revisão", closed: "Operação encerrada",
};
const invalid = (): never => { throw new Error("Invalid voice projection"); };
const object = (value: unknown): Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : invalid();
function exact(value: unknown, fields: string[]) { const item = object(value); if (Object.keys(item).length !== fields.length || fields.some(key => !Object.hasOwn(item, key))) invalid(); return item; }
function optional(value: unknown, fields: string[], additions: string[]) {
  const item = object(value); return exact(item, [...fields, ...additions.filter(key => Object.hasOwn(item, key))]);
}
export const uuid = (value: unknown): value is string => typeof value === "string" && /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(value);
const hash = (value: unknown): value is string => typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
const integer = (value: unknown, max = Number.MAX_SAFE_INTEGER): value is number => Number.isSafeInteger(value) && (value as number) >= 0 && (value as number) <= max;
const instanceId = (value: unknown): value is string => typeof value === "string" && /^[A-Za-z0-9_.-]{1,64}$/.test(value) && value !== "." && value !== "..";
const mod = (value: unknown): value is VoiceMod => value === "simple_voice_chat" || value === "plasmo_voice";
const text = (value: unknown, max: number, multiline = false): value is string => typeof value === "string" && new TextEncoder().encode(value).length <= max && !(multiline ? /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/u : /[\u0000-\u0008\u000a-\u001f\u007f]/u).test(value);
const bool = (value: unknown): value is boolean => typeof value === "boolean";
const nullable = <T,>(value: unknown, parse: (value: unknown) => T): T | null => value === null ? null : parse(value);
export function voiceCapabilitiesFrom(value: unknown): VoiceCapabilities {
  const item = optional(value, ["available", "experimental", "diagnosticCopyOnly", "mode", "guestAddress", "mods", "restoration", "audioState", "transportState"], ["durableReceipts"]);
  if (!bool(item.available) || item.experimental !== true || item.diagnosticCopyOnly !== true || item.mode !== "dedicatedAuroraOnly" || item.guestAddress !== "127.0.0.1:35554" || item.restoration !== "strictAndExplicitReconciled" || item.audioState !== "notVerified" || item.transportState !== "notObserved" || !Array.isArray(item.mods) || JSON.stringify(item.mods) !== JSON.stringify(item.available ? ["simple_voice_chat", "plasmo_voice"] : [])) invalid();
  if (Object.hasOwn(item, "durableReceipts")) {
    const capability = exact(item.durableReceipts, ["schemaVersion", "maxReceipts"]);
    if (!item.available || capability.schemaVersion !== 1 || capability.maxReceipts !== 64) invalid();
  }
  return item as VoiceCapabilities;
}
export function durableReceiptFrom(value: unknown): DurableReceipt {
  const item = exact(value, ["state", "receiptId", "generation"]);
  if (!["planned", "saved", "uncertain"].includes(item.state as string)) invalid();
  if (item.receiptId === null ? item.generation !== null : !uuid(item.receiptId) || ![0, 1].includes(item.generation as number)) invalid();
  if (item.state === "planned" && item.receiptId !== null || item.state === "saved" && item.receiptId === null) invalid();
  return item as DurableReceipt;
}
export function voiceReceiptListFrom(value: unknown): VoiceReceiptList {
  const item = exact(value, ["instanceId", "receipts"]);
  if (!instanceId(item.instanceId) || !Array.isArray(item.receipts) || item.receipts.length > 64) return invalid();
  const ids = new Set<string>();
  const readyMods = new Set<VoiceMod>();
  for (const raw of item.receipts) {
    const receipt = exact(raw, ["receiptId", "voiceMod", "state", "generation"]);
    if (!uuid(receipt.receiptId) || !mod(receipt.voiceMod) || ids.has(receipt.receiptId)
      || !(receipt.state === "ready" && receipt.generation === 0 || receipt.state === "restored" && receipt.generation === 1)) return invalid();
    ids.add(receipt.receiptId);
    if (receipt.state === "ready") {
      if (readyMods.has(receipt.voiceMod)) invalid();
      readyMods.add(receipt.voiceMod);
    }
  }
  return item as VoiceReceiptList;
}
export function voiceObservationFrom(value: unknown): VoiceObservation {
  const item = exact(value, ["voiceMod", "artifactVersion", "artifactSha256", "artifactBytes", "configPath", "sourceSha256", "sourceBytes", "limitations", "announcement"]);
  if (!mod(item.voiceMod)) invalid();
  const expected = MODS[item.voiceMod as VoiceMod];
  if (item.artifactVersion !== expected.version || item.artifactSha256 !== expected.sha256 || item.artifactBytes !== expected.bytes || item.configPath !== expected.path || !hash(item.sourceSha256) || !integer(item.sourceBytes, 262144) || JSON.stringify(item.limitations) !== JSON.stringify(LIMITATIONS)) invalid();
  const announcement = object(item.announcement);
  if (item.voiceMod === "simple_voice_chat") {
    exact(announcement, ["type", "voiceHost"]);
    if (announcement.type !== "simpleVoiceChat" || !text(announcement.voiceHost, 1024)) invalid();
  } else if (announcement.type === "plasmoAbsent") exact(announcement, ["type"]);
  else {
    exact(announcement, ["type", "ip", "port"]);
    if (announcement.type !== "plasmoPublic" || typeof announcement.ip !== "string" || !/^[\x21-\x7e]{1,255}$/.test(announcement.ip) || !integer(announcement.port, 65535)) invalid();
  }
  return item as VoiceObservation;
}
export function voiceReviewFrom(value: unknown): VoiceReview {
  const item = exact(value, ["action", "voiceMod", "planId", "digest", "configPath", "before", "after", "sourceSha256", "replacementSha256", "expiresAtMs", "warning"]);
  if (typeof item.action !== "string" || !["setAnnouncement", "restoreOriginal", "restoreReconciled"].includes(item.action) || !mod(item.voiceMod) || item.configPath !== MODS[item.voiceMod as VoiceMod].path || !uuid(item.planId) || !hash(item.digest) || !hash(item.sourceSha256) || !hash(item.replacementSha256) || !integer(item.expiresAtMs) || !text(item.before, 2048, true) || !text(item.after, 2048, true) || !text(item.warning, 1024, true)) invalid();
  return item as VoiceReview;
}
function closeFrom(value: unknown): VoiceClose {
  const item = exact(value, ["closeId", "intent", "state", "review"]);
  if (!uuid(item.closeId) || typeof item.intent !== "string" || !["panel", "logout", "exit"].includes(item.intent) || typeof item.state !== "string" || !["waiting", "reviewRequired", "ready"].includes(item.state)) invalid();
  if (item.review !== null) {
    const review = optional(item.review, ["digest", "expiresAtMs", "instanceId", "voiceMod", "state", "selectiveRestorationLost"], ["durableReceipt"]);
    const receipt = Object.hasOwn(review, "durableReceipt") ? durableReceiptFrom(review.durableReceipt) : undefined;
    if (item.state !== "reviewRequired" || !hash(review.digest) || !integer(review.expiresAtMs) || !instanceId(review.instanceId) || !mod(review.voiceMod) || typeof review.state !== "string" || !["applied", "needsRecovery"].includes(review.state)
      || !bool(review.selectiveRestorationLost) || receipt?.state === "planned"
      || review.selectiveRestorationLost !== (receipt?.state !== "saved")
      || (receipt?.state === "saved" && (receipt.generation !== 0 || review.state !== "applied"))
      || (receipt?.state === "uncertain" && review.state !== "needsRecovery")) invalid();
  }
  return item as VoiceClose;
}
export function voiceSnapshotFrom(value: unknown): VoiceSnapshot {
  const item = optional(value, ["generation", "operationId", "instanceId", "voiceMod", "phase", "busy", "observation", "review", "restorationState", "lastRecoveryResult", "lastErrorCode", "close"], ["durableReceipt"]);
  if (!integer(item.generation) || typeof item.phase !== "string" || !Object.hasOwn(phaseText, item.phase) || !bool(item.busy) || (item.lastErrorCode !== null && (typeof item.lastErrorCode !== "string" || !Object.hasOwn(ERRORS, item.lastErrorCode))) || ![null, "ready", "needs_recovery", "restored"].includes(item.restorationState as null | string)) invalid();
  const phase = item.phase as VoicePhase;
  const hasContext = item.operationId !== null;
  if (hasContext ? !uuid(item.operationId) || !instanceId(item.instanceId) || !mod(item.voiceMod) : item.instanceId !== null || item.voiceMod !== null) invalid();
  const observation = nullable(item.observation, voiceObservationFrom);
  const review = nullable(item.review, voiceReviewFrom);
  const close = nullable(item.close, closeFrom);
  const result = nullable(item.lastRecoveryResult, value => {
    const parsed = exact(value, ["restoredPointId", "undoAvailable", "recoveredInterrupted"]);
    if (!uuid(parsed.restoredPointId) || !bool(parsed.undoAvailable) || !bool(parsed.recoveredInterrupted)) invalid();
    return parsed as NonNullable<VoiceSnapshot["lastRecoveryResult"]>;
  });
  if (observation && (!hasContext || observation.voiceMod !== item.voiceMod)) invalid();
  if (review && (!hasContext || review.voiceMod !== item.voiceMod)) invalid();
  const working = ["inspecting", "resumingReceipt", "preparingApply", "applying", "preparingRestore", "restoring", "closing"].includes(phase);
  if (item.busy !== working) invalid();
  if (!hasContext && !["idle", "resumingReceipt", "closing", "closed"].includes(phase)) invalid();
  if (!hasContext && (observation || review || result || item.restorationState !== null)) invalid();
  if (item.phase === "idle" && (hasContext || close)) invalid();
  if (item.phase === "resumingReceipt" && (hasContext || close || item.durableReceipt !== undefined)) invalid();
  if (item.phase === "closed" && (review || observation || item.restorationState === "ready" || item.restorationState === "needs_recovery")) invalid();
  if (item.phase === "observed" && !observation) invalid();
  if ((item.phase === "reviewApply") !== (review?.action === "setAnnouncement")) invalid();
  if ((item.phase === "reviewRestore") !== (!!review && review.action !== "setAnnouncement")) invalid();
  if (item.phase === "reviewApply" && (!observation || review?.sourceSha256 !== observation.sourceSha256 || item.restorationState !== null)) invalid();
  if (["applied", "preparingRestore", "reviewRestore", "restoring"].includes(phase) && item.restorationState !== "ready") invalid();
  if (item.phase === "restored" && item.restorationState !== "restored") invalid();
  if (item.restorationState === "needs_recovery" && !["needsRecovery", "closing", "closeReview"].includes(phase)) invalid();
  if (close && close.review && (close.review.instanceId !== item.instanceId || close.review.voiceMod !== item.voiceMod)) invalid();
  if (Object.hasOwn(item, "durableReceipt")) {
    const receipt = durableReceiptFrom(item.durableReceipt);
    if (!hasContext) invalid();
    if (receipt.state === "planned" && (!["preparingApply", "reviewApply", "applying", "closing"].includes(phase) || item.restorationState !== null)) invalid();
    if (receipt.state === "saved" && (!["applied", "preparingRestore", "reviewRestore", "restoring", "restored", "closing", "closeReview", "closed"].includes(phase)
      || (item.restorationState !== null && item.restorationState !== (receipt.generation === 0 ? "ready" : "restored")))) invalid();
    if (receipt.state === "uncertain" && (!["needsRecovery", "closing", "closeReview", "closed"].includes(phase)
      || (phase !== "closed" && item.restorationState !== "needs_recovery"))) invalid();
    if (close?.review && (close.review.durableReceipt?.state !== receipt.state
      || close.review.durableReceipt?.receiptId !== receipt.receiptId || close.review.durableReceipt?.generation !== receipt.generation)) invalid();
  } else if (close?.review?.durableReceipt !== undefined) invalid();
  if (close?.state === "waiting" && item.phase !== "closing") invalid();
  if (close?.state === "reviewRequired" && (item.phase !== "closeReview" || (!close.review && item.lastErrorCode !== "voice_stale_close"))) invalid();
  if (close?.state === "ready" && item.phase !== "closed") invalid();
  if (["closing", "closeReview"].includes(phase) && !close) invalid();
  return { ...item, observation, review, close, lastRecoveryResult: result } as VoiceSnapshot;
}
export function announcementText(value: VoiceObservation["announcement"]) {
  if (value.type === "simpleVoiceChat") return value.voiceHost === "" ? "voice_host vazio — endereço efetivo não resolvido" : `voice_host = ${value.voiceHost}`;
  if (value.type === "plasmoAbsent") return "host.public ausente — endereço efetivo não resolvido";
  return `host.public.ip = ${value.ip}; port = ${value.port}${value.port === 0 ? " — porta efetiva não resolvida" : ""}`;
}
export const voiceDate = (milliseconds: number) => new Date(milliseconds).toLocaleString("pt-BR");
