import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { setImmediate } from "node:timers/promises";
import test from "node:test";
import vm from "node:vm";
import ts from "typescript";

// Execute the actual App functions with controlled storage/IPC boundaries. AST
// extraction avoids duplicating their implementation or asserting source text.
const filename = new URL("../src/App.tsx", import.meta.url);
const source = ts.createSourceFile("App.tsx", readFileSync(filename, "utf8"), ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
const launcher = source.statements.find((node) => ts.isFunctionDeclaration(node) && node.name?.text === "LauncherApp");

function loadFunctions(names, globals) {
  const functions = names.map((name) => {
    const declaration = launcher.body.statements.find((node) => ts.isFunctionDeclaration(node) && node.name?.text === name);
    assert.ok(declaration, `App still exports its ${name} behavior through a named handler`);
    return declaration.getText(source);
  });
  const script = ts.transpileModule(functions.join("\n"), {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None },
  }).outputText;
  const context = vm.createContext(globals);
  vm.runInContext(script, context);
  return context;
}

function deferred() {
  let resolve;
  const promise = new Promise((yes) => { resolve = yes; });
  return { promise, resolve };
}

function librarySetup(overrides = {}) {
  const updates = [], revoked = [];
  let nextUrl = 0;
  const context = loadFunctions(["isCurrentSkinOwner", "refreshLocalSkinLibrary", "showEquippedSkin", "rememberEquippedSkin"], {
    services: { auth: { currentUser: { uid: "current" } } },
    skinOwnerRef: { current: "current" },
    libraryLoadGeneration: { current: 0 },
    selectedSkinIdRef: { current: "selected" },
    URL: { createObjectURL: () => `blob:current-${++nextUrl}`, revokeObjectURL: (url) => revoked.push(url) },
    listLocalSkins: async () => [],
    setLocalSkins: (value) => updates.push(["library", value]),
    setSelectedLocalSkinId: (value) => updates.push(["selected", value]),
    setSkinUrl: (value) => updates.push(["preview", value]),
    setSkinModel: (value) => updates.push(["model", value]),
    setFavoriteName: (value) => updates.push(["name", value]),
    setWardrobeView: (value) => updates.push(["view", value]),
    setLocalSkinPath: (value) => updates.push(["path", value]),
    setEquippedSkin: (value) => updates.push(["equipped", value]),
    window: { localStorage: { setItem: (key, value) => updates.push(["stored", key, value]) } },
    ...overrides,
  });
  return { context, updates, revoked };
}

test("a stale account refresh cannot invalidate or read the active account's library", async () => {
  const run = librarySetup({ listLocalSkins: () => assert.fail("stale account must not read storage") });
  await run.context.refreshLocalSkinLibrary("old-account");
  assert.equal(run.context.libraryLoadGeneration.current, 0);
  assert.equal(run.updates.length, 0);
});

test("latest library refresh wins and only stale replacement URLs are revoked", async () => {
  const first = deferred(), second = deferred();
  let calls = 0;
  const run = librarySetup({ listLocalSkins: () => (++calls === 1 ? first : second).promise });
  const oldRefresh = run.context.refreshLocalSkinLibrary("current");
  const newRefresh = run.context.refreshLocalSkinLibrary("current");
  second.resolve([{ id: "selected", image: new Blob(["new PNG"]) }]);
  await newRefresh;
  first.resolve([{ id: "selected", image: new Blob(["old PNG"]) }]);
  await oldRefresh;
  assert.equal(run.updates.filter(([kind]) => kind === "library").length, 1);
  assert.equal(run.updates.find(([kind]) => kind === "preview")[1], "blob:current-1");
  assert.deepEqual(run.revoked, ["blob:current-2"]);
});

test("logout during an IndexedDB read cannot publish stale skin URLs or selection", async () => {
  const waiting = deferred();
  const run = librarySetup({ listLocalSkins: () => waiting.promise });
  const loading = run.context.refreshLocalSkinLibrary("current");
  run.context.services.auth.currentUser = null;
  run.context.skinOwnerRef.current = "";
  waiting.resolve([{ id: "selected", image: new Blob(["PNG"]) }]);
  await loading;
  assert.equal(run.updates.length, 0);
  assert.deepEqual(run.revoked, ["blob:current-1"]);
});

test("Equipada displays actual equipped PNG/model without equipping or persisting the preview selection", () => {
  const equipped = { id: "in-game", name: "My equipped skin", skinModel: "slim", previewUrl: "data:image/png;base64,cG5n" };
  const run = librarySetup({ equippedSkin: equipped });
  run.context.showEquippedSkin();
  assert.deepEqual(run.updates, [
    ["view", "equipped"], ["selected", equipped.id], ["preview", equipped.previewUrl],
    ["model", "slim"], ["name", equipped.name],
  ]);
});

