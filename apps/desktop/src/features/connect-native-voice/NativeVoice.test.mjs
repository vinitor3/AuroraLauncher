// DTO validation only; no native command, voice audio or configuration mutation.
import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile } from "node:fs/promises";
import ts from "typescript";
const source = await readFile(new URL("./types.ts", import.meta.url), "utf8");
const code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
const { nativeVoiceSnapshotFrom, nativeVoiceError, nativeVoiceUdpLabel, nativeVoiceEulaLabel, voiceRuntimes, voiceReviewDeadline, voiceReviewDetails } = await import(`data:text/javascript;base64,${Buffer.from(code).toString("base64")}`);
const idle = { revision: 1, available: true, phase: "idle", profile: null, review: null, operationId: null, hostPublicId: null, participants: [], udpState: "notStarted", tcpAddress: null, negotiatedDatagramBytes: null, recoveryPending: false, eulaRecoveryAvailable: false, recoverableCopies: [], eulaState: "unchanged", errorCode: null, exportedInvitation: null, audioState: "notVerified" };
test("native voice stays descriptive and only accepts owned loopback game addresses", () => {
  assert.deepEqual(nativeVoiceSnapshotFrom(idle), idle);
  for (const field of [{ audioState: "ready" }, { tcpAddress: "192.168.1.1:25565" }, { tcpAddress: "127.0.0.1:65536" }, { negotiatedDatagramBytes: 4095 }, { profile: "both" }, { phase: "playing" }]) assert.throws(() => nativeVoiceSnapshotFrom({ ...idle, ...field }));
});
test("approval must include the complete bounded native diff and digest", () => {
  const review = { kind: "files", digest: "a".repeat(64), remainingMs: 30_000, before: "voice_host=", after: "voice_host=127.0.0.1:35554", details: ["Recibo durável"] };
  assert.equal(nativeVoiceSnapshotFrom({ ...idle, review }).review.digest, review.digest);
  for (const changed of [{ kind: "launchShell" }, { digest: "arbitrary" }, { remainingMs: undefined }, { remainingMs: -1 }, { remainingMs: 60_001 }, { remainingMs: 0.1 }, { details: Array(21).fill("x") }, { before: "x".repeat(16385) }]) assert.throws(() => nativeVoiceSnapshotFrom({ ...idle, review: { ...review, ...changed } }));
});
test("errors never echo a raw backend path or credential", () => {
  const raw = "C:/private/secret=fixture";
  assert.ok(!nativeVoiceError(raw).includes(raw));
  assert.match(nativeVoiceError("native_voice_local_udp_in_use"), /35554/);
});

test("source revalidation has its own bounded review and typed states", () => {
  const review = { kind: "socialSource", digest: "b".repeat(64), remainingMs: 60_000, before: "Source active", after: "New claim", details: [] };
  for (const phase of ["socialSourceReviewing", "socialSourceReviewed", "socialSourceApplying"]) assert.equal(nativeVoiceSnapshotFrom({ ...idle, phase, review, socialRevalidationAvailable: false }).phase, phase);
  assert.throws(() => nativeVoiceSnapshotFrom({ ...idle, review: { ...review, remainingMs: 60_001 } }));
  assert.throws(() => nativeVoiceSnapshotFrom({ ...idle, socialRevalidationAvailable: "true" }));
  assert.match(nativeVoiceError("native_voice_new_advertisement_required"), /novo no Social/);
  assert.match(nativeVoiceError("native_voice_social_still_active"), /liberação dos recursos/);
});
test("UDP and EULA labels use only known product text with a safe fallback", () => {
  assert.equal(nativeVoiceUdpLabel("notStarted"), "Não iniciado");
  assert.equal(nativeVoiceUdpLabel("retired"), "Encerrado nesta sessão");
  assert.equal(nativeVoiceEulaLabel("unchanged"), "Sem alteração");
  assert.equal(nativeVoiceEulaLabel("restored"), "Original restaurado");
  assert.equal(nativeVoiceEulaLabel("needsRecovery"), "Recuperação necessária");
  for (const unknown of ["toString", "__proto__", "token=secret", "newState"]) {
    assert.equal(nativeVoiceUdpLabel(unknown), "Estado não reconhecido");
    assert.equal(nativeVoiceEulaLabel(unknown), "Estado não reconhecido");
  }
});

