/** Strict presentation contracts. Only the native worker owns launch authority. */
export type ReadyGameLaunchCapability = {
  schemaVersion: 1; instanceScope: "diagnosticCopy"; maxPlanSeconds: 60;
  minecraftVersion: "1.21.1"; loader: "fabric"; loaderVersion: "0.16.14";
  javaMajor: 21; coreVersion: "1.1.5"; skinsVersion: "0.3.1";
};
export type ReadyGameAppearance = {
  skinUrl: string | null; skinFile: string | null; capeUrl: string | null; skinModel: "classic" | "slim" | null;
};
export type ReadyGameLaunchContext = {
  sourceInstanceId: string; contextKey: string; javaExecutable: string; nickname: string;
  appearance: ReadyGameAppearance;
};
export type ReadyGameLaunchRequest = ReadyGameLaunchContext & { operationId: string; instanceId: string };
export type ReadyGameLaunchReview = ReadyGameLaunchRequest & {
  planId: string; digest: string; expiresAtMs: number; sessionExpiresAtMs: number;
  minecraftVersion: "1.21.1"; loader: "fabric"; loaderVersion: "0.16.14";
  javaMajor: 21; gameAddress: string; coreVersion: "1.1.5"; coreSha256: string;
  skinsVersion: "0.3.1"; skinsSha256: string;
};
export type ReadyGameLaunchResult = { runId: string; processId: number; instanceId: string; worldState: "notVerified" };
export type ReadyGameLaunchSnapshot = {
  operationId: string; generation: number; planId: string | null;
  phase: "idle" | "preparing" | "reviewed" | "starting" | "started" | "failed" | "cancelled" | "expired";
  review: ReadyGameLaunchReview | null; result: ReadyGameLaunchResult | null; errorCode: string | null;
};
const errors: Record<string, string> = {
  connect_ready_game_disabled: "A entrada experimental pelo launcher não está habilitada nesta versão.",
  connect_ready_game_invalid_request: "Confira a cópia, o Java e o perfil selecionados antes de revisar novamente.",
  connect_ready_game_wrong_owner: "Esta preparação pertence a outra janela do launcher.",
  connect_ready_game_stale_operation: "Esta conexão não está mais disponível. Consulte o estado do Connect.",
  connect_ready_game_busy: "Já existe uma preparação de jogo em andamento. Aguarde ou cancele esse preparo.",
  connect_ready_game_unavailable: "A ponte TCP desta conexão não está mais disponível. Nenhum convite foi renovado.",
  connect_ready_game_expired: "A revisão venceu. Prepare uma nova revisão enquanto a conexão estiver disponível.",
  connect_ready_game_approval_mismatch: "A revisão mudou. Confira os dados e autorize novamente.",
  connect_ready_game_cancelled: "O preparo do jogo foi cancelado.",
  connect_ready_game_instance_not_ready: "Selecione uma cópia de teste pronta e verificada na seção Cópias de teste.",
  connect_ready_game_instance_changed: "A cópia ou algum arquivo mudou. Prepare uma nova revisão.",
  connect_ready_game_instance_busy: "Esta cópia está em uso. Feche seu jogo antes de tentar novamente.",
  connect_ready_game_recovery_pending: "A cópia tem uma recuperação pendente. Resolva-a antes de abrir o jogo.",
  connect_ready_game_unsupported_runtime: "Esta entrada experimental exige Minecraft 1.21.1 com Fabric 0.16.14 já preparados.",
  connect_ready_game_missing_artifact: "Faltam arquivos do jogo, Core ou Skins. Prepare a cópia pelo fluxo normal antes de continuar.",
  connect_ready_game_artifact_mismatch: "Os arquivos Core ou Skins não correspondem às versões verificadas desta integração.",
  connect_ready_game_java_unavailable: "O Java escolhido não pôde ser verificado. Selecione uma instalação disponível nas configurações.",
  connect_ready_game_java_incompatible: "Esta cópia exige Java 21 já instalado. Confira o Java selecionado nas configurações.",
  connect_ready_game_conflicting_arguments: "O perfil contém opções de entrada incompatíveis. Revise a cópia pelo fluxo normal.",
  connect_ready_game_launch_failed: "Não foi possível iniciar a cópia. Consulte o Doctor antes de tentar novamente.",
  connect_ready_game_internal: "Não foi possível concluir o preparo. Consulte o estado antes de tentar novamente.",
};
export const readyGameError = (cause: unknown) => typeof cause === "string" && Object.hasOwn(errors, cause)
  ? errors[cause] : "A resposta não pôde ser confirmada. Consulte o estado; o launcher não repetirá o lançamento automaticamente.";