test("equipped metadata stores identity/model instead of a renderer URL and rejects other accounts", () => {
  const run = librarySetup();
  const skin = { id: "png-id", name: "Skin", image: new Blob(["PNG"]), skinModel: "classic" };
  run.context.rememberEquippedSkin("old-account", skin, "slim", "blob:temporary", "C:/skin.png");
  assert.equal(run.updates.length, 0);
  run.context.rememberEquippedSkin("current", skin, "slim", "data:image/png;base64,cG5n", "C:/skin.png");
  const record = run.updates.find(([kind, key]) => kind === "stored" && key === "aurora.equippedSkin.current");
  assert.deepEqual(JSON.parse(record[2]), { id: "png-id", skinModel: "slim" });
  assert.equal(run.updates.find(([kind]) => kind === "equipped")[1].skinModel, "slim");
});

function launchSetup(overrides = {}) {
  const invocations = [], notices = [], busy = [];
  const doctor = { failure: undefined };
  const context = loadFunctions(["isCurrentSkinOwner", "launchMinecraft"], {
    profile: { uid: "current", username: "Player", skinModel: "classic", skinUrl: "https://example/remote.png" },
    services: { auth: { currentUser: { uid: "current" } } },
    skinOwnerRef: { current: "current" },
    wardrobeLoading: false,
    wardrobeReadyOwner: "current",
    targetInstance: "instance",
    capeWardrobe: { ready: true, share: false, active: { path: "C:/appearance/current/cape.png", url: null } },
    runningInstances: [],
    localSkinPath: "C:/appearance/current/skin.png",
    equippedSkin: { skinModel: "slim" },
    javaPath: "C:/java.exe",
    fabricLoaderNeedsUpgrade: () => false,
    javaMajor: () => 17,
    setNotice: (value) => notices.push(value),
    setBusy: (value) => busy.push(value),
    setDoctorFailure: (value) => { doctor.failure = typeof value === "function" ? value(doctor.failure) : value; },
    setTargetInstance() {}, setInstalledVersionId() {}, setInstalledMinecraftVersion() {},
    setMinecraftVersion() {}, setRunningInstances() {},
    invoke: async (command, args) => {
      invocations.push([command, args]);
      if (command === "begin_instance_launch") return { instanceId: args.id, runId: "test-launch-run", outcome: "preparing" };
      if (command === "cancel_instance_launch" || command === "fail_instance_launch") return;
      if (command === "read_instance_launch_profile") return { versionId: "fabric", minecraftVersion: "1.20.1" };
      if (command === "required_java_for_instance") return 17;
      if (command === "verify_java") return "17";
      if (command === "launch_instance") return { processId: 12, versionId: "fabric" };
      assert.fail(`Unexpected IPC: ${command}`);
    },
    ...overrides,
  });
  return { context, invocations, notices, busy, doctor };
}

test("launch waits for initial wardrobe restoration including the first pre-effect render", async () => {
  for (const state of [{ wardrobeLoading: true }, { wardrobeReadyOwner: "" }, { wardrobeReadyOwner: "previous" }, { profile: undefined }]) {
    const run = launchSetup(state);
    await run.context.launchMinecraft();
    assert.equal(run.invocations.length, 0);
    assert.equal(run.busy.length, 0);
    assert.match(run.notices[0], /Aguarde/);
  }
});

test("ready launch supplies restored local PNG and equipped model rather than stale remote model", async () => {
  const run = launchSetup();
  await run.context.launchMinecraft();
  const launch = run.invocations.find(([command]) => command === "launch_instance")[1];
  assert.equal(launch.skinFile, "C:/appearance/current/skin.png");
  assert.equal(launch.capeFile, "C:/appearance/current/cape.png");
  assert.equal(launch.shareCape, false);
  assert.equal(launch.skinModel, "slim");
  assert.equal(launch.runId, "test-launch-run");
  assert.equal(run.doctor.failure, undefined);
  assert.ok(!run.invocations.some(([command]) => command === "cancel_instance_launch" || command === "fail_instance_launch"));
  assert.deepEqual(run.busy, [true, false]);
});

test("logout while preparing Java prevents the native Minecraft launch", async () => {
  const run = launchSetup();
  const original = run.context.invoke;
  run.context.invoke = async (command, args) => {
    const result = await original(command, args);
    if (command === "verify_java") run.context.services.auth.currentUser = null;
    return result;
  };
  await run.context.launchMinecraft();
  assert.ok(!run.invocations.some(([command]) => command === "launch_instance"));
  const cancellation = run.invocations.find(([command]) => command === "cancel_instance_launch")?.[1];
  assert.equal(cancellation?.id, "instance");
  assert.equal(cancellation?.runId, "test-launch-run");
  assert.ok(!run.invocations.some(([command]) => command === "fail_instance_launch"));
  assert.equal(run.doctor.failure, undefined);
  assert.deepEqual(run.busy, [true, false]);
});

