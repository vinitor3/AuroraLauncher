import { readDoctorJson } from "../../../shared/doctorTransport.ts";
import { aiFailureResponse, generateGemini, GEMINI_THINKING, GeminiFailure, type GeminiEnvironment } from "./gemini.ts";

export async function handleAssistant(request: Request, env: GeminiEnvironment, signal: AbortSignal): Promise<Response> {
  try {
    let body: Record<string, unknown>;
    try { body = await readDoctorJson(request, 12_288, signal) as Record<string, unknown>; }
    catch { throw new GeminiFailure(signal.aborted ? "timeout" : "invalid_request", signal.aborted ? 504 : 400); }
    if (!body || typeof body !== "object" || Array.isArray(body) || Object.keys(body).some((key) => !["message", "mode"].includes(key))
      || (body.mode !== undefined && body.mode !== "launcher")) return Response.json({ error: "Use o Assistente do Aurora Launcher.", code: "invalid_request" }, { status: 400, headers: { "Access-Control-Allow-Origin": "*" } });
    if (typeof body.message !== "string" || !body.message.trim() || body.message.length > 2_000) throw new GeminiFailure("invalid_request", 400);
    const result = await generateGemini(env, {
      systemInstruction: { parts: [{ text: "Você é o Assistente do Aurora Launcher. Ajude com Minecraft, mods, desempenho e erros de instâncias. Responda em português, em poucas frases úteis. Nunca diga que executou uma ação: este chat só responde. Ações exigem seleção explícita da instância, proposta e confirmação em popup. Não peça senhas, tokens ou arquivos privados." }] },
      contents: [{ role: "user", parts: [{ text: body.message.trim() }] }],
      generationConfig: { maxOutputTokens: 2_048, thinkingConfig: GEMINI_THINKING },
    }, signal);
    return Response.json(result, { headers: { "Access-Control-Allow-Origin": "*", "Cache-Control": "no-store" } });
  } catch (cause) { return aiFailureResponse(cause); }
}
