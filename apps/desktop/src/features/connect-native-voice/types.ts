export const voiceRuntimes = {"fabric-loader-0.16.14-1.21.1": "Fabric 1.21.1 · Java 21", "fabric-loader-0.16.14-1.20.1": "Fabric 1.20.1 · Java 17", "fabric-loader-0.16.14-1.19.2": "Fabric 1.19.2 · Java 17", "fabric-loader-0.13.3-1.16.5": "Fabric 1.16.5 · Java 8", "1.21.1-forge-52.1.12": "Forge 1.21.1 · Java 21", "1.20.1-forge-47.4.23": "Forge 1.20.1 · Java 17", "1.19.2-forge-43.4.16": "Forge 1.19.2 · Java 17", "1.16.5-forge-36.2.42": "Forge 1.16.5 · Java 8", "1.12.2-forge-14.23.5.2860": "Forge 1.12.2 · Java 8"} as const;
export type VoiceRuntime = keyof typeof voiceRuntimes;
export type VoiceProfile = "simpleVoiceChat" | "plasmoVoice";
export type VoiceApproval = { kind: "preparation" | "files" | "execution" | "host" | "restore" | "eulaRestore" | "socialSource"; digest: string; remainingMs: number; before: string; after: string; details: string[] };
export type VoiceParticipant = { participantId: string; publicId: string; localPlayer: boolean };
export type NativeVoiceSnapshot = { revision: number; available: boolean; manualLabAvailable?: boolean; phase: string; profile: VoiceProfile | null; runtimeProfile?: VoiceRuntime | null;
  review: VoiceApproval | null; operationId: string | null; hostPublicId: string | null; participants: VoiceParticipant[];
  udpState: string; tcpAddress: string | null; negotiatedDatagramBytes: number | null; recoveryPending: boolean;
  recoverableCopies: { bundleId: string; receiptId: string; profile: VoiceProfile; runtimeProfile: VoiceRuntime; eulaRecoveryPending: boolean; announcementRecoveryPending: boolean }[];
  eulaRecoveryAvailable: boolean; socialRevalidationAvailable?: boolean; eulaState: string; errorCode: string | null; exportedInvitation: string | null; audioState: "notVerified" };
