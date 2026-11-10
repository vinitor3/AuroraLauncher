export const socialRuntimeLabels = {
  "fabric-loader-0.16.14-1.21.1": "Minecraft 1.21.1 · Fabric 0.16.14",
  "fabric-loader-0.16.14-1.20.1": "Minecraft 1.20.1 · Fabric 0.16.14",
  "fabric-loader-0.16.14-1.19.2": "Minecraft 1.19.2 · Fabric 0.16.14",
  "fabric-loader-0.13.3-1.16.5": "Minecraft 1.16.5 · Fabric 0.13.3",
  "1.21.1-forge-52.1.12": "Minecraft 1.21.1 · Forge 52.1.12",
  "1.20.1-forge-47.4.23": "Minecraft 1.20.1 · Forge 47.4.23",
  "1.19.2-forge-43.4.16": "Minecraft 1.19.2 · Forge 43.4.16",
  "1.16.5-forge-36.2.42": "Minecraft 1.16.5 · Forge 36.2.42",
  "1.12.2-forge-14.23.5.2860": "Minecraft 1.12.2 · Forge 14.23.5.2860",
} as const;
export type SocialConnectStage = "reserved" | "authenticating" | "sourceChecking" | "hostReview" | "guestReview" | "waitingGuest" | "connecting" | "connected" | "closed" | "failed";
export const socialFailureStageText = {
  authentication: "Autenticação da conta",
  bootstrap: "Abertura da sessão",
  identity: "Confirmação da identidade",
  keyChallenge: "Validação da chave da conexão",
  hostReview: "Revisão do compartilhamento",
  hostRegistration: "Registro do anfitrião",
  guestAdmission: "Admissão do participante",
  sessionContinuity: "Manutenção da sessão",
  relay: "Autorização da rota de conexão",
  sessionControl: "Controle da sessão",
} as const;
export const socialFailureGateText = {
  hostPoll: "Atualização da autorização do anfitrião", source: "Origem dedicada", relayTicket: "Autorização da rota",
  invitation: "Convite original", session: "Prazo da sessão", hostTransport: "Transporte do anfitrião", other: "Conexão",
} as const;
export type SocialFailureObservation = {
  gate: keyof typeof socialFailureGateText; bootstrapRoundtripMs: number | null; bootstrapUncertaintyMs: number | null;
  hostPollElapsedMs: number | null; hostPollAgeMs: number | null; sessionRemainingMs: number;
};
export type SocialConnectProjection = {
  sessionId: string; role: "host" | "guest"; stage: SocialConnectStage;
  digest: string | null; gameTarget: string | null; visibility: "public" | "friends" | null; expiresAtMs: number | null;
  readyGameLaunchAvailable?: true;
  voiceProfile?: "svc" | "plasmo"; voiceState?: "notStarted" | "enabled" | "retiring" | "retired" | "failed" | "closed";
  hostPlayer?: boolean; maxGuests?: 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8;
  hostPlayerVoiceState?: "enabled" | "retired" | "failed" | "closed";
  runtimeProfile?: keyof typeof socialRuntimeLabels;
  voiceTransportProfile?: "nativeVoice8V1";
  failureStage?: keyof typeof socialFailureStageText;
  failureObservation?: SocialFailureObservation;
  participants?: { grantId: string; accountId: string; state: "waiting" | "connected" | "disconnected" | "removing" }[];
};
export type SocialConnectCapabilities = { available: boolean; experimental: true; accountEpoch: number; maxSessionSeconds: 60 | 14400; ownedGameTcpDiscovery?: boolean };
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value);
export const socialSessionId = (value: unknown): value is string => typeof value === "string" && /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(value);
export const socialGameTarget = (value: unknown): value is string => typeof value === "string" && /^127\.0\.0\.1:[1-9][0-9]{0,4}$/.test(value) && Number(value.split(":")[1]) <= 65535;
const integer = (value: unknown): value is number => Number.isSafeInteger(value) && (value as number) >= 0;
const invalid = (): never => { throw new Error("Invalid social Connect projection"); };
function failureObservation(value: unknown): value is SocialFailureObservation {
  const optional = ["bootstrapRoundtripMs", "bootstrapUncertaintyMs", "hostPollElapsedMs", "hostPollAgeMs"];
  const bounded = (value: unknown) => integer(value) && value <= 14_400_000;
  return record(value) && Object.keys(value).length === 6 && typeof value.gate === "string" && Object.hasOwn(socialFailureGateText, value.gate)
    && optional.every(key => Object.hasOwn(value, key) && (value[key] === null || bounded(value[key]))) && bounded(value.sessionRemainingMs);
}
export function socialConnectCapabilitiesFrom(value: unknown): SocialConnectCapabilities {
  if (!record(value) || Object.keys(value).some(key => !["available", "experimental", "accountEpoch", "maxSessionSeconds", "ownedGameTcpDiscovery"].includes(key)) || typeof value.available !== "boolean" || value.experimental !== true
    || !(value.ownedGameTcpDiscovery === undefined || typeof value.ownedGameTcpDiscovery === "boolean") || (value.ownedGameTcpDiscovery === true && value.available !== true)
    || !integer(value.accountEpoch) || ![60, 14400].includes(value.maxSessionSeconds as number)) return invalid();
  return value as SocialConnectCapabilities;
}
export function socialConnectProjectionFrom(value: unknown): SocialConnectProjection {
  if (!record(value) || Object.keys(value).length < 7 || Object.keys(value).length > 18
    || Object.keys(value).some(key => !["sessionId", "role", "stage", "digest", "gameTarget", "visibility", "expiresAtMs", "readyGameLaunchAvailable", "voiceProfile", "voiceState", "hostPlayer", "hostPlayerVoiceState", "maxGuests", "participants", "runtimeProfile", "failureStage", "voiceTransportProfile", "failureObservation"].includes(key))
    || !(value.failureStage === undefined || value.stage === "failed" && typeof value.failureStage === "string" && Object.hasOwn(socialFailureStageText, value.failureStage))
    || !(value.failureObservation === undefined || value.stage === "failed" && failureObservation(value.failureObservation))
    || !(value.voiceProfile === undefined || ["svc", "plasmo"].includes(String(value.voiceProfile)))
    || !(value.runtimeProfile === undefined || value.voiceProfile !== undefined && typeof value.runtimeProfile === "string" && Object.hasOwn(socialRuntimeLabels, value.runtimeProfile)
      && !(value.runtimeProfile === "1.12.2-forge-14.23.5.2860" && value.voiceProfile === "plasmo"))
    || !(value.voiceTransportProfile === undefined || value.voiceTransportProfile === "nativeVoice8V1" && value.runtimeProfile !== undefined && value.voiceProfile !== undefined && typeof value.hostPlayer === "boolean")
    || !(value.voiceState === undefined || value.voiceProfile !== undefined && ["notStarted", "enabled", "retiring", "retired", "failed", "closed"].includes(String(value.voiceState)))
    || !(value.hostPlayer === undefined || value.voiceProfile !== undefined && typeof value.hostPlayer === "boolean")
    || !(value.hostPlayerVoiceState === undefined || value.hostPlayer === true && ["enabled", "retired", "failed", "closed"].includes(String(value.hostPlayerVoiceState)))
    || !(value.maxGuests === undefined || integer(value.maxGuests) && value.maxGuests >= 1 && value.maxGuests <= 8)
    || !(value.participants === undefined || value.role === "host" && Array.isArray(value.participants) && value.participants.length <= 8
      && value.participants.every(person => record(person) && Object.keys(person).length === 3 && typeof person.grantId === "string" && /^[a-f0-9]{64}$/.test(person.grantId)
        && typeof person.accountId === "string" && /^[A-Za-z0-9_-]{1,128}$/.test(person.accountId) && ["waiting", "connected", "disconnected", "removing"].includes(String(person.state)))
      && (value.maxGuests === undefined || value.participants.length <= Number(value.maxGuests))
      && new Set(value.participants.map(person => person.grantId)).size === value.participants.length
      && new Set(value.participants.map(person => person.accountId)).size === value.participants.length)
    || !(value.readyGameLaunchAvailable === undefined || value.readyGameLaunchAvailable === true
      && (value.role === "guest" && value.stage === "connected" || value.role === "host" && value.hostPlayer === true && ["connected", "waitingGuest"].includes(String(value.stage)))) || !socialSessionId(value.sessionId)
    || !["host", "guest"].includes(value.role as string)
    || !["reserved", "authenticating", "sourceChecking", "hostReview", "guestReview", "waitingGuest", "connecting", "connected", "closed", "failed"].includes(value.stage as string)
    || !(value.digest === null || typeof value.digest === "string" && /^[0-9a-f]{64}$/.test(value.digest))
    || !(value.gameTarget === null || socialGameTarget(value.gameTarget))
    || ![null, "public", "friends"].includes(value.visibility as null | string)
    || !(value.expiresAtMs === null || integer(value.expiresAtMs))) return invalid();
  if (value.role === "guest" && (value.stage !== "guestReview" && value.digest !== null || value.gameTarget !== null || value.visibility !== null)) return invalid();
  if (value.stage === "sourceChecking" && (value.role !== "host" || value.gameTarget !== null || value.digest !== null)) return invalid();
  if (value.stage === "guestReview" && (value.role !== "guest" || value.voiceTransportProfile !== "nativeVoice8V1" || value.digest === null || value.expiresAtMs === null)) return invalid();
  if (value.stage === "hostReview" && (value.role !== "host" || value.digest === null || value.gameTarget === null || value.visibility === null || value.expiresAtMs === null)) return invalid();
  return value as SocialConnectProjection;
}
export function socialReservationFrom(value: unknown): { operationId: string; generation: number } {
  if (!record(value) || Object.keys(value).length !== 2 || !socialSessionId(value.operationId) || !integer(value.generation) || value.generation < 1) return invalid();
  return value as { operationId: string; generation: number };
}
export function socialConnectError(cause: unknown): string {
  if (cause === "native_voice_new_advertisement_required") return "Encerre o anúncio anterior e publique um novo para iniciar outra sessão com este servidor.";
  const code = typeof cause === "string" ? cause : record(cause) && typeof cause.code === "string" ? cause.code : "";
  const messages: Record<string, string> = {
    connect_busy: "Já existe uma conexão em andamento. Encerre-a antes de entrar em outra sessão.",
    connect_social_unavailable: "O serviço online está indisponível neste momento ou não está ativado neste executável.",
    connect_social_quota: "O limite gratuito de uso foi atingido. A conexão foi encerrada; tente novamente mais tarde.",
    connect_social_account_changed: "A conta mudou. Atualize a sessão antes de continuar.",
    connect_social_failed: "A sessão não está disponível ou a permissão mudou. Atualize a lista.",
    connect_social_auth_failed: "Sua conta não pôde ser confirmada. Entre novamente no Aurora.",
    connect_social_account_not_registered: "A conta ainda não está registrada no Social do Aurora. Abra a aba Social para concluir o cadastro.",
    connect_social_not_authorized: "Sua conta não tem acesso a esta sessão. Atualize a lista de sessões.",
    connect_social_authority_changed: "A autorização original da sessão mudou ou expirou. Atualize a lista de sessões.",
    connect_social_clock_uncertain: "O horário do serviço não pôde ser confirmado dentro do prazo. Tente iniciar uma nova conexão.",
    connect_social_clock_unverified: "A validação do horário assinado do serviço falhou. Atualize o Aurora antes de tentar novamente.",
    connect_social_request_timed_out: "O serviço online não respondeu dentro do prazo. A tentativa foi encerrada; prepare uma nova sessão.",
    connect_social_continuity_lost: "Não foi possível confirmar a continuidade da sessão a tempo. A conexão foi encerrada; prepare uma nova sessão.",
    connect_social_service_unavailable: "Não foi possível obter resposta do serviço online. Tente novamente em instantes.",
    connect_social_request_rejected: "Os dados desta etapa da conexão não puderam ser validados. Atualize a sessão antes de tentar novamente.",
    connect_social_epoch_mismatch: "A sessão da conta mudou. Atualize antes de continuar.",
    connect_plan_expired: "O prazo da sessão terminou. O anfitrião pode preparar outra sessão.",
    connect_port_unavailable: "A porta de conexão está ocupada. Nenhum outro processo foi encerrado.",
    connect_invitation_rejected: "A sessão não está disponível ou sua autorização mudou. Atualize a lista.",
    connect_transport_failed: "Não foi possível estabelecer ou manter a conexão. O servidor pode ter sido encerrado.",
    connect_cancelled: "Conexão cancelada.",
    connect_source_changed: "O jogo ou a porta selecionada mudou. Faça uma nova busca antes de compartilhar.",
    connect_disabled: "A conexão social não está disponível neste executável.",
    connect_stale_operation: "Esta tentativa não está mais disponível. Atualize o estado.",
    connect_account_cleanup_incomplete: "A conexão anterior ainda não foi encerrada. Tente encerrar novamente.",
    connect_cleanup_failed: "Não foi possível confirmar a liberação da conexão. Tente encerrar novamente.",
  };
  return Object.hasOwn(messages, code) ? messages[code] : "Não foi possível concluir a conexão. Atualize o estado antes de tentar novamente.";
}
export const socialStageText: Record<SocialConnectStage, string> = {
  reserved: "Preparando conexão…", authenticating: "Confirmando sua conta…", sourceChecking: "Conferindo o jogo selecionado…", hostReview: "Revise o compartilhamento", guestReview: "Revise a entrada com voz",
  waitingGuest: "Aguardando um participante", connecting: "Conectando…", connected: "Conexão autenticada",
  closed: "Conexão encerrada", failed: "Conexão indisponível",
};