test("runtime selection is closed and unavailable voice remains unavailable", () => {
  assert.equal(Object.keys(voiceRuntimes).length, 9);
  for (const runtimeProfile of Object.keys(voiceRuntimes)) {
    assert.equal(nativeVoiceSnapshotFrom({ ...idle, profile: "simpleVoiceChat", runtimeProfile }).runtimeProfile, runtimeProfile);
  }
  assert.throws(() => nativeVoiceSnapshotFrom({ ...idle, runtimeProfile: "custom-loader", profile: "simpleVoiceChat" }));
  assert.throws(() => nativeVoiceSnapshotFrom({ ...idle, runtimeProfile: "1.12.2-forge-14.23.5.2860", profile: "plasmoVoice" }));
  const review = { kind: "preparation", digest: "a".repeat(64), remainingMs: 60_000, before: "Absent", after: "Official Forge installer", details: ["Java 8"] };
  assert.equal(nativeVoiceSnapshotFrom({ ...idle, runtimeProfile: "1.12.2-forge-14.23.5.2860", phase: "preparationReviewed", profile: "simpleVoiceChat", review }).review.kind, "preparation");
});

test("EULA and announcement recovery stay separate in the native projection", () => {
  const copy = { bundleId: "copy", receiptId: "receipt", profile: "simpleVoiceChat", runtimeProfile: "1.12.2-forge-14.23.5.2860", eulaRecoveryPending: true, announcementRecoveryPending: false };
  const review = { kind: "eulaRestore", digest: "a".repeat(64), remainingMs: 40_000, before: "eula=true", after: "eula=false", details: [] };
  assert.equal(nativeVoiceSnapshotFrom({ ...idle, phase: "eulaRestoreReviewed", eulaRecoveryAvailable: true, review, recoverableCopies: [copy] }).review.kind, "eulaRestore");
  assert.equal(nativeVoiceSnapshotFrom({ ...idle, phase: "failed" }).phase, "failed");
  assert.equal(nativeVoiceSnapshotFrom({ ...idle, phase: "restoringEula" }).phase, "restoringEula");
  assert.equal(nativeVoiceSnapshotFrom({ ...idle, review: { ...review, kind: "restore", remainingMs: 300_000 } }).review.remainingMs, 300_000);
  assert.throws(() => nativeVoiceSnapshotFrom({ ...idle, review: { ...review, remainingMs: 300_000 } }));
  assert.throws(() => nativeVoiceSnapshotFrom({ ...idle, review: { ...review, kind: "restore", remainingMs: 300_001 } }));
  for (const changed of [{ runtimeProfile: "unknown" }, { eulaRecoveryPending: false }, { announcementRecoveryPending: undefined }, { profile: "plasmoVoice" }]) {
    assert.throws(() => nativeVoiceSnapshotFrom({ ...idle, recoverableCopies: [{ ...copy, ...changed }] }));
  }
});

test("polling cannot extend the original review deadline", () => {
  const review = { kind: "files", digest: "a".repeat(64), remainingMs: 10_000 };
  const first = voiceReviewDeadline(undefined, review, 1000);
  assert.equal(first.at, 11_000);
  assert.equal(voiceReviewDeadline(first, review, 5000).at, 11_000);
  assert.equal(voiceReviewDeadline(first, { ...review, remainingMs: 2000 }, 5000).at, 7000);
  assert.equal(voiceReviewDeadline(first, { ...review, digest: "b".repeat(64) }, 5000).at, 15_000);
});

test("native JSON recipes remain technical details with only a descriptive copy path", () => {
  const recipe = JSON.stringify({ cwd: "C:/owned-copy", executable: "C:/managed-java", args: ["fixed"] });
  assert.deepEqual(voiceReviewDetails(["Somente cópia própria", recipe]), { summary: ["Somente cópia própria"], technical: [recipe], copies: ["C:/owned-copy"] });
});
