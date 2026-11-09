// DTO validation only; no native command, voice audio or configuration mutation.
import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile } from "node:fs/promises";
import ts from "typescript";
const source = await readFile(new URL("./types.ts", import.meta.url), "utf8");
const code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
const { nativeVoiceSnapshotFrom, nativeVoiceError } = await import(`data:text/javascript;base64,${Buffer.from(code).toString("base64")}`);
const idle = { revision: 1, available: true, phase: "idle", profile: null, review: null, operationId: null, hostPublicId: null, participants: [], udpState: "notStarted", tcpAddress: null, negotiatedDatagramBytes: null, recoveryPending: false, recoverableCopies: [], eulaState: "unchanged", errorCode: null, exportedInvitation: null, audioState: "notVerified" };
test("native voice stays descriptive and only accepts owned loopback game addresses", () => {
  assert.deepEqual(nativeVoiceSnapshotFrom(idle), idle);
  for (const field of [{ audioState: "ready" }, { tcpAddress: "192.168.1.1:25565" }, { tcpAddress: "127.0.0.1:65536" }, { negotiatedDatagramBytes: 4095 }, { profile: "both" }, { phase: "playing" }]) assert.throws(() => nativeVoiceSnapshotFrom({ ...idle, ...field }));
});
test("approval must include the complete bounded native diff and digest", () => {
  const review = { kind: "files", digest: "a".repeat(64), before: "voice_host=", after: "voice_host=127.0.0.1:35554", details: ["Recibo durável"] };
  assert.equal(nativeVoiceSnapshotFrom({ ...idle, review }).review.digest, review.digest);
  for (const changed of [{ kind: "launchShell" }, { digest: "arbitrary" }, { details: Array(21).fill("x") }, { before: "x".repeat(16385) }]) assert.throws(() => nativeVoiceSnapshotFrom({ ...idle, review: { ...review, ...changed } }));
});
test("errors never echo a raw backend path or credential", () => {
  const raw = "C:/private/secret=fixture";
  assert.ok(!nativeVoiceError(raw).includes(raw));
  assert.match(nativeVoiceError("native_voice_local_udp_in_use"), /35554/);
});
