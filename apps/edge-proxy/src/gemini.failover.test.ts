// Independent failover fixtures. All fetches are synthetic; no credentials/network.
import assert from "node:assert/strict";
import test, { afterEach, beforeEach, type TestContext } from "node:test";
import { aiFailureResponse, generateGemini, GEMINI_CURRENT_MODEL, GEMINI_LITE_MODEL, GEMINI_MODEL } from "./gemini.ts";
import { handleProjectTranslation } from "./translation.ts";

const originalFetch = globalThis.fetch;
const realSetTimeout = globalThis.setTimeout.bind(globalThis);
const realSignalTimeout = AbortSignal.timeout.bind(AbortSignal);
let warnings: unknown[][] = [];
beforeEach((t) => {
  warnings = [];
  t.mock.method(console, "warn", (...args: unknown[]) => { warnings.push(args); });
});
afterEach(() => { globalThis.fetch = originalFetch; });

const keys = {
  GEMINI_API_KEY: "synthetic-primary", GEMINI_API_KEY_2: "synthetic-secondary", GEMINI_API_KEY_3: "synthetic-tertiary",
};
const reply = (text = "Resposta sintética.", model = GEMINI_CURRENT_MODEL) => Response.json({ modelVersion: model,
  candidates: [{ finishReason: "STOP", content: { parts: [{ text }] } }] });
const failure = (status: number, apiStatus = "UNAVAILABLE") => Response.json({ error: {
  status: apiStatus, message: "synthetic-provider-private-detail",
} }, { status });
const callDetails = (url: string | URL | Request, init?: RequestInit) => ({
  model: new URL(String(url)).pathname.split("/").at(-1)?.split(":")[0],
  key: new Headers(init?.headers).get("x-goog-api-key"), signal: init?.signal,
});

/** Keep the runtime's requested budgets observable without sleeping for seconds. */
function fastBackoff(t: TestContext) {
  const delays: number[] = [];
  t.mock.method(globalThis, "setTimeout", (callback: (...args: unknown[]) => void, milliseconds?: number, ...args: unknown[]) => {
    const delay = Number(milliseconds ?? 0);
    delays.push(delay);
    return realSetTimeout(callback, delay >= 500 && delay < 5_000 ? 1 : delay, ...args);
  });
  return delays;
}

test("secondary and tertiary keys independently configure the transport, trimming blanks", async () => {
  for (const [env, expected] of [
    [{ GEMINI_API_KEY: "   ", GEMINI_API_KEY_2: " synthetic-secondary " }, "synthetic-secondary"],
    [{ GEMINI_API_KEY_3: " synthetic-tertiary " }, "synthetic-tertiary"],
  ] as const) {
    let calls = 0;
    globalThis.fetch = async (url, init) => {
      calls++;
      assert.equal(callDetails(url, init).key, expected);
      assert.equal(callDetails(url, init).model, GEMINI_MODEL);
      assert.equal(new URL(String(url)).search, "", "keys never belong in the URL");
      return reply();
    };
    await generateGemini(env, {}, new AbortController().signal);
    assert.equal(calls, 1);
  }
  globalThis.fetch = async () => assert.fail("empty configuration must not fetch");
  await assert.rejects(generateGemini({ GEMINI_API_KEY: " ", GEMINI_API_KEY_2: "", GEMINI_API_KEY_3: "\t" }, {}, new AbortController().signal), { code: "not_configured" });
});

test("duplicate trimmed keys do not receive repeated attempts before a distinct key", async (t) => {
  fastBackoff(t);
  const calls: ReturnType<typeof callDetails>[] = [];
  globalThis.fetch = async (url, init) => {
    calls.push(callDetails(url, init));
    return calls.length === 1 ? failure(503) : reply();
  };
  await generateGemini({ GEMINI_API_KEY: " synthetic-primary ", GEMINI_API_KEY_2: "synthetic-primary", GEMINI_API_KEY_3: " synthetic-tertiary " }, {}, new AbortController().signal);
  assert.deepEqual(calls.map((call) => call.key), [keys.GEMINI_API_KEY, keys.GEMINI_API_KEY_3]);
  assert.deepEqual(calls.map((call) => call.model), [GEMINI_MODEL, GEMINI_MODEL]);
});

