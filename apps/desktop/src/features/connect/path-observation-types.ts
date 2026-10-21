/** A transient native observation; it grants no connection or application service. */
export type PathObservation = { schemaVersion: 1; state: "unknown" | "direct" | "relay" | "closed" };

const invalid = (): never => { throw new Error("Invalid Connect path observation"); };
export function pathObservationFrom(value: unknown): PathObservation {
  if (!value || typeof value !== "object" || Array.isArray(value)) return invalid();
  const item = value as Record<string, unknown>;
  if (Object.keys(item).length !== 2 || !Object.hasOwn(item, "schemaVersion") || !Object.hasOwn(item, "state")
    || item.schemaVersion !== 1 || !["unknown", "direct", "relay", "closed"].includes(item.state as string)) return invalid();
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
  return value ? { unknown: "Não confirmado", direct: "Caminho direto observado", relay: "Relay observado", closed: "Conexão encerrada" }[value.state]
    : "Não confirmado";
}
