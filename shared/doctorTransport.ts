/** Doctor-only transport bounds, including stalled token/body promises. */
export function withDoctorAbort<T>(promise: Promise<T>, signal: AbortSignal): Promise<T> {
  if (signal.aborted) {
    void promise.catch(() => undefined);
    return Promise.reject(new DOMException("Operação cancelada.", "AbortError"));
  }
  return new Promise<T>((resolve, reject) => {
    const abort = () => reject(new DOMException("Operação cancelada.", "AbortError"));
    signal.addEventListener("abort", abort, { once: true });
    promise.then(resolve, reject).finally(() => signal.removeEventListener("abort", abort));
  });
}

export async function readDoctorJson(input: Request | Response, maximumBytes: number, signal: AbortSignal): Promise<unknown> {
  const advertised = Number(input.headers.get("Content-Length"));
  if (advertised > maximumBytes || signal.aborted) {
    void input.body?.cancel().catch(() => undefined);
    throw new Error("Resposta indisponível.");
  }
  const reader = input.body?.getReader();
  if (!reader) throw new Error("Resposta indisponível.");
  const chunks: Uint8Array[] = [];
  let length = 0;
  try {
    while (true) {
      const next = await withDoctorAbort(reader.read(), signal);
      if (next.done) break;
      length += next.value.length;
      if (length > maximumBytes) throw new Error("Resposta indisponível.");
      chunks.push(next.value);
    }
  } catch (cause) {
    void reader.cancel().catch(() => undefined);
    throw cause;
  } finally {
    reader.releaseLock();
  }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
  return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
}
