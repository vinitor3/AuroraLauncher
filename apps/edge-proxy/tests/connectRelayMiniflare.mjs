// Loopback fixture for real Iroh and relay storage tests.
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { build } from "esbuild";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";

export async function startRelayFixture(approved) {
  const bundle = await build({
    stdin: {
      contents: `
        import { DurableObject } from "cloudflare:workers";
        import { ConnectRelaySession, routeRelayRequest } from "./src/connectRelayWorker.ts";
        export class FixtureRelaySession extends ConnectRelaySession {
          async fixtureRead(key) { return JSON.stringify(await this.ctx.storage.get(key) ?? null); }
          fixtureSeed(key, value) { return this.ctx.storage.put(key, value); }
          fixtureAlarm() { return this.alarm(); }
        }
        export class FixtureSocialSession extends DurableObject {
          fixtureRevoke(peer) { return this.ctx.storage.put("revoked:" + peer, true); }
          async relayAuthorize(ticket) {
            const approved = this.env.APPROVED.find(record =>
              JSON.stringify(record.ticket) === JSON.stringify(ticket));
            const now = Date.now();
            if (!approved || ticket.body.sessionId !== this.ctx.id.name || now >= ticket.body.expiresAtUnixMs
              || await this.ctx.storage.get("revoked:" + ticket.body.peerId))
              throw new Error("relay_fixture_unauthorized");
            return {
              body: ticket.body,
              allowedPeers: approved.allowedPeers,
              freshUntil: Math.min(now + 3000, ticket.body.expiresAtUnixMs),
            };
          }
        }
        export default { fetch: routeRelayRequest };
      `,
      resolveDir: process.cwd(),
      loader: "ts",
    },
    bundle: true,
    write: false,
    format: "esm",
    external: ["cloudflare:workers"],
  });
  const options = convertV4MiniflareOptions({
    host: "127.0.0.1",
    cf: false,
    port: 0,
    // Match the workerd binary bundled with the locked Wrangler dependency.
    compatibilityDate: "2026-09-02",
    modules: true,
    script: bundle.outputFiles[0].text,
    bindings: { APPROVED: approved },
    durableObjects: {
      RELAY_SESSIONS: { className: "FixtureRelaySession", useSQLite: true },
      SOCIAL_SESSIONS: { className: "FixtureSocialSession", useSQLite: true },
    },
  });
  const runtime = new Miniflare({ ...options, logRequests: false });
  try {
    const address = await runtime.ready;
    if (address.hostname !== "127.0.0.1" || !address.port) throw new Error("fixture relay not loopback");
    return { runtime, address };
  } catch (error) {
    await runtime.dispose();
    throw error;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const input = createInterface({ input: process.stdin });
  const lines = input[Symbol.asyncIterator]();
  const config = await lines.next();
  if (config.done) throw new Error("relay fixture configuration missing");
  const { runtime, address } = await startRelayFixture(JSON.parse(config.value));
  try {
    console.log(address.href);
    await lines.next();
  } finally {
    input.close();
    await runtime.dispose();
  }
}
