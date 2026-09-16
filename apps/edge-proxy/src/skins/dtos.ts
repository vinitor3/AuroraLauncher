/** Untrusted wire DTOs: runtime validation lives in the client adapters. */
export interface McApiSearchDto { players?: unknown; query?: unknown; total?: unknown }
export interface McApiProfileDto {
  uuid?: unknown;
  username?: unknown;
  skin_url?: unknown;
  is_slim?: unknown;
  renders?: { skin_png?: unknown; head?: unknown; body?: unknown };
}
export interface McApiHistoryDto { skins?: unknown }
export function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("invalid-response");
  return value as Record<string, unknown>;
}