test("configured slot order selects the requested primary and excludes omitted credentials", async (t) => {
  fastBackoff(t);
  const calls: ReturnType<typeof callDetails>[] = [];
  globalThis.fetch = async (url, init) => {
    calls.push(callDetails(url, init));
    return calls.length === 1 ? failure(503) : reply();
  };
  await generateGemini({ ...keys, GEMINI_KEY_ORDER: "2,1" }, {}, new AbortController().signal);
  assert.deepEqual(calls.map((call) => call.key), [keys.GEMINI_API_KEY_2, keys.GEMINI_API_KEY]);
  calls.length = 0;
  globalThis.fetch = async (url, init) => { calls.push(callDetails(url, init)); return failure(503); };
  await assert.rejects(generateGemini({ ...keys, GEMINI_KEY_ORDER: "2,1" }, {}, new AbortController().signal), { code: "provider_unavailable" });
  assert.deepEqual(calls.map((call) => call.key), [keys.GEMINI_API_KEY_2, keys.GEMINI_API_KEY]);
  assert.ok(!calls.some((call) => call.key === keys.GEMINI_API_KEY_3));
});

test("invalid, empty and duplicate slot orders fail closed instead of consulting a default key", async () => {
  globalThis.fetch = async () => assert.fail("invalid slot selection must not send a request");
  for (const order of ["", " ", "1,1", "2,1,2", "2, 1", "1,2,", ",1", "4", "0", "01", "1\n", "1;2"]) {
    await assert.rejects(generateGemini({ ...keys, GEMINI_KEY_ORDER: order }, {}, new AbortController().signal), { code: "not_configured" });
  }
  await assert.rejects(generateGemini({ GEMINI_API_KEY: keys.GEMINI_API_KEY, GEMINI_KEY_ORDER: "3" }, {}, new AbortController().signal), { code: "not_configured" });
});

test("key deduplication respects configured order and never restores an excluded slot", async (t) => {
  fastBackoff(t);
  const calls: ReturnType<typeof callDetails>[] = [];
  globalThis.fetch = async (url, init) => {
    calls.push(callDetails(url, init));
    return calls.length === 1 ? failure(503) : reply();
  };
  await generateGemini({ GEMINI_API_KEY: " same-synthetic ", GEMINI_API_KEY_2: "same-synthetic", GEMINI_API_KEY_3: keys.GEMINI_API_KEY_3,
    GEMINI_KEY_ORDER: "2,1,3" }, {}, new AbortController().signal);
  assert.deepEqual(calls.map((call) => call.key), ["same-synthetic", keys.GEMINI_API_KEY_3]);
});

test("three configured slots fail over in order with bounded increasing backoff", async (t) => {
  const delays = fastBackoff(t);
  const calls: ReturnType<typeof callDetails>[] = [];
  globalThis.fetch = async (url, init) => {
    calls.push(callDetails(url, init));
    return calls.length < 3 ? failure(503) : reply("Terceira credencial respondeu.");
  };
  const result = await generateGemini(keys, {}, new AbortController().signal);
  assert.equal(result.text, "Terceira credencial respondeu.");
  assert.deepEqual(calls.map((call) => call.key), Object.values(keys));
  assert.deepEqual(calls.map((call) => call.model), [GEMINI_MODEL, GEMINI_MODEL, GEMINI_MODEL]);
  const backoffs = delays.filter((delay) => delay >= 500 && delay < 5_000);
  assert.equal(backoffs.length, 2);
  assert.ok(backoffs[0] >= 1_000 && backoffs[0] <= 1_250);
  assert.ok(backoffs[1] >= 2_000 && backoffs[1] <= 2_250);
});

test("network exceptions, HTTP408 and server failures may use the next configured key", async (t) => {
  fastBackoff(t);
  for (const first of ["network", 408, 500, 502, 504] as const) {
    const calls: ReturnType<typeof callDetails>[] = [];
    globalThis.fetch = async (url, init) => {
      calls.push(callDetails(url, init));
      if (calls.length === 1) {
        if (first === "network") throw new TypeError("synthetic-network-private-detail");
        return failure(first);
      }
      return reply();
    };
    await generateGemini(keys, {}, new AbortController().signal);
    assert.deepEqual(calls.map((call) => call.key), [keys.GEMINI_API_KEY, keys.GEMINI_API_KEY_2]);
  }
});

