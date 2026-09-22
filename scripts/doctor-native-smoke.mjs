// Actual native/WebView smoke. No fixtures, launch, install, repair or settings
// writes. Existing editor artwork caching may run when opening the manager.
// Output is restricted to aggregate checks/counts; never print IDs, names, paths,
// report text, log excerpts, account data or raw native/CDP exception messages.
// PowerShell: $env:AURORA_DOCTOR_CDP_PORT = "9223"; node scripts/doctor-native-smoke.mjs
const port = Number(process.env.AURORA_DOCTOR_CDP_PORT ?? "9223");
if (!Number.isInteger(port) || port < 1024 || port > 65535) {
  console.error(JSON.stringify({ result: "FAIL", code: "INVALID_CDP_PORT" }));
  process.exit(1);
}

let socket;
try {
  const response = await fetch(`http://127.0.0.1:${port}/json`, { signal: AbortSignal.timeout(5000) });
  if (!response.ok) throw new Error("CDP_UNAVAILABLE");
  const targets = await response.json();
  const target = targets.find((item) => item.type === "page" && item.title === "Aurora Smart Launcher");
  if (!target?.webSocketDebuggerUrl) throw new Error("AURORA_WEBVIEW_NOT_FOUND");
  socket = new WebSocket(target.webSocketDebuggerUrl);
  const pending = new Map();
  let sequence = 0;
  socket.onmessage = (event) => {
    const message = JSON.parse(event.data);
    const operation = pending.get(message.id);
    if (!operation) return;
    pending.delete(message.id);
    clearTimeout(operation.timer);
    message.error ? operation.reject(new Error("CDP_COMMAND_FAILED")) : operation.resolve(message.result);
  };
  await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("CDP_CONNECT_TIMEOUT")), 10000);
    socket.onopen = () => { clearTimeout(timer); resolve(); };
    socket.onerror = () => { clearTimeout(timer); reject(new Error("CDP_CONNECT_FAILED")); };
  });
  function call(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = ++sequence;
      const timer = setTimeout(() => { pending.delete(id); reject(new Error("NATIVE_SMOKE_TIMEOUT")); }, 90000);
      pending.set(id, { resolve, reject, timer });
      socket.send(JSON.stringify({ id, method, params }));
    });
  }
  await call("Runtime.enable");
  const result = await call("Runtime.evaluate", {
    awaitPromise: true,
    returnByValue: true,
    expression: `(${(async function doctorNativeSmoke() {
      const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
      const ensure = (condition, code) => { if (!condition) throw new Error(code); };
      let stage = "NATIVE_BRIDGE";
      try {
        const readyUntil = Date.now() + 15000;
        while (!window.__TAURI_INTERNALS__?.invoke && Date.now() < readyUntil) await pause(100);
        ensure(window.__TAURI_INTERNALS__?.invoke, "NATIVE_BRIDGE_MISSING");
        const invoke = window.__TAURI_INTERNALS__.invoke;
        stage = "INSTANCE_INVENTORY";
        const instances = await invoke("list_instances");
        ensure(Array.isArray(instances), "INVALID_INSTANCE_INVENTORY");
        const installed = instances.filter((item) => item.hasInstalledVersion);
        if (!installed.length) return { result: "SKIP", code: "NO_INSTALLED_INSTANCE", instances: instances.length, installedInstances: 0, synthetic: false };
        const instance = installed[0];
        stage = "SAVED_LAUNCH_PROFILE";
        const profile = await invoke("read_instance_launch_profile", { id: instance.id });
        ensure(profile?.minecraftVersion && profile?.versionId, "INSTALLED_PROFILE_INCOMPLETE");
        stage = "NATIVE_ANALYZE_INSTANCE";
        const report = await invoke("analyze_instance", { id: instance.id, javaExecutable: null, runId: null });
        function verifyReport(value) {
          ensure(value?.schemaVersion === 1, "REPORT_SCHEMA_VERSION");
          ensure(["issues", "no_issues", "no_logs", "incomplete"].includes(value.status), "REPORT_STATUS");
          ensure(value.instance?.minecraftVersion === profile.minecraftVersion, "REPORT_PROFILE_MISMATCH");
          ensure(Array.isArray(value.issues) && Array.isArray(value.logs) && Array.isArray(value.limitations), "REPORT_COLLECTIONS");
          ensure(typeof value.sanitizedJson === "string", "SANITIZED_REPORT_MISSING");
          const shared = JSON.parse(value.sanitizedJson);
          ensure(shared.schemaVersion === value.schemaVersion && shared.status === value.status, "SANITIZED_SCHEMA_MISMATCH");
          ensure(shared.issues.length === value.issues.length && shared.logs.length === value.logs.length, "SANITIZED_COUNTS_MISMATCH");
          ensure(!Object.hasOwn(shared, "sanitizedJson") && !Object.hasOwn(shared, "instanceId"), "SANITIZED_PRIVATE_FIELDS");
          ensure(!Object.hasOwn(shared.instance, "id") && !Object.hasOwn(shared.instance, "path") && !Object.hasOwn(shared.instance, "name"), "SANITIZED_INSTANCE_IDENTITY");
          const serialized = value.sanitizedJson.replaceAll("\\\\", "\\");
          ensure(!serialized.includes(instance.path), "SANITIZED_INSTANCE_PATH");
          ensure(!/[a-z]:[\\/]Users[\\/]|\/(?:Users|home)\//i.test(serialized), "SANITIZED_PERSONAL_PATH");
          ensure(!/eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}/.test(serialized), "SANITIZED_JWT");
          ensure(!/(?:access_token|refresh_token|api_key|password)\s*[=:]\s*[^\s\"]{6,}/i.test(serialized), "SANITIZED_CREDENTIAL");
          for (const issue of value.issues) {
            ensure(["error", "warning", "info"].includes(issue.severity), "ISSUE_SEVERITY");
            ensure(["high", "medium"].includes(issue.confidence), "ISSUE_CONFIDENCE");
            ensure(["fact", "hypothesis"].includes(issue.kind), "ISSUE_FACT_OR_HYPOTHESIS");
            ensure(issue.title && issue.explanation && issue.nextStep && Array.isArray(issue.evidence), "ISSUE_DETAILS_MISSING");
          }
          for (const log of value.logs) {
            ensure(["last_run", "historical", "unassociated"].includes(log.association), "LOG_ASSOCIATION");
            ensure(Number.isFinite(log.bytesRead) && log.bytesRead >= 0, "LOG_BYTE_COUNT");
          }
          return true;
        }
        verifyReport(report);
        const output = {
          result: "PASS", synthetic: false,
          instances: instances.length, installedInstances: installed.length,
          nativeCommand: true, savedProfileMatches: true, structuredReport: true,
          sanitizedReport: true, status: report.status,
          issueCount: report.issues.length, logCount: report.logs.length,
          historicalLogCount: report.logs.filter((log) => log.association === "historical").length,
          bytesRead: report.logs.reduce((total, log) => total + log.bytesRead, 0),
          hasRecordedRun: Boolean(report.lastRun), ui: "SKIP_NOT_AUTHENTICATED",
        };
        const authenticatedUntil = Date.now() + 15000;
        while (!document.querySelector(".app-nav") && !document.querySelector(".auth-shell") && Date.now() < authenticatedUntil) await pause(100);
        if (!document.querySelector(".app-nav")) return output;
        stage = "DOCTOR_UI_OPEN";
        const instanceNav = [...document.querySelectorAll(".app-nav button")].find((button) => button.textContent.trim().endsWith("Instâncias"));
        if (instanceNav) instanceNav.click();
        await pause(100);
        const card = [...document.querySelectorAll(".instance")].find((item) => {
          const copy = item.querySelector(".instance-copy");
          return copy && (copy.querySelector("small")?.textContent === instance.id || copy.querySelector("strong")?.textContent === instance.id);
        });
        if (!card) return { ...output, ui: "SKIP_INSTANCE_NOT_VISIBLE" };
        const edit = [...card.querySelectorAll("button")].find((button) => button.textContent.trim() === "Editar" && !button.disabled);
        if (!edit) return { ...output, ui: "SKIP_MANAGER_UNAVAILABLE" };
        edit.click();
        const editorUntil = Date.now() + 10000;
        while (!document.getElementById("editor-view-tab-doctor") && Date.now() < editorUntil) await pause(100);
        const tab = document.getElementById("editor-view-tab-doctor");
        ensure(tab, "DOCTOR_TAB_MISSING");
        tab.click();
        await pause(100);
        const analyze = [...document.querySelectorAll(".doctor-view button")].find((button) => button.textContent.trim() === "Analisar instância" && !button.disabled);
        if (!analyze) return { ...output, ui: "SKIP_ANALYZE_BUSY" };
        stage = "DOCTOR_UI_ANALYZE";
        analyze.click();
        const analyzedUntil = Date.now() + 30000;
        let sharedField;
        while (Date.now() < analyzedUntil) {
          sharedField = document.querySelector('.doctor-share textarea[aria-label="Conteúdo do relatório sanitizado"]');
          if (sharedField || document.querySelector(".doctor-state-error")) break;
          await pause(100);
        }
        ensure(sharedField, "DOCTOR_UI_REPORT_MISSING");
        const uiShared = JSON.parse(sharedField.value);
        verifyReport({ ...uiShared, sanitizedJson: sharedField.value });
        ensure(document.querySelectorAll("details.doctor-issue").length === uiShared.issues.length, "DOCTOR_UI_ISSUE_COUNT");
        ensure(document.querySelector(".doctor-result-summary"), "DOCTOR_UI_SUMMARY_MISSING");
        return { ...output, ui: "PASS", uiIssueCount: uiShared.issues.length, uiLogCount: uiShared.logs.length, uiSanitizedReport: true };
      } catch (error) {
        // Return only codes authored in this script, never native error text.
        const code = typeof error?.message === "string" && /^[A-Z][A-Z_]+$/.test(error.message) ? error.message : "OPERATION_FAILED";
        return { result: "FAIL", stage, code, synthetic: false };
      }
    }).toString()})()`,
  });
  if (result.exceptionDetails) throw new Error("WEBVIEW_EVALUATION_FAILED");
  const aggregate = result.result?.value;
  if (!aggregate || !["PASS", "SKIP", "FAIL"].includes(aggregate.result)) throw new Error("SMOKE_RESULT_MISSING");
  console.log(JSON.stringify(aggregate, null, 2));
  if (aggregate.result === "FAIL") process.exitCode = 1;
} catch (error) {
  const allowlisted = new Set(["CDP_UNAVAILABLE", "AURORA_WEBVIEW_NOT_FOUND", "CDP_COMMAND_FAILED", "CDP_CONNECT_TIMEOUT", "CDP_CONNECT_FAILED", "NATIVE_SMOKE_TIMEOUT", "WEBVIEW_EVALUATION_FAILED", "SMOKE_RESULT_MISSING"]);
  console.error(JSON.stringify({ result: "FAIL", code: allowlisted.has(error.message) ? error.message : "SMOKE_CONNECTION_FAILED", synthetic: false }));
  process.exitCode = 1;
} finally {
  socket?.close();
}
