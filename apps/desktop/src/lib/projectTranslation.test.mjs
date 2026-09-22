import assert from "node:assert/strict";
import test from "node:test";
import { parseHTML } from "linkedom";
import { createProjectTranslationJob, prepareProjectTranslation, secureProjectLinks, splitTranslationText, translationBatches, validateProjectTranslations } from "./projectTranslation.ts";

function element(html) { return parseHTML(`<html><body><div id="description">${html}</div></body></html>`).document.getElementById("description"); }
function topology(root) {
  return Array.from(root.querySelectorAll("*")).map((node) => [node.tagName, Array.from(node.attributes).map(({ name, value }) => [name, value])]);
}

test("traduz texto mantendo títulos, negrito, links, imagens, tabelas e espaços entre elementos", () => {
  const original = element('<h1 class="title">Amazing mod</h1><p>Download <strong>this mod</strong> from <a href="https://modrinth.com/mod/test" target="_blank">the project</a>. </p><img src="https://example.com/banner.png" alt="Banner text"><table><thead><tr><th>Version</th></tr></thead><tbody><tr><td>Latest</td></tr></tbody></table>');
  const before = original.innerHTML;
  const plan = prepareProjectTranslation(original);
  assert.deepEqual(plan.segments, ["Amazing mod", "Download", "this mod", "from", "the project", "Version", "Latest"]);
  const translated = element(plan.apply(["Mod incrível", "Baixe", "este mod", "em", "o projeto", "Versão", "Mais recente"]));
  assert.equal(original.innerHTML, before, "o original permanece intacto para cancelar/restaurar");
  assert.deepEqual(topology(translated), topology(original));
  assert.equal(translated.querySelector("p").textContent, "Baixe este mod em o projeto. ");
  assert.equal(translated.querySelector("h1").textContent, "Mod incrível");
});

test("código, comandos, URLs isoladas, elementos excluídos e legendas de imagem não são enviados", () => {
  const original = element('<p>Guide</p><pre><code>java -Xmx4G -jar server.jar</code></pre><p><code>config.json</code><kbd>Ctrl+R</kbd><samp>DONE</samp></p><a href="https://example.com">https://example.com</a><span translate="no">Aurora Core</span><span class="notranslate">Fabric API</span><strong data-translation-preserve>Sobre este projeto</strong><svg><text>Logo</text></svg><math><mi>x</mi></math><img alt="Screenshot" src="screenshot.png">');
  const plan = prepareProjectTranslation(original);
  assert.deepEqual(plan.segments, ["Guide"]);
  const translated = element(plan.apply(["Guia"]));
  assert.equal(translated.querySelector("code").textContent, "java -Xmx4G -jar server.jar");
  assert.deepEqual(topology(translated), topology(original));
});

test("texto traduzido com HTML permanece texto e não cria elementos executáveis", () => {
  const original = element("<p>Hello</p>");
  const translated = element(prepareProjectTranslation(original).apply(['<img src=x onerror="alert(1)">']));
  assert.equal(translated.querySelector("img"), null);
  assert.equal(translated.querySelector("p").textContent, '<img src=x onerror="alert(1)">');
});

test("links do catálogo mantêm a navegação fora do launcher e a proteção do opener", () => {
  const page = element('<p><a href="https://modrinth.com/mod/test">Download</a><a href="https://curseforge.com" target="_self" rel="opener">Project</a></p>');
  secureProjectLinks(page);
  const translated = element(prepareProjectTranslation(page).apply(["Baixar", "Projeto"]));
  for (const anchor of translated.querySelectorAll("a")) {
    assert.equal(anchor.getAttribute("target"), "_blank");
    assert.equal(anchor.getAttribute("rel"), "noopener noreferrer");
  }
});

test("falha ou tradução incompleta não altera nem parcialmente a página original", () => {
  const original = element("<p>One <strong>two</strong></p>");
  const before = original.innerHTML;
  const plan = prepareProjectTranslation(original);
  assert.throws(() => plan.apply(["Um"]), /incompleta/);
  assert.deepEqual(validateProjectTranslations([{ id: 1, text: "Dois" }, { id: 0, text: "Um" }], plan.segments), ["Um", "Dois"]);
  assert.throws(() => validateProjectTranslations([{ id: 0, text: "Um" }, { id: 0, text: "Dois" }], plan.segments), /conferir/);
  assert.throws(() => validateProjectTranslations([{ id: 0, text: "" }, { id: 1, text: "Dois" }], plan.segments), /conferir/);
  assert.throws(() => validateProjectTranslations(null, plan.segments), /incompleta/);
  assert.equal(original.innerHTML, before);
});

