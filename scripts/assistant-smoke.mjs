const targets = await (await fetch("http://127.0.0.1:9222/json")).json();
const target = targets.find((item) => item.title === "Aurora Smart Launcher");
if (!target?.webSocketDebuggerUrl) throw new Error("WebView Aurora não encontrado");

const socket = new WebSocket(target.webSocketDebuggerUrl);
const pending = new Map();
let sequence = 0;
socket.onmessage = (event) => {
  const message = JSON.parse(event.data);
  const operation = pending.get(message.id);
  if (!operation) return;
  pending.delete(message.id);
  if (message.error) operation.reject(new Error(message.error.message));
  else operation.resolve(message.result);
};
await new Promise((resolve, reject) => {
  socket.onopen = resolve;
  socket.onerror = reject;
});

function call(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = ++sequence;
    pending.set(id, { resolve, reject });
    socket.send(JSON.stringify({ id, method, params }));
  });
}

await call("Runtime.enable");
const result = await call("Runtime.evaluate", {
  awaitPromise: true,
  returnByValue: true,
  expression: `
    (async () => {
      const originalFetch = window.fetch.bind(window);
      let capturedAuthorization = "";
      window.fetch = (input, init) => {
        const url = typeof input === "string" ? input : input.url;
        if (url.includes("/v1/assistant")) {
          capturedAuthorization = new Headers(init?.headers).get("Authorization") ?? "";
        }
        return originalFetch(input, init);
      };
      const waitStartedAt = Date.now();
      let assistantButton;
      while (!assistantButton && Date.now() - waitStartedAt < 15000) {
        assistantButton = [...document.querySelectorAll("button")]
          .find((button) => button.textContent?.trim() === "Assistente");
        if (!assistantButton) await new Promise((resolve) => setTimeout(resolve, 250));
      }
      if (!assistantButton) throw new Error("Sessão Aurora autenticada não ficou pronta");
      if (!document.querySelector('[aria-label="Assistente Aurora"]')) assistantButton?.click();
      await new Promise((resolve) => setTimeout(resolve, 300));
      const input = document.querySelector('textarea[aria-label="Mensagem para o Assistente"]');
      if (!input) throw new Error("Campo do Assistente não encontrado");
      const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set;
      setter.call(input, "Responda exatamente com: Aurora online");
      input.dispatchEvent(new Event("input", { bubbles: true }));
      await new Promise((resolve) => setTimeout(resolve, 50));
      input.form.requestSubmit();
      const startedAt = Date.now();
      let panelText = "";
      while (Date.now() - startedAt < 45000) {
        await new Promise((resolve) => setTimeout(resolve, 250));
        const panel = document.querySelector('[aria-label="Assistente Aurora"]');
        const text = panel?.innerText ?? "";
        if (!text.includes("Pensando") && (
          text.includes("Aurora online")
          || text.includes("não está disponível")
          || text.includes("não autorizou")
          || text.includes("limite temporário")
        )) {
          panelText = text;
          break;
        }
      }
      window.fetch = originalFetch;
      if (!panelText) throw new Error("O Assistente não concluiu em 45 segundos");
      if (!panelText.includes("Aurora online") || !capturedAuthorization) {
        return { panelText, inGame: null };
      }
      const inGameResponse = await originalFetch(
        "https://aurora-api.aurora-edge-proxy.workers.dev/v1/assistant",
        {
          method: "POST",
          headers: { Authorization: capturedAuthorization, "Content-Type": "application/json" },
          body: JSON.stringify({ message: "Responda exatamente com: Companion online", mode: "inGame" }),
        },
      );
      const inGame = await inGameResponse.json();
      return { panelText, inGame: { status: inGameResponse.status, ...inGame } };
    })()
  `,
});

socket.close();
if (result.exceptionDetails) {
  throw new Error(result.exceptionDetails.exception?.description ?? "Falha no WebView");
}
const visibleResult = result.result?.value;
if (!visibleResult || typeof visibleResult !== "object") throw new Error("Resultado inválido do WebView");
console.log(JSON.stringify(visibleResult, null, 2));
