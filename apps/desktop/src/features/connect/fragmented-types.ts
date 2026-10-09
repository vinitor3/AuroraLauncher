// Transport-only profile. These projections do not attest a mod, its
// announcement/configuration, a player joining, or audio availability.
export const FRAGMENTED_PROFILE = "fragmentedGameUdp" as const;
export const FRAGMENTED_LIFETIME_SECONDS = 120 as const;
export const FRAGMENTED_UDP_BIND = "127.0.0.1:35554" as const;

export type FragmentedUdpContract = {
  profile: typeof FRAGMENTED_PROFILE;
  invitationSchemaVersion: 1;
  udpEnvelopeVersion: 2;
  approvedOriginalDatagramBytes: 4096;
  maxFragmentFrameBytes: 1264;
  localUdpBind: typeof FRAGMENTED_UDP_BIND;
  udpReturnPolicy: { type: "authorizedDownstream"; bytesPerSecond: 16384; datagramsPerSecond: 20 };
};
export type FragmentedUdpCapability = FragmentedUdpContract & { maxLifetimeSeconds: typeof FRAGMENTED_LIFETIME_SECONDS };
export type FragmentedUdpReview = FragmentedUdpContract & { udpTarget: string | null };
export type FragmentedUdpState = "notStarted" | "starting" | "available" | "failed" | "closed";
export type FragmentedUdpError = "connect_udp_setup_failed" | "connect_udp_capacity_unavailable" | "connect_udp_closed";
export type FragmentedGuestUdp = {
  udpState: FragmentedUdpState;
  udpAddress: typeof FRAGMENTED_UDP_BIND | null;
  udpErrorCode: FragmentedUdpError | null;
  udpOriginalCapacity: 4096 | null;
  udpCapacityScope: "initialAssociation" | null;
};
type RecordValue = Record<string, unknown>;
const invalid = (): never => { throw new Error("Invalid Connect fragmented projection"); };
const record = (value: unknown): value is RecordValue => !!value && typeof value === "object" && !Array.isArray(value);
const integer = (value: unknown): value is number => Number.isSafeInteger(value) && (value as number) >= 0;
const exact = (value: unknown, keys: readonly string[]): value is RecordValue => record(value)
  && Object.keys(value).length === keys.length && keys.every(key => Object.hasOwn(value, key));
const CONTRACT_KEYS = ["profile", "invitationSchemaVersion", "udpEnvelopeVersion", "approvedOriginalDatagramBytes", "maxFragmentFrameBytes", "localUdpBind", "udpReturnPolicy"];
const UDP_KEYS = ["udpState", "udpAddress", "udpErrorCode", "udpOriginalCapacity", "udpCapacityScope"];
const PHASES = ["hostPrepared", "guestPrepared", "reviewed", "starting", "hostListening", "guestConnected", "closing", "closed", "failed"];
const UDP_ERRORS: FragmentedUdpError[] = ["connect_udp_setup_failed", "connect_udp_capacity_unavailable", "connect_udp_closed"];