test("páginas grandes são divididas em lotes limitados sem perder texto ou pares Unicode", () => {
  const source = "  " + "Long sentence. ".repeat(900) + "🌟".repeat(2000) + " end.\n";
  const split = splitTranslationText(source);
  assert.equal(split.join(""), source);
  assert.ok(split.every((part) => part.length <= 2400 && !/[\uD800-\uDBFF]$/.test(part)));
  const original = element(`<p>${source}</p>`);
  const plan = prepareProjectTranslation(original);
  assert.equal(element(plan.apply(plan.segments)).textContent, source);
  const batches = translationBatches([...plan.segments, ...Array(110).fill("Title")]);
  assert.ok(batches.every((batch) => batch.length <= 48 && batch.join("").length <= 2500));
  assert.equal(batches.flat().length, plan.segments.length + 110);
  assert.throws(() => translationBatches(Array(20).fill("x".repeat(3500))), /muito longa/);
  assert.throws(() => translationBatches(Array(1601).fill("Title")), /muito longa/);
});

test("página com japonês, chinês e inglês mantém formatação e aceita expansão para português", () => {
  const original = element('<h2>導入方法</h2><p>请先安装 <strong>Fabric API</strong>。 Install the mod.</p><pre>java -jar server.jar</pre>');
  const plan = prepareProjectTranslation(original);
  assert.deepEqual(plan.segments, ["導入方法", "请先安装", "Fabric API", "。 Install the mod."]);
  const translated = element(plan.apply(["Como instalar", "Primeiro, instale", "Fabric API", ". Instale o mod."]));
  assert.deepEqual(topology(translated), topology(original));
  assert.equal(translated.querySelector("strong").textContent, "Fabric API");
  const prose = "这是一个测试".repeat(30);
  const expanded = "Uma descrição traduzida para português. ".repeat(40);
  assert.equal(validateProjectTranslations([{ id: 0, text: expanded }], [prose])[0], expanded.trim());
});

test("textos CJK longos respeitam pontuação e orçamento menor sem perder caracteres", () => {
  const source = "日本語の説明です。中文的安装说明。🌟".repeat(400);
  const split = splitTranslationText(source);
  assert.equal(split.join(""), source);
  assert.ok(split.every(text => text.length <= 800 && !/[\uD800-\uDBFF]$/.test(text)));
  assert.ok(split.slice(0, -1).every(text => /[。！？]$/.test(text)));
});

test("pontuação chinesa junto a link recebe espaço português sem modificar código", () => {
  const source = element('<p>首先，<a href="https://example.com">下载文件</a>然后运行<code>start.bat</code>。</p>');
  const before = source.innerHTML;
  const plan = prepareProjectTranslation(source);
  const result = element(plan.apply(["Primeiro,", "baixe o arquivo", "e execute"]));
  assert.equal(result.querySelector("p").textContent, "Primeiro, baixe o arquivo e execute start.bat。");
  assert.deepEqual(topology(result), topology(source));
  assert.equal(plan.apply(plan.segments), before);
});

test("falha em lote posterior preserva progresso e retoma apenas os trechos restantes", async () => {
  const original = element(Array.from({ length: 60 }, (_, index) => `<p>Text ${index}</p>`).join(""));
  const job = createProjectTranslationJob(original);
  const seen = [];
  let progress;
  const translate = async segments => { seen.push(segments); return segments.map((text, id) => ({ id, text: text.replace("Text", "Texto") })); };
  await assert.rejects(job.run(async segments => {
    if (seen.length) throw Error("rede indisponível");
    return translate(segments);
  }, new AbortController().signal, value => { progress = value; }), /rede indisponível/);
  assert.equal(progress.completed, 48);
  assert.equal(progress.complete, false);
  assert.ok(progress.html.includes("Texto 0") && progress.html.includes("Text 59"));
  const final = await job.run(translate, new AbortController().signal, () => {});
  assert.equal(seen.length, 2);
  assert.equal(seen[1][0], "Text 48");
  assert.equal(final.complete, true);
  assert.equal(final.completed, 60);
  assert.ok(original.textContent.includes("Text 0"));
});

test("cancelamento descarta resposta tardia e permite tentar o lote novamente", async () => {
  const job = createProjectTranslationJob(element("<p>Hello world</p>"));
  const controller = new AbortController();
  let updates = 0;
  await assert.rejects(job.run(async () => {
    controller.abort();
    return [{ id: 0, text: "Olá, mundo" }];
  }, controller.signal, () => { updates += 1; }), { name: "AbortError" });
  assert.equal(updates, 0);
  const final = await job.run(async () => [{ id: 0, text: "Olá, mundo" }], new AbortController().signal, () => {});
  assert.equal(final.completed, 1);
  assert.equal(final.changed, true);
});

test("sem texto traduzível e retorno idêntico não indicam mudança", async () => {
  const signal = new AbortController().signal;
  const empty = await createProjectTranslationJob(element('<img alt="Words" src="banner.png">')).run(async () => { throw Error("não deve chamar API"); }, signal, () => {});
  assert.equal(empty.total, 0);
  assert.equal(empty.changed, false);
  const same = await createProjectTranslationJob(element("<p>Fabric API</p>")).run(async () => [{ id: 0, text: "Fabric API" }], signal, () => {});
  assert.equal(same.changed, false);
  assert.equal(same.complete, true);
});
