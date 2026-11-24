import assert from "node:assert/strict";
import { setImmediate } from "node:timers/promises";
import test from "node:test";
import { cacheLocalSkinImage, deleteLocalSkin, isRetiredLocalSkin, listLocalSkins, saveLocalSkin, updateLocalSkinSource } from "../src/lib/localSkins.ts";

// Minimal event-driven IndexedDB stand-in: requests succeed before transaction
// commit, writes roll back on abort, and every open creates a closable connection.
// The browser smoke test separately exercises the real IndexedDB implementation.
function databaseFixture(options = {}) {
  const records = new Map();
  const connections = [];
  const transactions = [];
  const original = globalThis.indexedDB;
  globalThis.indexedDB = {
    open() {
      const request = {};
      const database = {
        closed: false,
        close() { this.closed = true; },
        transaction(_name, mode) {
          if (options.transactionError) throw new Error("cannot start transaction");
          const data = new Map([...records].map(([key, value]) => [key, structuredClone(value)]));
          let pending = 0;
          let finished = false;
          const transaction = {
            error: null,
            commit() {
              if (finished || pending) return;
              finished = true;
              if (mode === "readwrite") {
                records.clear();
                for (const [key, value] of data) records.set(key, value);
              }
              transaction.oncomplete?.();
            },
            abort() {
              if (finished) throw new Error("inactive transaction");
              finished = true;
              queueMicrotask(() => transaction.onabort?.());
            },
            failAtCommit() {
              transaction.error = new Error("disk commit failed");
              this.abort();
            },
            objectStore() {
              const run = (kind, operation) => {
                const result = {};
                pending++;
                queueMicrotask(() => {
                  pending--;
                  if (finished) return;
                  if (options.failRequest === kind) {
                    result.error = new Error(`${kind} failed`);
                    result.onerror?.();
                    transaction.onerror?.();
                    return;
                  }
                  result.result = operation();
                  result.onsuccess?.();
                  if (!pending && !finished && !options.manualCommit) {
                    queueMicrotask(() => transaction.commit());
                  }
                });
                return result;
              };
              return {
                get: (key) => run("get", () => data.get(key)),
                put: (value) => run("put", () => { data.set(value.key, structuredClone(value)); return value.key; }),
                delete: (key) => run("delete", () => { data.delete(key); }),
                index: () => ({ getAll: (owner) => run("getAll", () => [...data.values()].filter((record) => record.ownerId === owner)) }),
              };
            },
          };
          transactions.push(transaction);
          return transaction;
        },
      };
      connections.push(database);
      queueMicrotask(() => { request.result = database; request.onsuccess?.(); });
      return request;
    },
  };
  return { records, connections, transactions, restore: () => { globalThis.indexedDB = original; } };
}

const input = (extra = {}) => ({ ownerId: "player", id: "skin", name: "My skin", skinModel: "slim", ...extra });
const png = () => new Blob([new Uint8Array([137, 80, 78, 71])], { type: "image/png" });
const seed = (fixture, extra = {}) => {
  const record = { ...input(), createdAt: 1234, image: png(), sourceUrl: "https://skins.example/original.png", key: "player:skin", ...extra };
  fixture.records.set(record.key, record);
  return record;
};

test("old Studio selections stay hidden offline and players can import similarly named skins", async () => {
  const fixture = databaseFixture();
  try {
    const path = "/v1/skins/catalog/solar-ninja.png";
    seed(fixture, { sourceUrl: "https://aurora-api.aurora-edge-proxy.workers.dev" + path });
    seed(fixture, { key: "player:imported", id: "imported", sourceUrl: undefined, name: "Ninja Solar" });
    seed(fixture, { key: "player:remote", id: "remote", sourceUrl: "https://skins.example" + path, name: "Aurora Studio" });
    assert.deepEqual((await listLocalSkins("player")).map((skin) => skin.id).sort(), ["imported", "remote"]);
    assert.equal(await isRetiredLocalSkin("player", "skin"), true);
    assert.equal(await isRetiredLocalSkin("player", "imported"), false);
    assert.equal(await isRetiredLocalSkin("other-player", "skin"), false);
  } finally { fixture.restore(); }
});

test("same-id recovery preserves the PNG, custom name and original date", async () => {
  const fixture = databaseFixture();
  try {
    seed(fixture);
    const skin = await saveLocalSkin(input({ name: "Skin equipada", sourceUrl: "blob:expired-renderer" }));
    assert.equal(skin.name, "My skin");
    assert.equal(skin.createdAt, 1234);
    assert.equal(skin.sourceUrl, "https://skins.example/original.png");
    assert.deepEqual(await skin.image.arrayBuffer(), await png().arrayBuffer());
    assert.equal((await listLocalSkins("player")).length, 1);
    assert.ok(fixture.connections.every((connection) => connection.closed));
  } finally { fixture.restore(); }
});

test("recovered bytes can replace an unavailable legacy image without deleting its record", async () => {
  const fixture = databaseFixture();
  try {
    seed(fixture, { image: undefined, sourceUrl: "blob:previous-launch" });
    const legacy = (await listLocalSkins("player"))[0];
    assert.equal(legacy.sourceUrl, undefined);
    assert.equal(fixture.records.size, 1);
    assert.equal(fixture.records.get("player:skin").sourceUrl, "blob:previous-launch");
    const recovered = await saveLocalSkin(input({ image: png(), sourceUrl: "https://skins.example/recovered.png" }));
    assert.equal(recovered.createdAt, 1234);
    assert.equal(recovered.image.size, 4);
    assert.equal(fixture.records.size, 1);
  } finally { fixture.restore(); }
});

