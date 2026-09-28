// Synthetic transport fixtures. No Firebase account or Gemini key is used.
import assert from "node:assert/strict";
import test, { afterEach } from "node:test";
import { requestDoctorAdvice } from "./edge.ts";

const originalFetch = globalThis.fetch;
afterEach(() => { globalThis.fetch = originalFetch; });
const report = {
  schemaVersion: 1, status: "issues", instanceId: "private-player",
  instance: { minecraftVersion: "1.20.1", loader: "Fabric", javaMajor: 17, requiredJavaMajor: 17, maxMemoryMb: 4096 },
  lastRun: { outcome: "failed", runId: "private-run", javaExecutable: "C:/Users/private-player/bin/java.exe" },
  issues: [{ id: "memory_failure", confidence: "high", kind: "fact", explanation: "token=synthetic-secret", evidence: [{ summary: "private raw line" }] }],
  logs: [{ association: "last_run", bytesRead: 42, truncated: false, source: "private-player.log" }],
  sanitizedJson: "Do not send this serialized report",
};
const advice = { schemaVersion: 1, summary: "A JVM registrou falta de memória.", cause: "A memória disponível ou o limite do perfil não atendeu à execução.",
  certainty: "supported", actions: ["check_memory"], evidenceIds: ["memory_failure"] };
function services(getIdToken = async () => "synthetic-session") {
  return { config: { workerUrl: "https://aurora.test/" }, auth: { currentUser: { uid: "synthetic-owner", getIdToken } } };
}

test("Doctor uses the existing session, sends only categories/configuration and validates returned JSON", async () => {
  let calls = 0;
  globalThis.fetch = async (url, init) => {
    calls += 1;
    assert.equal(url, "https://aurora.test/v1/doctor/advice");
    assert.equal(init.headers.Authorization, "Bearer synthetic-session");
    assert.ok(init.signal instanceof AbortSignal);
    const input = JSON.parse(init.body);
    assert.deepEqual(Object.keys(input).sort(), ["schemaVersion", "status", "profile", "outcome", "issues", "coverage"].sort());
    for (const secret of ["private-player", "synthetic-secret", "private raw line", "private-run", "serialized report", "sanitizedJson"]) {
      assert.ok(!init.body.includes(secret));
    }
    assert.deepEqual(input.issues, [{ id: "memory_failure", confidence: "high", kind: "fact" }]);
    return Response.json(advice);
  };
  assert.deepEqual(await requestDoctorAdvice(services(), report, new AbortController().signal), advice);
  assert.equal(calls, 1);
});

test("cancellation during token acquisition returns promptly and never sends a late request", async () => {
  let resolveToken;
  let calls = 0;
  const session = services(() => new Promise((resolve) => { resolveToken = resolve; }));
  globalThis.fetch = async () => { calls += 1; return Response.json(advice); };
  const controller = new AbortController();
  const request = requestDoctorAdvice(session, report, controller.signal);
  controller.abort();
  await assert.rejects(request, { name: "AbortError" });
  resolveToken("synthetic-session");
  await Promise.resolve();
  assert.equal(calls, 0);
});

test("unidentified Minecraft does not send the engine's Java baseline fallback as a verified requirement", async () => {
  globalThis.fetch = async (_url, init) => {
    const input = JSON.parse(init.body);
    assert.equal(input.profile.minecraftVersion, null);
    assert.equal(input.profile.requiredJavaMajor, null);
    assert.equal(input.profile.javaMajor, 17);
    return Response.json(advice);
  };
  for (const minecraftVersion of ["Não identificada", null, undefined]) {
    await requestDoctorAdvice(services(), { ...report, instance: { ...report.instance, minecraftVersion, requiredJavaMajor: 8 } }, new AbortController().signal);
  }
});

test("switching accounts while acquiring a token or response discards the explanation", async () => {
  const session = services(async () => { session.auth.currentUser = { uid: "different-owner" }; return "synthetic-session"; });
  globalThis.fetch = async () => assert.fail("No request after owner changed");
  await assert.rejects(requestDoctorAdvice(session, report, new AbortController().signal), { name: "AbortError" });
  const second = services();
  globalThis.fetch = async () => { second.auth.currentUser = null; return Response.json(advice); };
  await assert.rejects(requestDoctorAdvice(second, report, new AbortController().signal), { name: "AbortError" });
});

test("quota, old server, invalid output and private upstream errors never replace the local report", async () => {
  const original = JSON.stringify(report);
  for (const status of [401, 404, 429, 500]) {
    globalThis.fetch = async () => Response.json({ error: "token=synthetic-secret C:/Users/private-player" }, { status });
    await assert.rejects(requestDoctorAdvice(services(), report, new AbortController().signal), (error) => !/synthetic-secret|private-player/.test(error.message));
  }
  for (const value of [{ ...advice, actions: ["delete_everything"] }, { ...advice, evidenceIds: ["duplicate_mod"] }, { ...advice, summary: "x".repeat(161) }]) {
    globalThis.fetch = async () => Response.json(value);
    await assert.rejects(requestDoctorAdvice(services(), report, new AbortController().signal), { code: "invalid_response" });
  }
  assert.equal(JSON.stringify(report), original);
});

test("cancel after headers also interrupts a stalled response body", async () => {
  let cancelled = false;
  const controller = new AbortController();
  globalThis.fetch = async () => new Response(new ReadableStream({ start() { setTimeout(() => controller.abort(), 10); }, cancel() { cancelled = true; } }),
    { headers: { "Content-Type": "application/json" } });
  await assert.rejects(requestDoctorAdvice(services(), report, controller.signal), { name: "AbortError" });
  assert.equal(cancelled, true);
});

test("large replies, missing session, aborted start and unknown local categories fail closed", async () => {
  let calls = 0;
  globalThis.fetch = async () => { calls += 1; return Response.json({ text: "x".repeat(8193) }); };
  await assert.rejects(requestDoctorAdvice(services(), report, new AbortController().signal));
  assert.equal(calls, 1);
  const session = services(); session.auth.currentUser = null;
  await assert.rejects(requestDoctorAdvice(session, report, new AbortController().signal), { code: "session" });
  await assert.rejects(requestDoctorAdvice(services(), report, AbortSignal.abort()), { name: "AbortError" });
  await assert.rejects(requestDoctorAdvice(services(), { ...report, issues: [{ id: "untrusted", confidence: "high", kind: "fact" }] }, new AbortController().signal));
  assert.equal(calls, 1);
});