test("quota status or RESOURCE_EXHAUSTED stops every configured key immediately", async (t) => {
  fastBackoff(t);
  for (const [httpStatus, apiStatus] of [[429, "RESOURCE_EXHAUSTED"], [503, "RESOURCE_EXHAUSTED"], [400, "RESOURCE_EXHAUSTED"]] as const) {
    let calls = 0;
    globalThis.fetch = async () => { calls++; return failure(httpStatus, apiStatus); };
    await assert.rejects(generateGemini(keys, {}, new AbortController().signal), { code: "quota" });
    assert.equal(calls, 1, "quota must not trigger another account or model");
  }
  let calls = 0;
  globalThis.fetch = async () => { calls++; return calls === 1 ? failure(503) : failure(429, "RESOURCE_EXHAUSTED"); };
  await assert.rejects(generateGemini(keys, {}, new AbortController().signal), { code: "quota" });
  assert.equal(calls, 2, "quota on slot2 must not try slot3");
});

test("authentication and permanent bad requests never rotate credentials or expose provider details", async () => {
  for (const [status, apiStatus, code] of [
    [401, "UNAUTHENTICATED", "provider_unauthorized"], [403, "PERMISSION_DENIED", "provider_unauthorized"],
    [400, "INVALID_ARGUMENT", "invalid_request"],
  ] as const) {
    let calls = 0;
    globalThis.fetch = async () => { calls++; return failure(status, apiStatus); };
    try {
      await generateGemini(keys, {}, new AbortController().signal);
      assert.fail("the request should fail");
    } catch (error) {
      assert.equal((error as { code: string }).code, code);
      const publicBody = await aiFailureResponse(error).text();
      assert.doesNotMatch(publicBody, /synthetic|x-goog|Bearer/);
    }
    assert.equal(calls, 1);
  }
  assert.doesNotMatch(JSON.stringify(warnings), /synthetic|x-goog|Bearer|private-detail/);
  for (const [, metadata] of warnings) {
    assert.deepEqual(Object.keys(metadata as object).sort(), ["attempt", "code", "elapsedMs", "model", "slot", "status"]);
  }
});

test("missing latest alias tries explicit current model using the same credential", async (t) => {
  fastBackoff(t);
  const calls: ReturnType<typeof callDetails>[] = [];
  globalThis.fetch = async (url, init) => {
    calls.push(callDetails(url, init));
    return calls.length === 1 ? failure(404, "NOT_FOUND") : reply();
  };
  await generateGemini(keys, {}, new AbortController().signal);
  assert.deepEqual(calls.map((call) => call.key), [keys.GEMINI_API_KEY, keys.GEMINI_API_KEY]);
  assert.deepEqual(calls.map((call) => call.model), [GEMINI_MODEL, GEMINI_CURRENT_MODEL]);
});

test("one unique key retains exactly one explicit-model fallback on service failure", async (t) => {
  fastBackoff(t);
  const calls: ReturnType<typeof callDetails>[] = [];
  globalThis.fetch = async (url, init) => { calls.push(callDetails(url, init)); return failure(503); };
  await assert.rejects(generateGemini({ GEMINI_API_KEY_2: keys.GEMINI_API_KEY_2 }, {}, new AbortController().signal), { code: "provider_unavailable" });
  assert.deepEqual(calls.map((call) => call.model), [GEMINI_MODEL, GEMINI_CURRENT_MODEL]);
  assert.equal(calls.length, 2);
});

test("default multi-key budget never exceeds four calls even when every provider fails", async (t) => {
  fastBackoff(t);
  for (const env of [keys, { GEMINI_API_KEY: keys.GEMINI_API_KEY, GEMINI_API_KEY_2: keys.GEMINI_API_KEY_2 }]) {
    let calls = 0;
    globalThis.fetch = async () => { calls++; return failure(503); };
    await assert.rejects(generateGemini(env, {}, new AbortController().signal), { code: "provider_unavailable" });
    assert.ok(calls <= Math.min(4, Object.keys(env).length + 1));
    assert.ok(calls >= Object.keys(env).length);
  }
});

