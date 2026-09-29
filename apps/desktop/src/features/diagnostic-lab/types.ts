import { recoveryError } from "../recovery/types";

export type LabEnvironment = {
  id: string;
  instanceUuid: string;
  originUuid: string;
  originInstanceId: string;
  createdAt: number;
  state: "ready" | "creating" | "failed" | "deleting";
  logicalBytes: number;
  copiedWorld: string | null;
  limitations: string[];
};
export type LabSetting = { path: string; pointer: string; label: string; value: number | boolean; kind: "number" | "boolean"; min: number | null; max: number | null };
export type LabDetails = {
  environment: LabEnvironment;
  mods: { path: string; enabled: boolean }[];
  settings: LabSetting[];
  changes: { id: string; path: string; action: string; before: string | null; after: string | null }[];
  baselineFingerprint: string;
};
export type LabProgress = { instanceId: string; phase: string; completedBytes: number; totalBytes: number };

// Only reviewed native messages may reach the UI; raw I/O errors can contain paths or secrets.
export function labError(cause: unknown): string {
  const messages = ["Feche o Minecraft antes de alterar a cópia de teste ou sua origem.", "Feche o Minecraft da instância original antes de revisar ou aplicar alterações.", "Feche o Minecraft antes de excluir esta cópia de teste.", "A operação foi interrompida. Reabra as cópias de teste para verificar o estado.", "A exclusão foi interrompida. Atualize a lista e tente novamente.", "Não foi possível consultar as cópias de teste."];
  return typeof cause === "string" && messages.includes(cause) ? cause : recoveryError(cause);
}
