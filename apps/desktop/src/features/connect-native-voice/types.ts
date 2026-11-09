export type VoiceProfile = "simpleVoiceChat" | "plasmoVoice";
export type VoiceApproval = { kind: "files" | "execution" | "host" | "restore"; digest: string; before: string; after: string; details: string[] };
export type VoiceParticipant = { participantId: string; publicId: string; localPlayer: boolean };
export type NativeVoiceSnapshot = { revision: number; available: boolean; manualLabAvailable?: boolean; phase: string; profile: VoiceProfile | null;
  review: VoiceApproval | null; operationId: string | null; hostPublicId: string | null; participants: VoiceParticipant[];
  udpState: string; tcpAddress: string | null; negotiatedDatagramBytes: number | null; recoveryPending: boolean;
  recoverableCopies: { bundleId: string; receiptId: string; profile: VoiceProfile }[]; eulaState: string; errorCode: string | null; exportedInvitation: string | null; audioState: "notVerified" };
const object = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value);
const string = (value: unknown, max = 4096): value is string => typeof value === "string" && value.length <= max;
const nullableString = (value: unknown, max = 4096) => value === null || string(value, max);
const phases = new Set(["idle", "preparing", "filesReviewed", "creating", "executionReviewed", "starting", "ready", "socialSource", "reviewingHost", "hostReviewed", "binding", "bound", "transportClosed", "reviewExpired", "stopping", "stopped", "restoreReviewed", "restored", "needsRecovery"]);
export function nativeVoiceSnapshotFrom(value: unknown): NativeVoiceSnapshot {
  if (!object(value) || !Number.isSafeInteger(value.revision) || Number(value.revision) < 0 || typeof value.available !== "boolean"
    || !(value.manualLabAvailable === undefined || typeof value.manualLabAvailable === "boolean" && (!value.manualLabAvailable || value.available))
    || !string(value.phase) || !phases.has(value.phase) || ![null, "simpleVoiceChat", "plasmoVoice"].includes(value.profile as null)
    || !nullableString(value.operationId, 128) || !nullableString(value.hostPublicId, 128)
    || !Array.isArray(value.participants) || value.participants.length > 8 || !value.participants.every(p => object(p)
      && string(p.participantId, 128) && string(p.publicId, 128) && typeof p.localPlayer === "boolean")
    || !["notStarted", "enabled", "retiring", "retired", "closed", "localUnavailable"].includes(String(value.udpState))
    || !nullableString(value.tcpAddress, 64) || (value.tcpAddress !== null && (!/^127\.0\.0\.1:[1-9][0-9]{0,4}$/.test(String(value.tcpAddress)) || Number(String(value.tcpAddress).split(":")[1]) > 65535))
    || !(value.negotiatedDatagramBytes === null || Number.isSafeInteger(value.negotiatedDatagramBytes) && Number(value.negotiatedDatagramBytes) >= 4096 && Number(value.negotiatedDatagramBytes) <= 65535)
    || !Array.isArray(value.recoverableCopies) || value.recoverableCopies.length > 64 || !value.recoverableCopies.every(c => object(c) && string(c.bundleId, 128) && string(c.receiptId, 128) && ["simpleVoiceChat", "plasmoVoice"].includes(String(c.profile)))
    || typeof value.recoveryPending !== "boolean" || !["unchanged", "ownedBySource", "restored", "needsRecovery"].includes(String(value.eulaState))
    || !nullableString(value.errorCode, 128) || !nullableString(value.exportedInvitation, 32768) || value.audioState !== "notVerified") throw new Error("native_voice_invalid_snapshot");
  if (value.review !== null) {
    const r = value.review;
    if (!object(r) || !["files", "execution", "host", "restore"].includes(String(r.kind)) || !string(r.digest, 64) || !/^[a-f0-9]{64}$/.test(r.digest)
      || !string(r.before, 16384) || !string(r.after, 16384) || !Array.isArray(r.details) || r.details.length > 20 || !r.details.every(d => string(d, 4096))) throw new Error("native_voice_invalid_review");
  }
  return value as NativeVoiceSnapshot;
}
export function nativeVoiceError(value: unknown): string {
  const code = typeof value === "string" ? value : "";
  if (code === "native_voice_pinned_resources_unavailable") return "Não foi possível baixar ou verificar os arquivos fixados deste perfil. Confira a conexão e tente novamente.";
  if (code === "native_voice_cancelled") return "O preparo dos recursos foi cancelado.";
  if (code === "native_voice_local_udp_in_use") return "A porta local 35554 já está ocupada. O transporte desta tentativa foi encerrado.";
  if (code === "native_voice_java_running") return "Encerre o Java antes de aplicar ou restaurar o anúncio.";
  if (code === "native_voice_restore_review_failed") return "A restauração estrita não foi aprovada. Você pode revisar a restauração somente das chaves de anúncio.";
  if (code.includes("expired") || code.includes("stale")) return "A revisão expirou ou mudou. Prepare uma nova revisão.";
  if (code === "native_voice_busy") return "Uma operação nativa ainda está em andamento.";
  return "A operação de voz foi recusada. Confira o estado nativo antes de tentar novamente.";
}
export const voicePhase: Record<string, string> = { idle: "Sem sessão", preparing: "Baixando ou verificando recursos", filesReviewed: "Arquivos em revisão", creating: "Preparando cópia e anúncio", executionReviewed: "Execução em revisão", starting: "Iniciando dedicado", ready: "Origem pronta para revisão", socialSource: "Origem reservada pela sessão Social", reviewingHost: "Observando origem", hostReviewed: "Host em revisão", binding: "Abrindo transporte", bound: "Transporte associado", transportClosed: "Transporte encerrado", reviewExpired: "Revisão expirada", stopping: "Recolhendo recursos", stopped: "Origem encerrada", restoreReviewed: "Restauração em revisão", restored: "Anúncio restaurado", needsRecovery: "Recuperação necessária" };