test("shared request budget is consumed by attempts across calls and prevents a fresh retry loop", async (t) => {
  fastBackoff(t);
  const budget = { remaining: 2 };
  let calls = 0;
  globalThis.fetch = async () => { calls++; return reply(); };
  await generateGemini(keys, {}, new AbortController().signal, 32_768, { budget });
  assert.equal(budget.remaining, 1);
  await generateGemini(keys, {}, new AbortController().signal, 32_768, { budget, model: GEMINI_CURRENT_MODEL });
  assert.equal(budget.remaining, 0);
  await assert.rejects(generateGemini(keys, {}, new AbortController().signal, 32_768, { budget }));
  assert.equal(calls, 2);
  const oneAttempt = { remaining: 1 };
  globalThis.fetch = async () => { calls++; return failure(503); };
  await assert.rejects(generateGemini(keys, {}, new AbortController().signal, 32_768, { budget: oneAttempt }));
  assert.equal(calls, 3);
  assert.equal(oneAttempt.remaining, 0);
});

test("long text remains rejected by default but translation may set its own larger bound", async () => {
  const text = "á".repeat(20_000);
  const calls: ReturnType<typeof callDetails>[] = [];
  globalThis.fetch = async (url, init) => { calls.push(callDetails(url, init)); return reply(text); };
  await assert.rejects(generateGemini(keys, {}, new AbortController().signal, 256_000), { code: "invalid_response" });
  assert.equal(calls.length, 1, "invalid content must not rotate accounts");
  const result = await generateGemini(keys, {}, new AbortController().signal, 256_000, {
    maximumTextLength: 100_000, model: GEMINI_CURRENT_MODEL,
  });
  assert.equal(result.text, text);
  assert.equal(calls[1].model, GEMINI_CURRENT_MODEL);
});

test("HTTP200 malformed UTF8, missing bodies, oversize and invalid envelopes are content errors without failover", async () => {
  const cases = [
    () => new Response(null, { headers: { "Content-Type": "application/json" } }),
    () => new Response(new Uint8Array([0xff]), { headers: { "Content-Type": "application/json" } }),
    () => new Response("{}", { headers: { "Content-Type": "application/json", "Content-Length": "32769" } }),
    () => new Response(" ".repeat(32_769), { headers: { "Content-Type": "application/json" } }),
    () => Response.json({ candidates: [{ finishReason: "STOP", content: { parts: [{ text: 123 }] } }] }),
    () => Response.json({ candidates: [{ finishReason: "MAX_TOKENS", content: { parts: [{ text: "partial" }] } }] }),
    () => Response.json({ candidates: [] }),
    () => new Response("{", { headers: { "Content-Type": "application/json" } }),
  ];
  for (const response of cases) {
    let calls = 0;
    globalThis.fetch = async () => { calls++; return response(); };
    await assert.rejects(generateGemini(keys, {}, new AbortController().signal), { code: "invalid_response" });
    assert.equal(calls, 1);
  }
});

test("parent cancellation during backoff prevents the next key and terminates promptly", async (t) => {
  const parent = new AbortController();
  let calls = 0;
  t.mock.method(globalThis, "setTimeout", (callback: (...args: unknown[]) => void, milliseconds?: number, ...args: unknown[]) => {
    if (Number(milliseconds) >= 500 && Number(milliseconds) < 5_000) parent.abort();
    return realSetTimeout(callback, 1, ...args);
  });
  globalThis.fetch = async () => { calls++; return failure(503); };
  await assert.rejects(generateGemini(keys, {}, parent.signal), { code: "timeout" });
  assert.equal(calls, 1);
  const stopped = new AbortController();
  stopped.abort();
  await assert.rejects(generateGemini(keys, {}, stopped.signal), { code: "timeout" });
  assert.equal(calls, 1);
});

