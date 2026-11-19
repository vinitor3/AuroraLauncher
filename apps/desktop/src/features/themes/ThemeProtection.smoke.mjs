import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { parseHTML } from "linkedom";

test("theme selectors leave official security controls outside custom styling", async () => {
  const css = (await readFile(new URL("shell.css", import.meta.url), "utf8")).replace(/\/\*[\s\S]*?\*\//g, "");
  const { document } = parseHTML(`<html><body><div class="launcher-theme-root aurora-themed" data-custom-cursor="true">
    <section class="panel"><h2>Comunidade</h2><button id="ordinary">Entrar</button></section>
    <section class="panel theme-protected" id="review"><h2>Revisão</h2><p class="muted">Limites</p>
      <label>Autorização<input type="checkbox"></label><input type="text"><select><option>Oficial</option></select><textarea></textarea>
      <button id="approve">Confirmar</button><button class="secondary">Cancelar</button><button class="danger">Encerrar</button><a href="#">Detalhes</a>
    </section></div></body></html>`);
  const review = document.getElementById("review");
  let ordinaryControls = 0;
  let officialReset = 0;
  // The checked stylesheet contains code-owned selectors, not user CSS. Test
  // their actual DOM matches, including selectors inside media/container rules.
  for (const [, rawSelector, declarations] of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const selector = rawSelector.trim();
    if (!selector.includes(".aurora-themed") || selector.startsWith("@")) continue;
    if (selector === ".aurora-themed .theme-protected") {
      officialReset++;
      assert.match(declarations, /background:\s*#110d20/);
      assert.match(declarations, /cursor:\s*auto/);
      assert.match(declarations, /--theme-scale:\s*1/);
      assert.doesNotMatch(declarations, /var\(/, "security defaults do not inherit theme values");
      continue;
    }
    const matches = [...document.querySelectorAll(selector.replace(/::before/g, ""))];
    assert.ok(matches.every((element) => element !== review && !review.contains(element)), selector);
    if (matches.includes(document.getElementById("ordinary"))) ordinaryControls++;
  }
  assert.equal(officialReset, 1);
  assert.ok(ordinaryControls >= 2, "ordinary cards retain custom button/cursor styling");
  assert.equal(document.getElementById("approve").disabled, false, "appearance does not replace functional state");
});
