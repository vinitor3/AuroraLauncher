import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { existsSync } from "node:fs";
import { copyFile, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";
import { promisify } from "node:util";
import { build } from "vite";

const desktop = fileURLToPath(new URL("../", import.meta.url));
const browser = [
  process.env.AURORA_TEST_BROWSER, "/usr/bin/google-chrome", "/usr/bin/chromium",
  "/usr/bin/chromium-browser", "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  ...[process.env.ProgramFiles, process.env["ProgramFiles(x86)"], process.env.LOCALAPPDATA]
    .filter(Boolean).flatMap(dir => [join(dir, "Google/Chrome/Application/chrome.exe"), join(dir, "Microsoft/Edge/Application/msedge.exe")]),
].find(path => path && existsSync(path));

test("o detalhe cabe nas duas janelas e devolve o foco ao cancelar pelo navegador", {
  skip: browser ? false : "Defina AURORA_TEST_BROWSER para um executável Chromium instalado",
}, async t => {
  const dir = await mkdtemp(join(tmpdir(), "aurora-catalog-window-"));
  t.after(() => rm(dir, { recursive: true, force: true, maxRetries: 8, retryDelay: 100 }));
  const entry = join(dir, "fixture.jsx");
  const dialogPath = join(desktop, "src/features/catalog/ProjectDialog.tsx").replaceAll("\\", "/");
  await writeFile(entry, `
    import { createRoot } from "react-dom/client";
    import { flushSync } from "react-dom";
    import { ProjectDialog } from ${JSON.stringify(dialogPath)};
    const root = createRoot(document.getElementById("detail"));
    const trigger = document.getElementById("trigger");
    trigger.focus();
    const list = document.querySelector(".pack-compact-list");
    const result = { columns: getComputedStyle(list).gridTemplateColumns.split(" ").length };
    const close = () => {
      flushSync(() => root.render(null));
      result.focusRestored = document.activeElement === trigger;
      result.closed = !document.querySelector("dialog");
      window.frameElement.dataset.result = encodeURIComponent(JSON.stringify(result));
    };
    flushSync(() => root.render(
      <ProjectDialog title="Modpack" onClose={close}>
        <header className="project-detail-header"><h2>Modpack</h2><button id="close">Fechar</button></header>
        <div className="project-detail-scroll"><p>{"Descrição da versão. ".repeat(2800)}</p></div>
        <footer className="project-detail-actions modpack-install-actions">
          <a href="#projeto">Abrir projeto</a>
          <label className="detail-version-select">Minecraft<select><option>1.21.11</option></select></label>
          <label className="detail-destination-select">Destino<select><option>Nova instância</option></select></label>
          <button id="install">Criar e instalar</button>
        </footer>
      </ProjectDialog>));
    const dialog = document.querySelector("dialog"), body = dialog.querySelector(".project-detail-scroll");
    const bounds = dialog.getBoundingClientRect(), action = document.getElementById("install").getBoundingClientRect();
    result.focusTrapped = dialog.matches(":modal") && document.activeElement.id === "close";
    result.actionVisible = action.top >= bounds.top && action.bottom <= innerHeight && action.right <= innerWidth;
    result.noHorizontalScroll = dialog.scrollWidth <= dialog.clientWidth;
    result.bodyScrollable = body.scrollHeight > body.clientHeight;
    dialog.requestClose();
  `);
  const bundle = await build({ configFile: false, root: desktop, logLevel: "error",
    oxc: { jsx: { runtime: "automatic" } }, resolve: { dedupe: ["react", "react-dom"] },
    define: { "process.env.NODE_ENV": JSON.stringify("production") },
    build: { write: false, lib: { entry, name: "catalogWindowFixture", formats: ["iife"] } },
  });
  const js = bundle[0].output.find(asset => asset.type === "chunk").code;
  await writeFile(join(dir, "fixture.js"), js);
  await copyFile(join(desktop, "src/styles.css"), join(dir, "styles.css"));
  await copyFile(join(desktop, "src/features/window/window.css"), join(dir, "window.css"));
  for (const [width, height, columns] of [[960, 620, 1], [1440, 900, 2]]) {
    const html = join(dir, "test.html");
    await writeFile(html, `<!doctype html><iframe src="frame.html" width="${width}" height="${height}"></iframe>`);
    await writeFile(join(dir, "frame.html"), `<!doctype html><meta charset="utf-8"><link rel="stylesheet" href="styles.css">
      <link rel="stylesheet" href="window.css"><div class="desktop-window"><main class="catalog-panel" style="width:calc(100vw - 280px)">
      <ul class="pack-compact-list"><li><button class="pack-row" id="trigger">Modpack</button></li><li>Outro modpack</li></ul>
      </main><div id="detail"></div></div><script src="fixture.js"></script>`);
    const args = ["--headless", "--disable-gpu", "--disable-background-networking", "--no-first-run", "--allow-file-access-from-files",
      "--virtual-time-budget=2500", "--user-data-dir=" + join(dir, "browser"), "--dump-dom", pathToFileURL(html).href];
    if (process.getuid?.() === 0) args.unshift("--no-sandbox");
    const { stdout } = await promisify(execFile)(browser, args, { timeout: 30000, maxBuffer: 2 ** 20, windowsHide: true });
    const encoded = stdout.match(/data-result="([^"]+)"/)?.[1];
    assert.ok(encoded, `Resultado do navegador ausente em ${width}×${height}`);
    assert.deepEqual(JSON.parse(decodeURIComponent(encoded)), { columns, focusTrapped: true, actionVisible: true,
      noHorizontalScroll: true, bodyScrollable: true, focusRestored: true, closed: true });
  }
});