test("an attempt deadline cancels a stalled body then permits the next key inside the parent deadline", async (t) => {
  fastBackoff(t);
  const requestedDeadlines: number[] = [];
  t.mock.method(AbortSignal, "timeout", (milliseconds: number) => {
    requestedDeadlines.push(milliseconds);
    return realSignalTimeout(Math.min(milliseconds, 20));
  });
  let cancelled = false;
  const calls: ReturnType<typeof callDetails>[] = [];
  globalThis.fetch = async (url, init) => {
    calls.push(callDetails(url, init));
    return calls.length === 1
      ? new Response(new ReadableStream({ cancel() { cancelled = true; } }), { headers: { "Content-Type": "application/json" } })
      : reply();
  };
  const keepAlive = realSetTimeout(() => undefined, 1_000);
  try { assert.equal((await generateGemini(keys, {}, new AbortController().signal)).text, "Resposta sintética."); }
  finally { clearTimeout(keepAlive); }
  assert.equal(calls.length, 2);
  assert.equal(cancelled, true);
  assert.equal(calls[0].signal?.aborted, true);
  assert.ok(requestedDeadlines.includes(34_000));
});

test("single-key fetch timeout is bounded at thirty-four seconds even when fetch ignores cancellation", async (t) => {
  fastBackoff(t);
  const requestedDeadlines: number[] = [];
  t.mock.method(AbortSignal, "timeout", (milliseconds: number) => {
    requestedDeadlines.push(milliseconds);
    return realSignalTimeout(Math.min(milliseconds, 20));
  });
  const calls: ReturnType<typeof callDetails>[] = [];
  globalThis.fetch = async (url, init) => {
    calls.push(callDetails(url, init));
    if (calls.length === 1) return await new Promise<Response>(() => undefined);
    return reply();
  };
  const keepAlive = realSetTimeout(() => undefined, 1_000);
  try { await generateGemini({ GEMINI_API_KEY_3: keys.GEMINI_API_KEY_3 }, {}, new AbortController().signal); }
  finally { clearTimeout(keepAlive); }
  assert.equal(calls.length, 2);
  assert.equal(calls[0].signal?.aborted, true);
  assert.ok(requestedDeadlines.includes(34_000));
  assert.deepEqual(calls.map((call) => call.model), [GEMINI_MODEL, GEMINI_CURRENT_MODEL]);
});

test("known quota and auth statuses cancel a stalled body immediately without waiting for an error-body deadline", async (t) => {
  fastBackoff(t);
  const deadlines: number[] = [];
  t.mock.method(AbortSignal, "timeout", (milliseconds: number) => { deadlines.push(milliseconds); return realSignalTimeout(Math.min(milliseconds, 20)); });
  for (const status of [429, 402, 401, 403]) {
    deadlines.length = 0;
    let cancelled = false;
    let calls = 0;
    globalThis.fetch = async () => {
      calls++;
      return new Response(new ReadableStream({ cancel() { cancelled = true; } }), {
        status, headers: { "Content-Type": "application/json" },
      });
    };
    const keepAlive = realSetTimeout(() => undefined, 1_000);
    try { await assert.rejects(generateGemini(keys, {}, new AbortController().signal), { code: status === 429 || status === 402 ? "quota" : "provider_unauthorized" }); }
    finally { clearTimeout(keepAlive); }
    assert.equal(cancelled, true);
    assert.equal(calls, 1);
    assert.ok(!deadlines.includes(1_000), "a known terminal status must not wait for its body");
  }
});

test("a real-scale 33.5-second response succeeds before the 34-second attempt and 35-second parent deadlines", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const deadlines: number[] = [];
  t.mock.method(AbortSignal, "timeout", (milliseconds: number) => {
    deadlines.push(milliseconds);
    const controller = new AbortController();
    setTimeout(() => controller.abort(new DOMException("Synthetic deadline", "TimeoutError")), milliseconds);
    return controller.signal;
  });
  const calls: ReturnType<typeof callDetails>[] = [];
  globalThis.fetch = async (url, init) => {
    calls.push(callDetails(url, init));
    return await new Promise<Response>((resolve) => setTimeout(() => resolve(reply("Resposta aos 33,5 segundos.")), 33_500));
  };
  const parent = AbortSignal.timeout(35_000);
  const pending = generateGemini({ ...keys, GEMINI_KEY_ORDER: "2,1" }, {}, parent);
  t.mock.timers.tick(33_499);
  assert.equal(parent.aborted, false);
  assert.equal(calls[0].signal?.aborted, false);
  t.mock.timers.tick(1);
  assert.equal((await pending).text, "Resposta aos 33,5 segundos.");
  assert.equal(calls.length, 1);
  assert.equal(calls[0].key, keys.GEMINI_API_KEY_2);
  assert.equal(calls[0].signal?.aborted, false);
  assert.ok(deadlines.includes(34_000));
});

