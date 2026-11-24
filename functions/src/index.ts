import { HttpsError, onCall } from "firebase-functions/v2/https";
import { defineSecret } from "firebase-functions/params";
import { logger } from "firebase-functions";

const curseForgeApiKey = defineSecret("CURSEFORGE_API_KEY");
const curseForgeApiBase = "https://api.curseforge.com/v1";
const minecraftGameId = 432;
const modpackClassId = 4471;

type CatalogRequest =
  | { action: "search"; query: string; pageSize?: number; index?: number }
  | { action: "mod"; modId: number }
  | { action: "file"; modId: number; fileId: number };

function positiveInteger(value: unknown, name: string, maximum = Number.MAX_SAFE_INTEGER): number {
  if (!Number.isInteger(value) || typeof value !== "number" || value <= 0 || value > maximum) {
    throw new HttpsError("invalid-argument", name + " inválido.");
  }
  return value;
}

function boundedText(value: unknown, name: string, maximum: number): string {
  if (typeof value !== "string" || !value.trim() || value.length > maximum) {
    throw new HttpsError("invalid-argument", name + " inválido.");
  }
  return value.trim();
}

async function curseForgeGet(path: string, query?: URLSearchParams): Promise<unknown> {
  const suffix = query?.size ? "?" + query.toString() : "";
  const response = await fetch(curseForgeApiBase + path + suffix, {
    headers: {
      Accept: "application/json",
      "x-api-key": curseForgeApiKey.value(),
    },
  });
  if (!response.ok) {
    logger.warn("CurseForge recusou a consulta", { status: response.status, path });
    throw new HttpsError(
      response.status === 404 ? "not-found" : "unavailable",
      "O catálogo CurseForge não está disponível neste momento.",
    );
  }
  return response.json();
}

export const curseforgeCatalog = onCall(
  { cors: false, secrets: [curseForgeApiKey] },
  async (request) => {
    if (!request.auth) {
      throw new HttpsError("unauthenticated", "Entre na sua conta Aurora para usar o CurseForge.");
    }
    const payload = request.data as Partial<CatalogRequest>;
    if (!payload || typeof payload.action !== "string") {
      throw new HttpsError("invalid-argument", "Pedido de catálogo inválido.");
    }

    switch (payload.action) {
      case "search": {
        const query = new URLSearchParams({
          gameId: String(minecraftGameId),
          classId: String(modpackClassId),
          searchFilter: boundedText(payload.query, "Busca", 120),
          pageSize: String(payload.pageSize ?? 20),
          index: String(payload.index ?? 0),
        });
        const pageSize = Number(query.get("pageSize"));
        const index = Number(query.get("index"));
        if (!Number.isInteger(pageSize) || pageSize < 1 || pageSize > 50 || !Number.isInteger(index) || index < 0) {
          throw new HttpsError("invalid-argument", "Paginação inválida.");
        }
        return curseForgeGet("/mods/search", query);
      }
      case "mod":
        return curseForgeGet("/mods/" + positiveInteger(payload.modId, "Modpack"));
      case "file":
        return curseForgeGet(
          "/mods/" + positiveInteger(payload.modId, "Modpack") + "/files/" + positiveInteger(payload.fileId, "Arquivo"),
        );
      default:
        throw new HttpsError("invalid-argument", "Ação de catálogo não permitida.");
    }
  },
);