test("uploaded URL update retains locally persisted PNG and does not recreate removed records", async () => {
  const fixture = databaseFixture();
  try {
    seed(fixture);
    const updated = await updateLocalSkinSource("player", "skin", "https://skins.example/new.png");
    assert.equal(updated.sourceUrl, "https://skins.example/new.png");
    assert.equal(updated.image.size, 4);
    assert.equal(updated.name, "My skin");
    assert.equal(updated.createdAt, 1234);
    await deleteLocalSkin("player", "skin");
    assert.equal(await updateLocalSkinSource("player", "skin", "https://skins.example/new.png"), undefined);
    assert.equal(fixture.records.size, 0);
  } finally { fixture.restore(); }
});

test("background image repair keeps name, model and date, and cannot resurrect a removed skin", async () => {
  const fixture = databaseFixture();
  try {
    seed(fixture, { image: undefined, skinModel: "classic" });
    const repaired = await cacheLocalSkinImage("player", "skin", png(), "https://skins.example/repaired.png");
    assert.equal(repaired.image.size, 4);
    assert.equal(repaired.sourceUrl, "https://skins.example/repaired.png");
    assert.equal(repaired.name, "My skin");
    assert.equal(repaired.skinModel, "classic");
    assert.equal(repaired.createdAt, 1234);
    const unchangedSource = await cacheLocalSkinImage("player", "skin", png(), "blob:temporary");
    assert.equal(unchangedSource.sourceUrl, "https://skins.example/repaired.png");
    await assert.rejects(cacheLocalSkinImage("player", "skin", new Blob()), /vazia/);
    await deleteLocalSkin("player", "skin");
    assert.equal(await cacheLocalSkinImage("player", "skin", png()), undefined);
    assert.equal(fixture.records.size, 0);
  } finally { fixture.restore(); }
});

test("save never persists renderer-only, insecure or credential-bearing URLs", async () => {
  const fixture = databaseFixture();
  try {
    for (const [index, sourceUrl] of ["blob:session", "data:image/png;base64,abc", "http://example/skin", "https://name:secret@example/skin", "not a URL"].entries()) {
      const skin = await saveLocalSkin(input({ id: `skin-${index}`, image: png(), sourceUrl }));
      assert.equal(skin.sourceUrl, undefined);
      assert.equal(skin.image.size, 4);
      await assert.rejects(updateLocalSkinSource("player", skin.id, sourceUrl), /HTTPS/);
    }
    assert.equal(fixture.records.size, 5);
  } finally { fixture.restore(); }
});

test("accounts are isolated for reads and same skin IDs", async () => {
  const fixture = databaseFixture();
  try {
    seed(fixture);
    await saveLocalSkin(input({ ownerId: "other", name: "Other skin", image: png() }));
    await updateLocalSkinSource("other", "skin", "https://skins.example/other.png");
    assert.equal((await listLocalSkins("player"))[0].sourceUrl, "https://skins.example/original.png");
    assert.equal((await listLocalSkins("other"))[0].name, "Other skin");
    await deleteLocalSkin("other", "skin");
    assert.equal((await listLocalSkins("player")).length, 1);
  } finally { fixture.restore(); }
});

test("save, read and delete promises wait for commit, not request success", async () => {
  const fixture = databaseFixture({ manualCommit: true });
  try {
    for (const action of [() => saveLocalSkin(input({ image: png() })), () => listLocalSkins("player"), () => deleteLocalSkin("player", "skin")]) {
      let settled = false;
      const pending = action().then((value) => { settled = true; return value; });
      await setImmediate();
      assert.equal(settled, false);
      assert.equal(fixture.connections.at(-1).closed, false);
      fixture.transactions.at(-1).commit();
      await pending;
      assert.equal(settled, true);
      assert.equal(fixture.connections.at(-1).closed, true);
    }
    assert.equal(fixture.records.size, 0);
  } finally { fixture.restore(); }
});

test("abort after successful put rejects instead of claiming persistence and preserves previous PNG", async () => {
  const fixture = databaseFixture({ manualCommit: true });
  try {
    seed(fixture);
    const saving = saveLocalSkin(input({ sourceUrl: "https://skins.example/new.png" }));
    const failed = assert.rejects(saving, /disk commit failed/);
    await setImmediate();
    fixture.transactions[0].failAtCommit();
    await failed;
    assert.equal(fixture.records.get("player:skin").sourceUrl, "https://skins.example/original.png");
    assert.equal(fixture.records.get("player:skin").image.size, 4);
    assert.ok(fixture.connections.every((connection) => connection.closed));
  } finally { fixture.restore(); }
});

test("request and transaction creation errors always close the database connection", async () => {
  for (const options of [{ failRequest: "get" }, { failRequest: "put" }, { failRequest: "getAll" }, { failRequest: "delete" }, { transactionError: true }]) {
    const fixture = databaseFixture(options);
    try {
      const action = options.failRequest === "getAll" ? () => listLocalSkins("player")
        : options.failRequest === "delete" ? () => deleteLocalSkin("player", "skin")
          : () => saveLocalSkin(input({ image: png() }));
      await assert.rejects(action());
      assert.ok(fixture.connections.every((connection) => connection.closed));
      assert.equal(fixture.records.size, 0);
    } finally { fixture.restore(); }
  }
});