test("an unknown server-error body gets only a one-second read budget before safe transient failover", async (t) => {
  fastBackoff(t);
  const deadlines: number[] = [];
  t.mock.method(AbortSignal, "timeout", (milliseconds: number) => { deadlines.push(milliseconds); return realSignalTimeout(Math.min(milliseconds, 20)); });
  let cancelled = false;
  let calls = 0;
  globalThis.fetch = async () => {
    calls++;
    return calls === 1 ? new Response(new ReadableStream({ cancel() { cancelled = true; } }), {
      status: 503, headers: { "Content-Type": "application/json" },
    }) : reply();
  };
  const keepAlive = realSetTimeout(() => undefined, 1_000);
  try { await generateGemini(keys, {}, new AbortController().signal); }
  finally { clearTimeout(keepAlive); }
  assert.equal(calls, 2);
  assert.equal(cancelled, true);
  assert.ok(deadlines.includes(1_000));
});

test("the shared 35-second parent deadline prevents a late retry after a stalled 34-second attempt", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  t.mock.method(AbortSignal, "timeout", (milliseconds: number) => {
    const controller = new AbortController();
    setTimeout(() => controller.abort(new DOMException("Synthetic deadline", "TimeoutError")), milliseconds);
    return controller.signal;
  });
  t.mock.method(Math, "random", () => 0);
  let calls = 0;
  globalThis.fetch = async () => { calls++; return await new Promise<Response>(() => undefined); };
  const parent = AbortSignal.timeout(35_000);
  const pending = generateGemini({ ...keys, GEMINI_KEY_ORDER: "2,1" }, {}, parent);
  const rejected = assert.rejects(pending, { code: "timeout" });
  t.mock.timers.tick(34_000);
  for (let index = 0; index < 6; index++) await Promise.resolve();
  assert.equal(parent.aborted, false);
  assert.equal(calls, 1);
  t.mock.timers.tick(1_000);
  await rejected;
  assert.equal(parent.aborted, true);
  assert.equal(calls, 1, "the total deadline must not restart when advancing a key");
});

const liteEnabled = { ...keys, GEMINI_KEY_ORDER: "2,1", GEMINI_FALLBACK_MODEL: GEMINI_LITE_MODEL };

test("Lite requires explicit configuration and a successful primary skips the reserve", async (t) => {
  fastBackoff(t);
  const calls: ReturnType<typeof callDetails>[] = [];
  globalThis.fetch = async (url, init) => { calls.push(callDetails(url, init)); return failure(503); };
  await assert.rejects(generateGemini(keys, {}, new AbortController().signal), { code: "provider_unavailable" });
  assert.ok(calls.every((call) => !call.model?.includes("lite")));
  calls.length = 0;
  globalThis.fetch = async (url, init) => { calls.push(callDetails(url, init)); return reply(); };
  await generateGemini(liteEnabled, {}, new AbortController().signal);
  assert.equal(calls.length, 1);
  assert.equal(calls[0].model, GEMINI_MODEL);
  for (const fallback of ["gemini-3.5-flash-lite", "gemini-3.8-flash", "arbitrary-model", " gemini-flash-lite-latest "]) {
    await assert.rejects(generateGemini({ ...keys, GEMINI_FALLBACK_MODEL: fallback }, {}, new AbortController().signal), { code: "not_configured" });
  }
  assert.equal(calls.length, 1);
});

test("two transient Flash failures permit one Lite answer using the selected key order", async (t) => {
  fastBackoff(t);
  const calls: ReturnType<typeof callDetails>[] = [];
  const budget = { remaining: 4 };
  globalThis.fetch = async (url, init) => {
    calls.push(callDetails(url, init));
    return calls.length <= 2 ? failure(503) : reply("Reserva respondeu.", "gemini-3.5-flash-lite");
  };
  const result = await generateGemini(liteEnabled, {}, new AbortController().signal, 32_768, { budget });
  assert.equal(result.model, "gemini-3.5-flash-lite");
  assert.deepEqual(calls.map((call) => call.model), [GEMINI_MODEL, GEMINI_MODEL, GEMINI_LITE_MODEL]);
  assert.deepEqual(calls.map((call) => call.key), [keys.GEMINI_API_KEY_2, keys.GEMINI_API_KEY, keys.GEMINI_API_KEY_2]);
  assert.equal(budget.remaining, 1);
});