function equipSetup(migration) {
  const actions = [], busy = [];
  const context = loadFunctions(["isCurrentSkinOwner", "saveAppearance"], {
    profile: { uid: "current", skinModel: "classic", skinUrl: "https://example/previous.png" },
    services: { auth: { currentUser: { uid: "current" } } },
    skinOwnerRef: { current: "current" },
    wardrobeLoading: false,
    selectedLocalSkinId: "selected",
    localSkins: [{ id: "selected", image: new Blob(["PNG"]), skinModel: "classic" }],
    skinModel: "slim",
    capeUrl: "",
    appearanceMigrationPending: { current: migration },
    File,
    cacheSkin: async (skin) => skin,
    saveLocalSkin: async (skin) => {
      assert.equal(skin.id, "selected");
      assert.equal(skin.skinModel, "slim");
      return skin;
    },
    validateAppearanceFile: async () => undefined,
    fileAsDataUrl: async () => "data:image/png;base64,cG5n",
    invoke: async (command) => {
      assert.equal(command, "save_local_appearance");
      actions.push(["native PNG"]);
      return "C:/appearance/current/skin.png";
    },
    rememberEquippedSkin: (_owner, _skin, model) => actions.push(["equipped locally", model]),
    refreshLocalSkinLibrary: async () => undefined,
    uploadAppearanceImage: async () => { actions.push(["upload"]); return "https://example/new.png"; },
    verifyPublicCatalogSkin: async () => undefined,
    updateLocalSkinSource: async () => undefined,
    saveAuroraAppearance: async (_services, _profile, appearance) => {
      actions.push(["save remote", appearance]);
      return { uid: "current", ...appearance };
    },
    setProfile: (value) => actions.push(["profile", value]),
    setNotice: (value) => actions.push(["notice", value]),
    setBusy: (value) => busy.push(value),
  });
  return { context, actions, busy };
}

test("manual equip persists PNG immediately but waits for legacy migration before upload/profile write", async () => {
  const migration = deferred();
  const run = equipSetup(migration.promise);
  const pending = run.context.saveAppearance({ preventDefault() {} });
  await setImmediate();
  assert.deepEqual(run.actions, [["native PNG"], ["equipped locally", "slim"]]);
  assert.deepEqual(run.busy, [true]);
  run.actions.push(["legacy migration finished"]);
  migration.resolve();
  await pending;
  const stages = run.actions.map(([stage]) => stage);
  assert.ok(stages.indexOf("legacy migration finished") < stages.indexOf("upload"));
  assert.ok(stages.indexOf("upload") < stages.indexOf("save remote"));
  const appearance = run.actions.find(([stage]) => stage === "save remote")[1];
  assert.equal(appearance.skinUrl, "https://example/new.png");
  assert.equal(appearance.skinModel, "slim");
  assert.deepEqual(run.busy, [true, false]);
});

test("logout while waiting for legacy migration prevents any manual upload or profile write", async () => {
  const migration = deferred();
  const run = equipSetup(migration.promise);
  const pending = run.context.saveAppearance({ preventDefault() {} });
  await setImmediate();
  run.context.services.auth.currentUser = null;
  migration.resolve();
  await pending;
  assert.deepEqual(run.actions, [["native PNG"], ["equipped locally", "slim"]]);
  assert.deepEqual(run.busy, [true, false]);
});

test("a failed upload leaves the remote appearance intact and keeps the new PNG equipped locally", async () => {
  const run = equipSetup(Promise.resolve());
  run.context.uploadAppearanceImage = async () => { throw new Error("offline"); };
  await run.context.saveAppearance({ preventDefault() {} });
  assert.ok(run.actions.some(([step]) => step === "equipped locally"));
  assert.ok(!run.actions.some(([step]) => step === "save remote"));
  assert.match(run.actions.at(-1)[1], /tentar a sincronização/);
});

test("a verified public skin can be equipped on the online profile during a storage outage", async () => {
  const run = equipSetup(Promise.resolve());
  const reference = "https://aurora-api.aurora-edge-proxy.workers.dev/v1/skins/player-texture/example";
  run.context.localSkins[0].publicCatalogUrl = reference;
  run.context.uploadAppearanceImage = async () => { throw new Error("storage unavailable"); };
  run.context.verifyPublicCatalogSkin = async (source, file) => {
    assert.equal(source, reference);
    assert.equal(await file.text(), "PNG");
    return source;
  };
  await run.context.saveAppearance({ preventDefault() {} });
  const appearance = run.actions.find(([step]) => step === "save remote")[1];
  assert.equal(appearance.skinUrl, reference);
  assert.equal(appearance.avatarUrl, reference);
  assert.equal(appearance.skinModel, "slim");
});

test("switching accounts during public skin verification stops the profile write", async () => {
  const run = equipSetup(Promise.resolve());
  run.context.uploadAppearanceImage = async () => { throw new Error("offline"); };
  run.context.verifyPublicCatalogSkin = async () => {
    run.context.services.auth.currentUser = { uid: "new-account" };
    return "https://example/recovered.png";
  };
  await run.context.saveAppearance({ preventDefault() {} });
  assert.ok(!run.actions.some(([step]) => step === "save remote"));
});

test("an account switch during the profile write cannot restore the previous user's profile", async () => {
  const run = equipSetup(Promise.resolve());
  run.context.saveAuroraAppearance = async () => {
    run.context.services.auth.currentUser = { uid: "new-account" };
    return { uid: "current" };
  };
  await run.context.saveAppearance({ preventDefault() {} });
  assert.ok(!run.actions.some(([step]) => step === "profile"));
});