function noSession(value: RecordValue) {
  if (value.sessionTiming !== undefined) invalid();
}
function contract(value: unknown, extraKeys: readonly string[] = []): RecordValue {
  if (!exact(value, [...CONTRACT_KEYS, ...extraKeys]) || value.profile !== FRAGMENTED_PROFILE
    || value.invitationSchemaVersion !== 1 || value.udpEnvelopeVersion !== 2
    || value.approvedOriginalDatagramBytes !== 4096 || value.maxFragmentFrameBytes !== 1264
    || value.localUdpBind !== FRAGMENTED_UDP_BIND
    || !exact(value.udpReturnPolicy, ["type", "bytesPerSecond", "datagramsPerSecond"])
    || value.udpReturnPolicy.type !== "authorizedDownstream" || value.udpReturnPolicy.bytesPerSecond !== 16384
    || value.udpReturnPolicy.datagramsPerSecond !== 20) return invalid();
  return value;
}
export function fragmentedContractFrom(value: unknown): FragmentedUdpContract {
  return contract(value) as FragmentedUdpContract;
}
export function fragmentedCapabilityFrom(value: unknown): FragmentedUdpCapability {
  const checked = contract(value, ["maxLifetimeSeconds"]);
  if (checked.maxLifetimeSeconds !== FRAGMENTED_LIFETIME_SECONDS) return invalid();
  return checked as FragmentedUdpCapability;
}
export function isFragmentedLifetime(value: unknown): value is number {
  return integer(value) && value >= 1 && value <= FRAGMENTED_LIFETIME_SECONDS;
}
export function isLoopbackUdpTarget(value: unknown): value is string {
  if (typeof value !== "string" || value.length > 128) return false;
  const ipv6 = /^\[::1\]:([1-9][0-9]{0,4})$/.exec(value);
  if (ipv6) return Number(ipv6[1]) <= 65535;
  const ipv4 = /^(127\.(?:0|[1-9][0-9]{0,2})\.(?:0|[1-9][0-9]{0,2})\.(?:0|[1-9][0-9]{0,2})):([1-9][0-9]{0,4})$/.exec(value);
  return !!ipv4 && ipv4[1].split(".").every(part => Number(part) <= 255) && Number(ipv4[2]) <= 65535;
}
export function fragmentedPreparationFieldsFrom(value: unknown): { invitationProfile: typeof FRAGMENTED_PROFILE } {
  if (!record(value) || value.invitationProfile !== FRAGMENTED_PROFILE || value.fragmentedUdp !== undefined
    || value.udpTarget !== undefined || UDP_KEYS.some(key => value[key] !== undefined)) return invalid();
  noSession(value);
  return value as { invitationProfile: typeof FRAGMENTED_PROFILE };
}
export function fragmentedReviewFieldsFrom(value: unknown): FragmentedUdpReview {
  if (!record(value) || value.invitationProfile !== undefined || !["host", "guest"].includes(value.role as string)
    || !["loopback", "externalIpv6"].includes(value.mode as string) || !isFragmentedLifetime(value.lifetimeSeconds)
    || !integer(value.expiresAtMs) || !integer(value.invitationExpiresAtMs) || value.expiresAtMs > value.invitationExpiresAtMs
    || !Array.isArray(value.services) || value.services.length !== 2 || value.udpTarget !== undefined
    || UDP_KEYS.some(key => value[key] !== undefined)) return invalid();
  noSession(value);
  const checked = contract(value.fragmentedUdp, ["udpTarget"]);
  for (const [index, expected] of [[0, ["game", "tcp", "reliableStream"]], [1, ["aux", "udp", "fragmentedDatagram"]]] as const) {
    const service = value.services[index];
    if (!exact(service, ["serviceId", "protocol", "revision", "requiredCapability"])
      || service.serviceId !== expected[0] || service.protocol !== expected[1]
      || service.requiredCapability !== expected[2] || service.revision !== 1) return invalid();
  }
  if (value.instanceId != null || value.runId != null) return invalid();
  if (value.role === "host") {
    if (!isLoopbackUdpTarget(checked.udpTarget) || !isLoopbackUdpTarget(value.gameTarget) || value.localTcpBind !== null
      || !(value.targetSource === undefined || value.targetSource === "manual")) return invalid();
  } else if (checked.udpTarget !== null || value.gameTarget !== null || value.localTcpBind !== "127.0.0.1:0"
    || !(value.targetSource === undefined || value.targetSource === null)) return invalid();
  return checked as FragmentedUdpReview;
}
export function fragmentedExportFieldsFrom(value: unknown): FragmentedUdpContract {
  if (!record(value) || value.invitationProfile !== undefined || !integer(value.expiresAtMs)
    || value.udpTarget !== undefined || UDP_KEYS.some(key => value[key] !== undefined)) return invalid();
  noSession(value);
  return fragmentedContractFrom(value.fragmentedUdp);
}
export function fragmentedGuestUdpFrom(value: unknown): FragmentedGuestUdp {
  if (!record(value) || !UDP_KEYS.every(key => Object.hasOwn(value, key))
    || !(value.invitationProfile === undefined || value.invitationProfile === FRAGMENTED_PROFILE)
    || !["notStarted", "starting", "available", "failed", "closed"].includes(value.udpState as string)
    || !(value.udpErrorCode === null || UDP_ERRORS.includes(value.udpErrorCode as FragmentedUdpError))) return invalid();
  noSession(value);
  if (value.udpState === "available") {
    if (value.udpAddress !== FRAGMENTED_UDP_BIND || value.udpOriginalCapacity !== 4096
      || value.udpCapacityScope !== "initialAssociation" || value.udpErrorCode !== null) return invalid();
  } else {
    if (value.udpAddress !== null || value.udpOriginalCapacity !== null || value.udpCapacityScope !== null) return invalid();
    if (value.udpState === "failed" && value.udpErrorCode === null) return invalid();
    if (["notStarted", "starting"].includes(value.udpState as string) && value.udpErrorCode !== null) return invalid();
  }
  return value as FragmentedGuestUdp;
}
export function fragmentedSnapshotFieldsFrom(value: unknown): void {
  if (!record(value) || value.invitationProfile !== FRAGMENTED_PROFILE || !["host", "guest"].includes(value.role as string)
    || !PHASES.includes(value.phase as string) || value.udpTarget !== undefined) return invalid();
  noSession(value);
  if (["hostPrepared", "hostListening"].includes(value.phase as string) && value.role !== "host") return invalid();
  if (["guestPrepared", "guestConnected"].includes(value.phase as string) && value.role !== "guest") return invalid();
  const prepared = value.phase === "hostPrepared" || value.phase === "guestPrepared";
  if (prepared && value.fragmentedUdp !== undefined) return invalid();
  if (["reviewed", "starting", "hostListening", "guestConnected"].includes(value.phase as string) && value.fragmentedUdp === undefined) return invalid();
  if (value.fragmentedUdp !== undefined) fragmentedContractFrom(value.fragmentedUdp);
  if (value.preparation != null) fragmentedPreparationFieldsFrom(value.preparation);
  if (value.review != null) {
    if (!record(value.review) || value.review.role !== value.role || value.fragmentedUdp === undefined) return invalid();
    fragmentedReviewFieldsFrom(value.review);
  }
  if (value.role === "host") {
    if (UDP_KEYS.some(key => value[key] !== undefined)) return invalid();
  } else {
    const udp = fragmentedGuestUdpFrom(value);
    if (["available", "starting"].includes(udp.udpState) && !["guestConnected", "closing"].includes(value.phase as string)) return invalid();
    if (prepared && udp.udpState !== "notStarted") return invalid();
    if (value.phase === "closed" && udp.udpState !== "closed") return invalid();
  }
}
export const fragmentedUdpStateText: Record<FragmentedUdpState, string> = {
  notStarted: "Ponte UDP ainda não iniciada", starting: "Preparando ponte UDP…", available: "Ponte UDP disponível",
  failed: "Ponte UDP indisponível", closed: "Ponte UDP encerrada",
};
export function fragmentedUdpErrorText(value: unknown): string {
  if (value === "connect_udp_capacity_unavailable") return "A ponte não disponibilizou o limite UDP aprovado. A conexão do jogo permanece independente.";
  if (value === "connect_udp_closed") return "A ponte UDP foi encerrada. O estado da conexão do jogo é mostrado separadamente.";
  return "Não foi possível preparar ou manter a ponte UDP. Nenhuma porta alternativa foi selecionada.";
}
