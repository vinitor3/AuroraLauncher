import { buildDoctorAdviceInput, DOCTOR_ISSUE_IDS, parseDoctorAdviceInput, type DoctorAdviceInput } from "./doctorAdvice.ts";

export type AiToolProposal = { kind: "remove_mod"; targetId: string; reason: string }
  | { kind: "set_setting"; targetId: string; value: boolean | number; reason: string };
export type AiToolHistoryEntry = {
  id: string; kind: "diagnosis" | "advice" | "action"; createdAtMs: number; title: string; summary: string;
  issueIds: string[]; actionId: string | null; status: "recorded" | "applied" | "undone" | "pending";
  assessment: "unresolved" | "normal_exit" | "inconclusive" | null; undoAvailable: boolean;
};
export type AiToolContext = {
  schemaVersion: 1; contextId: string;
  mods: Array<{ id: string; name: string; version: string | null; enabled: boolean; managed: boolean }>;
  settings: Array<{ id: string; label: string; value: boolean | number; kind: "boolean" | "number"; min: number | null; max: number | null }>;
  truncated: boolean; history: AiToolHistoryEntry[];
};
export type AiToolPlan = { planId: string; digest: string; title: string; changes: Array<{ label: string; before: string; after: string }>;
  warnings: string[]; expiresAtMs: number };
export type AiToolAdvice = { summary: string; proposals: AiToolProposal[]; model: string };
type ModelHistory = Pick<AiToolHistoryEntry, "kind" | "status" | "assessment" | "issueIds" | "title" | "summary">;
export type AiToolRequest = { schemaVersion: 1; question: string;
  context: Pick<AiToolContext, "mods" | "settings" | "truncated"> & { history: ModelHistory[] }; doctor: DoctorAdviceInput | null };
