/** A transient native observation; it grants no connection or application service. */
type PathState = "unknown" | "direct" | "relay" | "closed";
export type PathObservation = { schemaVersion: 1; state: PathState }
  | { schemaVersion: 2; state: "unknown" | "closed" }
  | { schemaVersion: 2; state: "direct" | "relay"; rttEstimateMicros?: number };

const invalid = (): never => { throw new Error("Invalid Connect path observation"); };
const validRtt = (value: unknown): value is number => typeof value === "number"
  && Number.isSafeInteger(value) && value > 0 && value <= 4_294_967_295;
export function pathObservationFrom(value: unknown): PathObservation {
  if (!value || typeof value !== "object" || Array.isArray(value)) return invalid();
  const item = value as Record<string, unknown>;
  const hasRtt = Object.hasOwn(item, "rttEstimateMicros");
  if (!Object.hasOwn(item, "schemaVersion") || !Object.hasOwn(item, "state")
    || ![1, 2].includes(item.schemaVersion as number) || !["unknown", "direct", "relay", "closed"].includes(item.state as string)
    || Object.keys(item).length !== (hasRtt ? 3 : 2)
    || ("rttEstimateMicros" in item && !hasRtt)) return invalid();
  if (hasRtt && (item.schemaVersion !== 2 || !["direct", "relay"].includes(item.state as string)
    || !validRtt(item.rttEstimateMicros))) return invalid();
  return item as PathObservation;
}

// Run before any specialized snapshot parser can return early. A group only
// observes its individual participants, and pending/terminal phases omit it.
export function snapshotPathObservationFrom(value: Record<string, unknown>): void {
  if ("pathObservation" in value && !Object.hasOwn(value, "pathObservation")) return invalid();
  if (!Object.hasOwn(value, "pathObservation")) return;
  pathObservationFrom(value.pathObservation);
  if (value.available !== true || typeof value.operationId !== "string" || value.operationId.length === 0
    || value.hostGroup !== undefined
    || !((value.phase === "hostListening" && value.role === "host") || (value.phase === "guestConnected" && value.role === "guest"))) return invalid();
}

export function pathObservationText(value?: PathObservation): string {
  const label = value ? { unknown: "Não confirmado", direct: "Caminho direto observado", relay: "Relay observado", closed: "Conexão encerrada" }[value.state]
    : "Não confirmado";
  if (value?.schemaVersion !== 2 || (value.state !== "direct" && value.state !== "relay") || !validRtt(value.rttEstimateMicros)) return label;
  // QUIC's current estimate is not an application ping or a fresh probe.
  const estimate = value.rttEstimateMicros < 1000 ? "< 1" : Math.round(value.rttEstimateMicros / 1000).toLocaleString("pt-BR");
  return `${label} · RTT estimado do transporte: ${estimate} ms`;
}
