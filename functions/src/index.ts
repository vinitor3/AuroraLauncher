import { HttpsError, onCall } from "firebase-functions/v2/https";
import { defineSecret } from "firebase-functions/params";
import { logger } from "firebase-functions";

const curseForgeApiKey = defineSecret("CURSEFORGE_API_KEY");
const geminiApiKey = defineSecret("GEMINI_API_KEY");
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

/**
 * Conversa curta do overlay. A chave Gemini nunca sai desta função e só perfis
 * Aurora autenticados podem chamar o endpoint.
 */
export const auroraAssistant = onCall(
  { cors: false, secrets: [geminiApiKey], timeoutSeconds: 30 },
  async (request) => {
    if (!request.auth) {
      throw new HttpsError("unauthenticated", "Entre na sua conta Aurora para usar o Assistente.");
    }
    const message = boundedText(request.data?.message, "Mensagem", 2_000);
    const response = await fetch(
      "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.7-flash:generateContent",
      {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "x-goog-api-key": geminiApiKey.value(),
        },
        body: JSON.stringify({
          systemInstruction: {
            parts: [{
              text: "Você é o Assistente Aurora dentro do Minecraft. Responda em português, de forma curta, segura e útil. Não peça senhas, tokens ou dados privados.",
            }],
          },
          contents: [{ role: "user", parts: [{ text: message }] }],
          generationConfig: { maxOutputTokens: 400 },
        }),
      },
    );
    if (!response.ok) {
      logger.warn("Gemini recusou a consulta", { status: response.status });
      throw new HttpsError("unavailable", "O Assistente Aurora não está disponível agora.");
    }
    const payload = await response.json() as {
      candidates?: Array<{ content?: { parts?: Array<{ text?: string }> } }>;
    };
    const text = payload.candidates?.[0]?.content?.parts
      ?.map((part) => part.text ?? "")
      .join("")
      .trim();
    if (!text) throw new HttpsError("unavailable", "O Assistente não retornou uma resposta.");
    return { text: text.slice(0, 8_000) };
  },
);
