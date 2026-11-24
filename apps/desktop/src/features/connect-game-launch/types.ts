/** Strict presentation contracts. Only the native worker owns launch authority. */
const runtimeTargets = [
  ["1.21.1", "fabric", "0.16.14", 21],
  ["1.20.1", "fabric", "0.16.14", 17],
  ["1.19.2", "fabric", "0.16.14", 17],
  ["1.16.5", "fabric", "0.13.3", 8],
  ["1.21.1", "forge", "52.1.12", 21],
  ["1.20.1", "forge", "47.4.23", 17],
  ["1.19.2", "forge", "43.4.16", 17],
  ["1.16.5", "forge", "36.2.42", 8],
  ["1.12.2", "forge", "14.23.5.2860", 8],
] as const;
const voiceModSizes: Record<string, readonly [simple: number, plasmo?: number]> = {
  "forge-1.12.2": [5_875_010],
  "fabric-1.16.5": [5_138_361, 12_335_242],
  "forge-1.16.5": [4_921_098, 12_319_461],
  "fabric-1.19.2": [5_227_844, 12_352_072],
  "forge-1.19.2": [5_362_155, 12_335_111],
  "fabric-1.20.1": [5_279_173, 12_354_369],
  "forge-1.20.1": [4_923_439, 12_337_455],
  "fabric-1.21.1": [5_369_475, 12_365_966],
  "forge-1.21.1": [4_914_752, 12_341_307],
};
const voiceFabricApi: Record<string, readonly [string, number]> = {
  "1.16.5": ["mods/fabric-api-0.42.0+1.16.jar", 1_090_414],
  "1.19.2": ["mods/fabric-api-0.77.0+1.19.2.jar", 1_892_953],
  "1.20.1": ["mods/fabric-api-0.92.12+1.20.1.jar", 2_137_232],
  "1.21.1": ["mods/fabric-api-0.116.10+1.21.1.jar", 2_425_988],
};
type RuntimeTarget = typeof runtimeTargets[number];
export type ReadyGameRuntime = {
  minecraftVersion: RuntimeTarget[0]; loader: RuntimeTarget[1]; loaderVersion: RuntimeTarget[2];
  javaMajor: RuntimeTarget[3]; coreVersion: "1.1.7"; skinsVersion: "0.3.3";
};
export type ReadyGameLaunchCapability = {
  schemaVersion: 1; instanceScope: "diagnosticCopy"; maxPlanSeconds: 60;
  minecraftVersion: "1.21.1"; loader: "fabric"; loaderVersion: "0.16.14";
  javaMajor: 21; coreVersion: "1.1.7"; skinsVersion: "0.3.3";
} | { schemaVersion: 2 | 3; instanceScope: "diagnosticCopy"; maxPlanSeconds: 60; runtimes: ReadyGameRuntime[] };
export type ReadyGameAppearance = {
  skinUrl: string | null; skinFile: string | null; capeUrl: string | null; skinModel: "classic" | "slim" | null;
};
export type ReadyGameLaunchContext = {
  sourceInstanceId: string; contextKey: string; javaExecutable: string; nickname: string;
  appearance: ReadyGameAppearance;
};
export type ReadyGameLaunchRequest = ReadyGameLaunchContext & { operationId: string; instanceId: string };
export type ReadyVoiceReview = {
  profile: "simpleVoiceChat" | "plasmoVoice";
  mods: { relative: string; bytes: number; sha256: string; action: "install" | "present" }[];
};
export type ReadyGameLaunchReview = ReadyGameLaunchRequest & ReadyGameRuntime & {
  planId: string; digest: string; expiresAtMs: number; sessionExpiresAtMs: number;
  gameAddress: string; coreSha256: string; skinsSha256: string;
  voice?: ReadyVoiceReview;
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
  connect_ready_game_unsupported_runtime: "Esta cópia precisa de uma combinação de Minecraft e loader compatível com o Connect, com o perfil oficial preparado.",
  connect_ready_game_missing_artifact: "Faltam arquivos do jogo, Core ou Skins. Prepare a cópia pelo fluxo normal antes de continuar.",
  connect_ready_game_artifact_mismatch: "Algum arquivo do jogo, do Core, do Skins ou dos mods de voz difere do perfil esperado.",
  connect_ready_game_voice_artifacts_unavailable: "Não foi possível baixar ou verificar os mods fixados para a voz desta sessão.",
  connect_ready_game_server_status_unavailable: "O servidor não respondeu à verificação pela ponte desta sessão.",
  connect_ready_game_server_protocol_mismatch: "O servidor informou um protocolo diferente da versão Minecraft da cópia.",
  connect_ready_game_java_unavailable: "O Java escolhido não pôde ser verificado. Selecione uma instalação disponível nas configurações.",
  connect_ready_game_java_incompatible: "O Java escolhido precisa corresponder à versão do Minecraft desta cópia.",
  connect_ready_game_conflicting_arguments: "O perfil contém opções de entrada incompatíveis. Revise a cópia pelo fluxo normal.",
  connect_ready_game_launch_failed: "Não foi possível iniciar a cópia. Consulte o Doctor antes de tentar novamente.",
  connect_ready_copy_install_failed: "Não foi possível concluir o runtime oficial. Uma cópia incompleta pode ser removida em Cópias de teste.",
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
const runtimeFields = ["minecraftVersion", "loader", "loaderVersion", "javaMajor", "coreVersion", "skinsVersion"];
const validRuntime = (value: Record<string, unknown>, targets: readonly RuntimeTarget[] = runtimeTargets) =>
  value.coreVersion === "1.1.7" && value.skinsVersion === "0.3.3"
  && targets.some(([minecraftVersion, loader, loaderVersion, javaMajor]) =>
    value.minecraftVersion === minecraftVersion && value.loader === loader
    && value.loaderVersion === loaderVersion && value.javaMajor === javaMajor);
export function readyGameCapabilityFrom(value: unknown): ReadyGameLaunchCapability {
  if (!object(value) || value.instanceScope !== "diagnosticCopy" || value.maxPlanSeconds !== 60) return invalid();
  if (value.schemaVersion === 1) {
    if (!exact(value, ["schemaVersion", "instanceScope", "maxPlanSeconds", ...runtimeFields])
      || !validRuntime(value, runtimeTargets.slice(0, 1))) return invalid();
  } else if (value.schemaVersion === 2 || value.schemaVersion === 3) {
    const targets = value.schemaVersion === 2 ? runtimeTargets.slice(0, 2) : runtimeTargets;
    if (!exact(value, ["schemaVersion", "instanceScope", "maxPlanSeconds", "runtimes"])
      || !Array.isArray(value.runtimes) || value.runtimes.length !== targets.length
      || !value.runtimes.every(runtime => object(runtime) && exact(runtime, runtimeFields) && validRuntime(runtime, targets))
      || new Set(value.runtimes.map(runtime => JSON.stringify([runtime.minecraftVersion, runtime.loader]))).size !== targets.length) return invalid();
  } else return invalid();
  return value as ReadyGameLaunchCapability;
}
export function readyGameCapabilitySupports(capability: ReadyGameLaunchCapability | undefined, runtime: ReadyGameRuntime): boolean {
  if (!capability) return false;
  const allowed = capability.schemaVersion === 1 ? [capability] : capability.runtimes;
  return allowed.some(profile => profile.minecraftVersion === runtime.minecraftVersion && profile.loader === runtime.loader
    && profile.loaderVersion === runtime.loaderVersion && profile.javaMajor === runtime.javaMajor
    && profile.coreVersion === runtime.coreVersion && profile.skinsVersion === runtime.skinsVersion);
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
  const fields = ["operationId", "planId", "digest", "expiresAtMs", "sessionExpiresAtMs", "instanceId", "sourceInstanceId", "contextKey", "minecraftVersion", "loader", "loaderVersion", "javaExecutable", "javaMajor", "nickname", "appearance", "gameAddress", "coreVersion", "coreSha256", "skinsVersion", "skinsSha256"];
  if (!object(value) || !exact(value, Object.hasOwn(value, "voice") ? [...fields, "voice"] : fields)
    || !uuid(value.operationId) || !uuid(value.planId) || !copyId(value.instanceId) || !hash(value.digest)
    || !integer(value.expiresAtMs) || !integer(value.sessionExpiresAtMs) || value.expiresAtMs === 0 || value.expiresAtMs > value.sessionExpiresAtMs
    || !validRuntime(value)
    || typeof value.gameAddress !== "string" || !/^127\.0\.0\.1:[1-9][0-9]{0,4}$/.test(value.gameAddress) || Number(value.gameAddress.slice(10)) > 65535
    || value.coreVersion !== "1.1.7" || !hash(value.coreSha256) || value.skinsVersion !== "0.3.3" || !hash(value.skinsSha256)) return invalid();
  if (Object.hasOwn(value, "voice")) {
    const voice = value.voice;
    if (!object(voice) || !exact(voice, ["profile", "mods"])) return invalid();
    const runtime = `${value.loader}-${value.minecraftVersion}`;
    const sizes = voiceModSizes[runtime];
    let mods: (readonly [string, number])[];
    if (voice.profile === "simpleVoiceChat" && sizes) {
      mods = [[`mods/voicechat-${runtime}-2.6.22.jar`, sizes[0]]];
    } else if (voice.profile === "plasmoVoice" && sizes?.[1] !== undefined) {
      mods = [[`mods/plasmovoice-${runtime}-2.1.17.jar`, sizes[1]]];
      if (value.loader === "fabric") mods.push(voiceFabricApi[String(value.minecraftVersion)]);
    } else return invalid();
    if (!Array.isArray(voice.mods) || voice.mods.length !== mods.length
      || !voice.mods.every((mod, index) => object(mod) && exact(mod, ["relative", "bytes", "sha256", "action"])
        && mod.relative === mods[index][0] && mod.bytes === mods[index][1] && hash(mod.sha256)
        && (mod.action === "install" || mod.action === "present"))) return invalid();
  }
  if (!validReadyGameContext(value)) return invalid();
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