const object = (v: unknown): v is Record<string, unknown> => !!v && typeof v === "object" && !Array.isArray(v);
const exact = (v: Record<string, unknown>, keys: string[]) => Object.keys(v).length === keys.length && keys.every((key) => Object.hasOwn(v, key));
const member = (v: unknown, values: readonly string[]): v is string => typeof v === "string" && values.includes(v);
const id = (v: unknown): v is string => typeof v === "string" && /^[a-zA-Z0-9_-]{1,96}$/.test(v);
const scalar = (v: unknown): v is number | boolean => typeof v === "boolean" || (typeof v === "number" && Number.isFinite(v) && Math.abs(v) <= 1_000_000);
const privateText = /[\u0000-\u001f\u007f<>`]|https?:\/\/|www\.|[A-Za-z]:[\\/]|\\\\|\/(?:Users|home)\/|[\w.+-]+@[\w.-]+\.[a-z]{2,}|(?:token|password|senha|api.?key)\s*[:=]/i;
const short = (v: unknown, max: number): v is string => typeof v === "string" && v.trim().length > 0 && v.length <= max && !privateText.test(v);
const clean = (v: unknown, max: number, fallback: string): string => short(v, max) ? v.trim() : fallback;

/** New explicit projection. No context/run/account IDs, files or arbitrary report text. */
export function buildAiToolRequest(context: AiToolContext, question: string, report?: unknown): AiToolRequest {
  const request = parseAiToolRequest({ schemaVersion: 1, question: question.trim(), doctor: report ? buildDoctorAdviceInput(report) : null,
    context: {
      mods: context.mods.map((mod) => ({ id: mod.id, name: clean(mod.name, 120, "Mod"), version: short(mod.version, 64) ? mod.version : null,
        enabled: mod.enabled, managed: mod.managed })),
      settings: context.settings.map((setting) => ({ id: setting.id, label: clean(setting.label, 160, "Configuração"), value: setting.value,
        kind: setting.kind, min: setting.min, max: setting.max })),
      truncated: context.truncated,
      // Context history is generated canonically by the native broker. Local
      // history with raw prior AI wording is not a valid model context source.
      history: context.history.slice(0, 12).map((entry) => ({ kind: entry.kind, status: entry.status, assessment: entry.assessment,
        issueIds: entry.issueIds.filter((value) => (DOCTOR_ISSUE_IDS as readonly string[]).includes(value)),
        title: clean(entry.title, 160, "Registro anterior"), summary: clean(entry.summary, 320, "Confira o histórico local da instância.") })),
    },
  });
  if (!request) throw new Error("Contexto inválido. Atualize os dados da instância.");
  return request;
}

export function parseAiToolRequest(value: unknown): AiToolRequest | null {
  if (!object(value) || !exact(value, ["schemaVersion", "question", "context", "doctor"]) || value.schemaVersion !== 1
    || typeof value.question !== "string" || !value.question.trim() || value.question.length > 2_000
    || (value.doctor !== null && !parseDoctorAdviceInput(value.doctor))) return null;
  const context = value.context;
  if (!object(context) || !exact(context, ["mods", "settings", "truncated", "history"]) || typeof context.truncated !== "boolean"
    || !Array.isArray(context.mods) || context.mods.length > 512 || !Array.isArray(context.settings) || context.settings.length > 128
    || !Array.isArray(context.history) || context.history.length > 12) return null;
  const seen = new Set<string>();
  for (const mod of context.mods) {
    if (!object(mod) || !exact(mod, ["id", "name", "version", "enabled", "managed"]) || !id(mod.id) || seen.has(mod.id)
      || !short(mod.name, 120) || (mod.version !== null && !short(mod.version, 64)) || typeof mod.enabled !== "boolean" || typeof mod.managed !== "boolean") return null;
    seen.add(mod.id);
  }
  for (const setting of context.settings) {
    if (!object(setting) || !exact(setting, ["id", "label", "value", "kind", "min", "max"]) || !id(setting.id) || seen.has(setting.id)
      || !short(setting.label, 160) || !scalar(setting.value) || !member(setting.kind, ["boolean", "number"])
      || typeof setting.value !== setting.kind || (setting.min !== null && (typeof setting.min !== "number" || !scalar(setting.min)))
      || (setting.max !== null && (typeof setting.max !== "number" || !scalar(setting.max)))) return null;
    if (typeof setting.value === "number" && ((typeof setting.min === "number" && setting.value < setting.min)
      || (typeof setting.max === "number" && setting.value > setting.max))) return null;
    seen.add(setting.id);
  }
  for (const entry of context.history) {
    if (!object(entry) || !exact(entry, ["kind", "status", "assessment", "issueIds", "title", "summary"])
      || !member(entry.kind, ["diagnosis", "advice", "action"]) || !member(entry.status, ["recorded", "applied", "undone", "pending"])
      || (entry.assessment !== null && !member(entry.assessment, ["unresolved", "normal_exit", "inconclusive"]))
      || !short(entry.title, 160) || !short(entry.summary, 320) || !Array.isArray(entry.issueIds) || entry.issueIds.length > DOCTOR_ISSUE_IDS.length
      || entry.issueIds.some((v) => !member(v, DOCTOR_ISSUE_IDS))) return null;
  }
  return value as AiToolRequest;
}

export function parseAiToolAdvice(value: unknown, context: AiToolRequest["context"]): AiToolAdvice | null {
  if (!object(value) || !exact(value, ["summary", "proposals", "model"]) || !short(value.summary, 400)
    || typeof value.model !== "string" || !/^gemini-[a-z0-9.-]{1,80}$/.test(value.model)
    || !Array.isArray(value.proposals) || value.proposals.length > 3) return null;
  const seen = new Set<string>();
  for (const proposal of value.proposals) {
    if (!object(proposal) || !short(proposal.reason, 240) || !id(proposal.targetId) || seen.has(proposal.targetId)) return null;
    if (proposal.kind === "remove_mod") {
      if (!exact(proposal, ["kind", "targetId", "reason"]) || !context.mods.some((mod) => mod.id === proposal.targetId && !mod.managed)) return null;
    } else if (proposal.kind === "set_setting") {
      const setting = context.settings.find((item) => item.id === proposal.targetId);
      if (!exact(proposal, ["kind", "targetId", "value", "reason"]) || !setting || !scalar(proposal.value)
        || typeof proposal.value !== setting.kind || proposal.value === setting.value) return null;
      if (typeof proposal.value === "number" && ((setting.min !== null && proposal.value < setting.min)
        || (setting.max !== null && proposal.value > setting.max))) return null;
    } else return null;
    seen.add(proposal.targetId);
  }
  return value as AiToolAdvice;
}
