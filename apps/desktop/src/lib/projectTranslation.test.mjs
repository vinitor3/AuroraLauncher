import assert from "node:assert/strict";
import test from "node:test";
import { parseHTML } from "linkedom";
import { prepareProjectTranslation, secureProjectLinks, splitTranslationText, translationBatches, validateProjectTranslations } from "./projectTranslation.ts";

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
  assert.throws(() => validateProjectTranslations([{ id: 1, text: "Um" }, { id: 0, text: "Dois" }], plan.segments), /conferir/);
  assert.throws(() => validateProjectTranslations([{ id: 0, text: "" }, { id: 1, text: "Dois" }], plan.segments), /conferir/);
  assert.throws(() => validateProjectTranslations(null, plan.segments), /incompleta/);
  assert.equal(original.innerHTML, before);
});

test("páginas grandes são divididas em lotes limitados sem perder texto ou pares Unicode", () => {
  const source = "  " + "Long sentence. ".repeat(900) + "🌟".repeat(2000) + " end.\n";
  const split = splitTranslationText(source);
  assert.equal(split.join(""), source);
  assert.ok(split.every((part) => part.length <= 3500 && !/[\uD800-\uDBFF]$/.test(part)));
  const original = element(`<p>${source}</p>`);
  const plan = prepareProjectTranslation(original);
  assert.equal(element(plan.apply(plan.segments)).textContent, source);
  const batches = translationBatches([...plan.segments, ...Array(110).fill("Title")]);
  assert.ok(batches.every((batch) => batch.length <= 96 && batch.join("").length <= 7000));
  assert.equal(batches.flat().length, plan.segments.length + 110);
  assert.throws(() => translationBatches(Array(20).fill("x".repeat(3500))), /muito longa/);
  assert.throws(() => translationBatches(Array(1601).fill("Title")), /muito longa/);
});
