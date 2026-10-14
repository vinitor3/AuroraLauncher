import type { VoiceMod } from "./types";

/** Presentation only; the native prepared object owns the record and deadline. */
export type RetirementReview = {
  planId: string; receiptId: string; voiceMod: VoiceMod; expectedGeneration: 0;
  currentConfigSha256: string | null; currentConfigBytes: number | null;
  digest: string; expiresAtMs: number; warning: string;
};
const invalid = (): never => { throw new Error("Invalid retirement projection"); };
export function retirementReviewFrom(value: unknown): RetirementReview {
  if (!value || typeof value !== "object" || Array.isArray(value)) return invalid();
  const item = value as Record<string, unknown>;
  const fields = ["planId", "receiptId", "voiceMod", "expectedGeneration", "currentConfigSha256", "currentConfigBytes", "digest", "expiresAtMs", "warning"];
  if (Object.keys(item).length !== fields.length || fields.some(key => !Object.hasOwn(item, key))) invalid();
  const uuid = (candidate: unknown) => typeof candidate === "string" && /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(candidate);
  const hash = (candidate: unknown) => typeof candidate === "string" && /^[a-f0-9]{64}$/.test(candidate);
  if (!uuid(item.planId) || !uuid(item.receiptId) || !["simple_voice_chat", "plasmo_voice"].includes(item.voiceMod as string)
    || item.expectedGeneration !== 0 || !hash(item.digest) || !Number.isSafeInteger(item.expiresAtMs) || (item.expiresAtMs as number) < 0
    || typeof item.warning !== "string" || new TextEncoder().encode(item.warning).length > 1024 || /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(item.warning)) invalid();
  if (item.currentConfigSha256 === null ? item.currentConfigBytes !== null
    : !hash(item.currentConfigSha256) || !Number.isSafeInteger(item.currentConfigBytes) || (item.currentConfigBytes as number) < 0 || (item.currentConfigBytes as number) > 262144) invalid();
  return item as RetirementReview;
}