export const isReadyGameError = (cause: unknown): cause is string => typeof cause === "string" && Object.hasOwn(errors, cause);
const object = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, maximum: number): value is string => typeof value === "string" && value.length > 0 && value.length <= maximum && !/[\u0000-\u001f\u007f]/u.test(value);
const integer = (value: unknown): value is number => Number.isSafeInteger(value) && (value as number) >= 0;
const uuid = (value: unknown): value is string => typeof value === "string" && /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(value);
const copyId = (value: unknown): value is string => typeof value === "string" && value.startsWith("aurora-test-") && uuid(value.slice(12));
const hash = (value: unknown): value is string => typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
const exact = (value: Record<string, unknown>, fields: string[]) => Object.keys(value).length === fields.length && fields.every(field => Object.hasOwn(value, field));
const invalid = (): never => { throw new Error("Invalid ready game launch projection"); };
export function readyGameCapabilityFrom(value: unknown): ReadyGameLaunchCapability {
  if (!object(value) || !exact(value, ["schemaVersion", "instanceScope", "maxPlanSeconds", "minecraftVersion", "loader", "loaderVersion", "javaMajor", "coreVersion", "skinsVersion"])
    || value.schemaVersion !== 1 || value.instanceScope !== "diagnosticCopy" || value.maxPlanSeconds !== 60
    || value.minecraftVersion !== "1.21.1" || value.loader !== "fabric" || value.loaderVersion !== "0.16.14"
    || value.javaMajor !== 21 || value.coreVersion !== "1.1.5" || value.skinsVersion !== "0.3.1") return invalid();
  return value as ReadyGameLaunchCapability;
}
function appearanceFrom(value: unknown): ReadyGameAppearance {
  if (!object(value) || !exact(value, ["skinUrl", "skinFile", "capeUrl", "skinModel"])
    || !(value.skinUrl === null || text(value.skinUrl, 2048)) || !(value.skinFile === null || text(value.skinFile, 4096))
    || !(value.capeUrl === null || text(value.capeUrl, 2048)) || ![null, "classic", "slim"].includes(value.skinModel as string | null)) return invalid();
  return value as ReadyGameAppearance;
}
export function validReadyGameContext(value: unknown): value is ReadyGameLaunchContext {
  if (!object(value) || !text(value.sourceInstanceId, 96) || !text(value.contextKey, 512)
    || !text(value.javaExecutable, 4096) || typeof value.nickname !== "string" || !/^[A-Za-z0-9_]{3,16}$/.test(value.nickname)) return false;
  try { appearanceFrom(value.appearance); return true; } catch { return false; }
}
function reviewFrom(value: unknown): ReadyGameLaunchReview {
  if (!object(value) || !exact(value, ["operationId", "planId", "digest", "expiresAtMs", "sessionExpiresAtMs", "instanceId", "sourceInstanceId", "contextKey", "minecraftVersion", "loader", "loaderVersion", "javaExecutable", "javaMajor", "nickname", "appearance", "gameAddress", "coreVersion", "coreSha256", "skinsVersion", "skinsSha256"])
    || !uuid(value.operationId) || !uuid(value.planId) || !copyId(value.instanceId) || !hash(value.digest)
    || !integer(value.expiresAtMs) || !integer(value.sessionExpiresAtMs) || value.expiresAtMs === 0 || value.expiresAtMs > value.sessionExpiresAtMs
    || value.minecraftVersion !== "1.21.1" || value.loader !== "fabric" || value.loaderVersion !== "0.16.14" || value.javaMajor !== 21
    || typeof value.gameAddress !== "string" || !/^127\.0\.0\.1:[1-9][0-9]{0,4}$/.test(value.gameAddress) || Number(value.gameAddress.slice(10)) > 65535
    || value.coreVersion !== "1.1.5" || !hash(value.coreSha256) || value.skinsVersion !== "0.3.1" || !hash(value.skinsSha256)
    || !validReadyGameContext(value)) return invalid();
  return value as ReadyGameLaunchReview;
}
export function readyGameSnapshotFrom(value: unknown): ReadyGameLaunchSnapshot {
  if (!object(value) || !exact(value, ["operationId", "generation", "planId", "phase", "review", "result", "errorCode"])
    || !uuid(value.operationId) || !integer(value.generation)
    || !["idle", "preparing", "reviewed", "starting", "started", "failed", "cancelled", "expired"].includes(value.phase as string)
    || !(value.errorCode === null || isReadyGameError(value.errorCode))) return invalid();
  if (value.phase === "idle") {
    if (value.planId !== null || value.generation !== 0 || value.review !== null || value.result !== null || value.errorCode !== null) return invalid();
  } else if (!uuid(value.planId) || value.generation === 0) return invalid();
  if (value.phase === "reviewed") {
    const review = reviewFrom(value.review);
    if (review.operationId !== value.operationId || review.planId !== value.planId || value.errorCode !== null) return invalid();
  } else if (value.review !== null) return invalid();
  if (value.phase === "started") {
    const result = value.result;
    if (!object(result) || !exact(result, ["runId", "processId", "instanceId", "worldState"]) || !uuid(result.runId)
      || !integer(result.processId) || result.processId < 1 || result.processId > 0xffffffff || !copyId(result.instanceId)
      || result.worldState !== "notVerified" || value.errorCode !== null) return invalid();
  } else if (value.result !== null) return invalid();
  if (value.phase === "failed" && value.errorCode === null) return invalid();
  if (["preparing", "starting"].includes(value.phase as string) && value.errorCode !== null) return invalid();
  if (value.phase === "cancelled" && value.errorCode !== null && value.errorCode !== "connect_ready_game_cancelled") return invalid();
  if (value.phase === "expired" && value.errorCode !== null && value.errorCode !== "connect_ready_game_expired") return invalid();
  return value as ReadyGameLaunchSnapshot;
}
export function sameReadyGameContext(review: ReadyGameLaunchContext, context: ReadyGameLaunchContext | undefined) {
  return validReadyGameContext(context) && review.sourceInstanceId === context.sourceInstanceId && review.contextKey === context.contextKey
    && review.javaExecutable === context.javaExecutable && review.nickname === context.nickname
    && review.appearance.skinUrl === context.appearance.skinUrl && review.appearance.skinFile === context.appearance.skinFile
    && review.appearance.capeUrl === context.appearance.capeUrl && review.appearance.skinModel === context.appearance.skinModel;
}
export function readyGameContextFingerprint(context: ReadyGameLaunchContext | undefined) {
  return validReadyGameContext(context) ? JSON.stringify([context.sourceInstanceId, context.contextKey, context.javaExecutable, context.nickname,
    context.appearance.skinUrl, context.appearance.skinFile, context.appearance.capeUrl, context.appearance.skinModel]) : "";
}