const object = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value);
const string = (value: unknown, max = 4096): value is string => typeof value === "string" && value.length <= max;
const nullableString = (value: unknown, max = 4096) => value === null || string(value, max);
const phases = new Set(["idle", "preparing", "preparationReviewed", "preparingServer", "filesReviewed", "creating", "executionReviewed", "starting", "ready", "socialSource", "socialSourceReviewing", "socialSourceReviewed", "socialSourceApplying", "reviewingHost", "hostReviewed", "binding", "bound", "transportClosed", "reviewExpired", "stopping", "stopped", "restoreReviewed", "eulaRestoreReviewed", "restoringEula", "restored", "needsRecovery", "failed"]);
export function nativeVoiceSnapshotFrom(value: unknown): NativeVoiceSnapshot {
  if (!object(value) || !Number.isSafeInteger(value.revision) || Number(value.revision) < 0 || typeof value.available !== "boolean"
    || !(value.manualLabAvailable === undefined || typeof value.manualLabAvailable === "boolean" && (!value.manualLabAvailable || value.available))
    || !string(value.phase) || !phases.has(value.phase) || ![null, "simpleVoiceChat", "plasmoVoice"].includes(value.profile as null)
    || !(value.runtimeProfile === undefined || value.runtimeProfile === null || typeof value.runtimeProfile === "string" && Object.hasOwn(voiceRuntimes, value.runtimeProfile))
    || (value.profile === "plasmoVoice" && value.runtimeProfile === "1.12.2-forge-14.23.5.2860")
    || !(value.socialRevalidationAvailable === undefined || typeof value.socialRevalidationAvailable === "boolean")
    || !nullableString(value.operationId, 128) || !nullableString(value.hostPublicId, 128)
    || !Array.isArray(value.participants) || value.participants.length > 8 || !value.participants.every(p => object(p)
      && string(p.participantId, 128) && string(p.publicId, 128) && typeof p.localPlayer === "boolean")
    || !["notStarted", "enabled", "retiring", "retired", "closed", "localUnavailable", "cleanupFailed"].includes(String(value.udpState))
    || !nullableString(value.tcpAddress, 64) || (value.tcpAddress !== null && (!/^127\.0\.0\.1:[1-9][0-9]{0,4}$/.test(String(value.tcpAddress)) || Number(String(value.tcpAddress).split(":")[1]) > 65535))
    || !(value.negotiatedDatagramBytes === null || Number.isSafeInteger(value.negotiatedDatagramBytes) && Number(value.negotiatedDatagramBytes) >= 4096 && Number(value.negotiatedDatagramBytes) <= 65535)
    || !Array.isArray(value.recoverableCopies) || value.recoverableCopies.length > 64 || !value.recoverableCopies.every(c => object(c) && string(c.bundleId, 128) && string(c.receiptId, 128) && ["simpleVoiceChat", "plasmoVoice"].includes(String(c.profile))
      && typeof c.runtimeProfile === "string" && Object.hasOwn(voiceRuntimes, c.runtimeProfile) && !(c.profile === "plasmoVoice" && c.runtimeProfile === "1.12.2-forge-14.23.5.2860")
      && typeof c.eulaRecoveryPending === "boolean" && typeof c.announcementRecoveryPending === "boolean" && (c.eulaRecoveryPending || c.announcementRecoveryPending))
    || typeof value.recoveryPending !== "boolean" || typeof value.eulaRecoveryAvailable !== "boolean" || !["unchanged", "ownedBySource", "restored", "needsRecovery"].includes(String(value.eulaState))
    || !nullableString(value.errorCode, 128) || !nullableString(value.exportedInvitation, 32768) || value.audioState !== "notVerified") throw new Error("native_voice_invalid_snapshot");
  if (value.review !== null) {
    const r = value.review;
    if (!object(r) || !["preparation", "files", "execution", "host", "restore", "eulaRestore", "socialSource"].includes(String(r.kind)) || !string(r.digest, 64) || !/^[a-f0-9]{64}$/.test(r.digest)
      || !Number.isSafeInteger(r.remainingMs) || Number(r.remainingMs) < 0 || Number(r.remainingMs) > (r.kind === "restore" ? 300_000 : 60_000)
      || !string(r.before, 16384) || !string(r.after, 16384) || !Array.isArray(r.details) || r.details.length > (value.runtimeProfile ? 1024 : 20) || !r.details.every(d => string(d, value.runtimeProfile ? 32768 : 4096))) throw new Error("native_voice_invalid_review");
  }
  return value as NativeVoiceSnapshot;
}
export function nativeVoiceError(value: unknown): string {
  const code = typeof value === "string" ? value : "";
  if (code === "native_voice_new_advertisement_required") return "Encerre o anúncio anterior e crie um novo no Social antes de usar este servidor novamente.";
  if (code === "native_voice_social_still_active") return "Encerre a tentativa Social e aguarde a liberação dos recursos antes de revalidar este servidor.";
  if (code === "native_voice_social_source_changed") return "A conta, a tentativa anterior ou a origem mudou. Esta revalidação foi recusada.";
  if (code === "native_voice_revalidation_failed") return "Não foi possível confirmar a origem atual. O servidor permanece sob responsabilidade nativa; tente uma nova revisão ou encerre-o.";
  if (code === "native_voice_revalidation_expired") return "A revisão de revalidação terminou. O prazo do servidor não foi renovado; prepare uma nova revisão se ele continuar ativo.";
  if (code === "native_voice_runtime_unsupported") return "Plasmo Voice não está disponível para Forge 1.12.2. Selecione Simple Voice Chat ou outro runtime.";
  if (code === "native_voice_pinned_resources_unavailable") return "Não foi possível baixar ou verificar os arquivos fixados deste perfil. Confira a conexão e tente novamente.";
  if (code === "native_voice_cancelled") return "O preparo dos recursos foi cancelado.";
  if (code === "native_voice_stop_forced") return "O servidor dedicado foi encerrado à força. O salvamento do mundo pode ter ficado incompleto.";
  if (code === "native_voice_transport_cleanup_failed") return "O encerramento do transporte de voz ainda precisa ser confirmado.";
  if (code === "native_voice_stop_forced_cleanup_failed") return "O servidor dedicado foi encerrado à força e o salvamento do mundo pode ter ficado incompleto. O encerramento do transporte de voz ainda precisa ser confirmado.";
  if (code === "native_voice_source_expired") return "O prazo original do servidor dedicado terminou.";
  if (code === "native_voice_source_cancelled") return "A execução do servidor dedicado foi cancelada.";
  if (code === "native_voice_source_failed") return "A execução do servidor dedicado terminou com falha. Confira o estado da cópia.";
  if (code === "native_voice_preparation_failed") return "O preparo não foi concluído. Revise o estado abaixo e tente novamente quando os recursos forem liberados.";
  if (code === "native_voice_recovery_required") return "Há uma recuperação pendente nesta cópia. Revise o EULA e o anúncio separadamente antes de preparar outra sessão.";
  if (code === "native_voice_eula_restore_failed" || code === "native_voice_eula_restore_review_failed") return "Não foi possível recuperar o EULA. Encerre o Java e prepare uma nova revisão; o anúncio de voz tem sua própria recuperação.";
  if (code === "native_voice_local_udp_in_use") return "A porta local 35554 já está ocupada. O transporte desta tentativa foi encerrado.";
  if (code === "native_voice_java_running") return "Encerre o Java antes de aplicar ou restaurar o anúncio.";
  if (code === "native_voice_restore_review_failed") return "A restauração estrita não foi aprovada. Você pode revisar a restauração somente das chaves de anúncio.";
  if (code.includes("expired") || code.includes("stale")) return "A revisão expirou ou mudou. Prepare uma nova revisão.";
  if (code === "native_voice_busy") return "Uma operação nativa ainda está em andamento.";
  return "A operação de voz foi recusada. Confira o estado nativo antes de tentar novamente.";
}
export const voicePhase: Record<string, string> = { idle: "Sem sessão", preparationReviewed: "Preparo do servidor em revisão", preparingServer: "Preparando servidor oficial", preparing: "Baixando ou verificando recursos", filesReviewed: "Arquivos em revisão", creating: "Preparando cópia e anúncio", executionReviewed: "Execução em revisão", starting: "Iniciando dedicado", ready: "Origem pronta para revisão", socialSource: "Origem reservada pela sessão Social", socialSourceReviewing: "Revalidando a origem atual", socialSourceReviewed: "Revalidação em revisão", socialSourceApplying: "Confirmando servidor para nova sessão", reviewingHost: "Observando origem", hostReviewed: "Host em revisão", binding: "Abrindo transporte", bound: "Transporte associado", transportClosed: "Transporte encerrado", reviewExpired: "Revisão expirada", stopping: "Recolhendo recursos", stopped: "Origem encerrada", restoreReviewed: "Restauração do anúncio em revisão", eulaRestoreReviewed: "Recuperação do EULA em revisão", restoringEula: "Recuperando EULA", restored: "Recuperação concluída", needsRecovery: "Recuperação necessária", failed: "Operação encerrada com falha" };