test("quota, authorization, bad input and invalid content remain terminal with Lite enabled", async (t) => {
  fastBackoff(t);
  for (const [response, code] of [
    [() => failure(429, "RESOURCE_EXHAUSTED"), "quota"],
    [() => failure(402), "quota"],
    [() => failure(503, "RESOURCE_EXHAUSTED"), "quota"],
    [() => failure(401, "UNAUTHENTICATED"), "provider_unauthorized"],
    [() => failure(403, "PERMISSION_DENIED"), "provider_unauthorized"],
    [() => failure(400, "INVALID_ARGUMENT"), "invalid_request"],
    [() => new Response("{", { headers: { "Content-Type": "application/json" } }), "invalid_response"],
    [() => Response.json({ candidates: [] }), "invalid_response"],
  ] as const) {
    const calls: ReturnType<typeof callDetails>[] = [];
    const budget = { remaining: 4 };
    globalThis.fetch = async (url, init) => { calls.push(callDetails(url, init)); return response(); };
    await assert.rejects(generateGemini(liteEnabled, {}, new AbortController().signal, 32_768, { budget }), { code });
    assert.equal(calls.length, 1);
    assert.equal(calls[0].model, GEMINI_MODEL);
    assert.equal(budget.remaining, 3);
  }
});

test("missing Flash and Lite aliases may reach the explicit Lite model but never exceed four provider calls", async () => {
  const calls: ReturnType<typeof callDetails>[] = [];
  const budget = { remaining: 4 };
  globalThis.fetch = async (url, init) => {
    calls.push(callDetails(url, init));
    return calls.length < 4 ? failure(404, "NOT_FOUND") : reply("Versão explícita disponível.", "gemini-3.5-flash-lite");
  };
  await generateGemini(liteEnabled, {}, new AbortController().signal, 32_768, { budget });
  assert.deepEqual(calls.map((call) => call.model), [GEMINI_MODEL, GEMINI_CURRENT_MODEL, GEMINI_LITE_MODEL, "gemini-3.5-flash-lite"]);
  assert.equal(budget.remaining, 0);
  await assert.rejects(generateGemini(liteEnabled, {}, new AbortController().signal, 32_768, { budget }));
  assert.equal(calls.length, 4);
});

test("all transient Flash and Lite failures share one four-call cap", async (t) => {
  fastBackoff(t);
  const calls: ReturnType<typeof callDetails>[] = [];
  const budget = { remaining: 4 };
  globalThis.fetch = async (url, init) => { calls.push(callDetails(url, init)); return failure(503); };
  await assert.rejects(generateGemini(liteEnabled, {}, new AbortController().signal, 32_768, { budget }), { code: "provider_unavailable" });
  assert.equal(calls.length, 4);
  assert.deepEqual(calls.map((call) => call.model), [GEMINI_MODEL, GEMINI_MODEL, GEMINI_LITE_MODEL, GEMINI_LITE_MODEL]);
  assert.equal(budget.remaining, 0);
});

function virtualDeadlines(t: TestContext) {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  t.mock.method(AbortSignal, "timeout", (milliseconds: number) => {
    const controller = new AbortController();
    setTimeout(() => controller.abort(new DOMException("Synthetic deadline", "TimeoutError")), milliseconds);
    return controller.signal;
  });
}

async function flushMicrotasks() {
  for (let index = 0; index < 20; index++) await Promise.resolve();
}

test("an expired 20-second Flash phase permits Lite to finish within the original 35-second deadline", async (t) => {
  virtualDeadlines(t);
  const calls: ReturnType<typeof callDetails>[] = [];
  globalThis.fetch = async (url, init) => {
    calls.push(callDetails(url, init));
    if (calls.length === 1) return await new Promise<Response>(() => undefined);
    return await new Promise<Response>((resolve) => setTimeout(() => resolve(reply("Reserva aos 34 segundos totais.", "gemini-3.5-flash-lite")), 14_000));
  };
  const parent = AbortSignal.timeout(35_000);
  const pending = generateGemini(liteEnabled, {}, parent);
  t.mock.timers.tick(20_000);
  await flushMicrotasks();
  assert.equal(calls.length, 2);
  assert.equal(calls[0].signal?.aborted, true);
  assert.equal(calls[1].model, GEMINI_LITE_MODEL);
  assert.equal(parent.aborted, false);
  t.mock.timers.tick(14_000);
  assert.equal((await pending).model, "gemini-3.5-flash-lite");
  assert.equal(parent.aborted, false);
  assert.equal(calls.length, 2);
});

