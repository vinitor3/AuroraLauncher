import assert from "node:assert/strict";
import { test } from "node:test";
import { connectOnlineEnvironment } from "./connect-online-build-options.mjs";

test("ordinary online builds reset inherited private opt-ins and pin both trusted origins", () => {
  const inherited = { AURORA_SOCIAL_PRIVATE_ONLINE: "1", VITE_AURORA_SOCIAL_PRIVATE_ONLINE: "1",
    AURORA_SOCIAL_ORIGIN: "https://other.invalid", VITE_AURORA_SOCIAL_ORIGIN: "https://other.invalid", OTHER: "preserved" };
  const value = connectOnlineEnvironment([], inherited);
  assert.equal(value.AURORA_SOCIAL_PRIVATE_ONLINE, "0"); assert.equal(value.VITE_AURORA_SOCIAL_PRIVATE_ONLINE, "0");
  assert.equal(value.AURORA_SOCIAL_ORIGIN, "https://aurora-connect-social.aurora-edge-proxy.workers.dev");
  assert.equal(value.VITE_AURORA_SOCIAL_ORIGIN, value.AURORA_SOCIAL_ORIGIN); assert.equal(value.OTHER, "preserved");
  assert.equal(inherited.AURORA_SOCIAL_PRIVATE_ONLINE, "1");
});
test("explicit private build pairs both gates and rejects unknown or duplicate options", () => {
  const value = connectOnlineEnvironment(["--private-invitations"]);
  assert.equal(value.AURORA_SOCIAL_PRIVATE_ONLINE, "1"); assert.equal(value.VITE_AURORA_SOCIAL_PRIVATE_ONLINE, "1");
  for (const args of [["--private-invitations=1"], ["--private-invitations", "--private-invitations"], ["--release"]]) {
    assert.throws(() => connectOnlineEnvironment(args), /desconhecida/);
  }
});