const udpLabels: Record<string, string> = { notStarted: "Não iniciado", enabled: "Transporte disponível", retiring: "Encerrando", retired: "Encerrado nesta sessão", closed: "Encerrado", localUnavailable: "Indisponível neste computador", cleanupFailed: "Encerramento não confirmado" };
const eulaLabels: Record<string, string> = { unchanged: "Sem alteração", ownedBySource: "Aplicado ao servidor dedicado", restored: "Original restaurado", needsRecovery: "Recuperação necessária" };
export const nativeVoiceUdpLabel = (value: string) => Object.hasOwn(udpLabels, value) ? udpLabels[value] : "Estado não reconhecido";
export const nativeVoiceEulaLabel = (value: string) => Object.hasOwn(eulaLabels, value) ? eulaLabels[value] : "Estado não reconhecido";

/** Presentation only. A native recipe stays opaque to every IPC action. */
export function voiceReviewDetails(details: string[]) {
  const summary: string[] = [], technical: string[] = [], copies: string[] = [];
  for (const detail of details) {
    let decoded: unknown;
    try { decoded = JSON.parse(detail); } catch { /* Plain native review text. */ }
    if (object(decoded) || Array.isArray(decoded)) {
      technical.push(detail);
      if (object(decoded) && string(decoded.cwd, 4096) && decoded.cwd) copies.push(decoded.cwd);
    } else summary.push(detail);
  }
  return { summary, technical, copies };
}

export type VoiceReviewDeadline = { digest: string; kind: VoiceApproval["kind"]; at: number };
/** Only read-only snapshot round trips may establish this display deadline. */
export function voiceReviewDeadline(previous: VoiceReviewDeadline | undefined, review: VoiceApproval, sentAt: number): VoiceReviewDeadline {
  const candidate = sentAt + review.remainingMs;
  return { digest: review.digest, kind: review.kind, at: previous?.digest === review.digest && previous.kind === review.kind ? Math.min(previous.at, candidate) : candidate };
}