test("a shared parent deadline still stops a slow Lite reserve and forbids further attempts", async (t) => {
  virtualDeadlines(t);
  const calls: ReturnType<typeof callDetails>[] = [];
  globalThis.fetch = async (url, init) => { calls.push(callDetails(url, init)); return await new Promise<Response>(() => undefined); };
  const parent = AbortSignal.timeout(35_000);
  const pending = generateGemini(liteEnabled, {}, parent);
  const rejected = assert.rejects(pending, { code: "timeout" });
  t.mock.timers.tick(20_000);
  await flushMicrotasks();
  assert.equal(calls.length, 2);
  assert.equal(calls[1].model, GEMINI_LITE_MODEL);
  t.mock.timers.tick(15_000);
  await rejected;
  assert.equal(parent.aborted, true);
  assert.equal(calls[1].signal?.aborted, true);
  assert.equal(calls.length, 2);
});

test("translation semantic retries and Lite reserve consume the same four-call budget", async (t) => {
  fastBackoff(t);
  const calls: ReturnType<typeof callDetails>[] = [];
  globalThis.fetch = async (url, init) => {
    calls.push(callDetails(url, init));
    if (calls.length === 1) return reply('{"translations":[]}');
    if (calls.length <= 3) return failure(503);
    return reply('{"translations":[{"id":0,"text":"Texto traduzido."}]}', "gemini-3.5-flash-lite");
  };
  const request = new Request("https://synthetic.invalid/v1/translate", { method: "POST", headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ targetLanguage: "pt-BR", segments: ["Source text."] }) });
  const response = await handleProjectTranslation(request, liteEnabled);
  assert.equal(response.status, 200);
  assert.deepEqual((await response.json()).translations, [{ id: 0, text: "Texto traduzido." }]);
  assert.deepEqual(calls.map((call) => call.model), [GEMINI_MODEL, GEMINI_CURRENT_MODEL, GEMINI_CURRENT_MODEL, GEMINI_LITE_MODEL]);
  assert.equal(calls.length, 4);
});

test("public model overrides cannot bypass the configured Flash-first Lite reserve policy", async () => {
  globalThis.fetch = async () => assert.fail("an invalid public model override must not fetch");
  for (const env of [keys, liteEnabled]) {
    for (const model of [GEMINI_LITE_MODEL, "gemini-3.5-flash-lite", "arbitrary-model"]) {
      await assert.rejects(generateGemini(env, {}, new AbortController().signal, 32_768, { model }), { code: "model_unavailable" });
    }
  }
});

test("the Lite reserve never creates an extra attempt when the shared budget has only one remaining", async (t) => {
  fastBackoff(t);
  const calls: ReturnType<typeof callDetails>[] = [];
  globalThis.fetch = async (url, init) => { calls.push(callDetails(url, init)); return failure(503); };
  const exhausted = { remaining: 1 };
  await assert.rejects(generateGemini(liteEnabled, {}, new AbortController().signal, 32_768, { budget: exhausted }), { code: "provider_unavailable" });
  assert.equal(exhausted.remaining, 0);
  assert.equal(calls.length, 1);
  assert.equal(calls[0].model, GEMINI_MODEL);
  calls.length = 0;
  const twoAttempts = { remaining: 2 };
  globalThis.fetch = async (url, init) => {
    calls.push(callDetails(url, init));
    return calls.length === 1 ? failure(503) : reply("Reserva no orçamento restante.", "gemini-3.5-flash-lite");
  };
  await generateGemini(liteEnabled, {}, new AbortController().signal, 32_768, { budget: twoAttempts });
  assert.deepEqual(calls.map((call) => call.model), [GEMINI_MODEL, GEMINI_LITE_MODEL]);
  assert.equal(twoAttempts.remaining, 0);
});
