import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { parseTheme, THEME_EXAMPLES, themeVariables } from "../themes/model.ts";

const css = await readFile(new URL("social.css", import.meta.url), "utf8");
function declaration(selector, property) {
  for (const [, selectors, body] of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    if (!selectors.trim().split(/,\s*/).includes(selector)) continue;
    const value = body.split(";").map(item => item.trim()).find(item => item.startsWith(`${property}:`));
    if (value) return value.slice(property.length + 1).trim();
  }
  throw new Error(`Missing CSS declaration ${selector} ${property}`);
}
function resolveColor(value, variables) {
  const token = /^var\((--[\w-]+),\s*(#[0-9a-f]{6})\)$/i.exec(value);
  if (token) return variables[token[1]] ?? token[2];
  assert.match(value, /^#[0-9a-f]{6}$/i, "essential foreground/background must be opaque colors");
  return value;
}
function luminance(color) {
  const rgb = color.slice(1).match(/../g).map(hex => parseInt(hex, 16) / 255)
    .map(value => value <= .04045 ? value / 12.92 : ((value + .055) / 1.055) ** 2.4);
  return rgb[0] * .2126 + rgb[1] * .7152 + rgb[2] * .0722;
}
function readable(foreground, background) {
  const values = [luminance(foreground), luminance(background)].sort((a, b) => b - a);
  assert.ok((values[0] + .05) / (values[1] + .05) >= 4.5, `${foreground} on ${background}`);
}

test("social names, versions, population and actions remain readable with a valid light theme", () => {
  const light = parseTheme(JSON.stringify({ ...THEME_EXAMPLES[0], colors: {
    background: "#ffffff", panel: "#fafafa", text: "#202020", muted: "#ffffff",
    accent: "#123456", accentText: "#ffffff", border: "#666666",
  } }));
  for (const theme of [THEME_EXAMPLES[0], light]) {
    const variables = themeVariables(theme);
    const panel = resolveColor(declaration(".social-view", "background"), variables);
    assert.equal(panel, theme.colors.panel);
    for (const selector of [".social-view", ".social-view p", ".social-view small", ".social-self strong",
      ".social-search label", ".social-person strong", ".social-session-title p", ".social-session-meta span",
      ".social-session-meta .social-population", ".social-empty strong", ".social-availability", ".social-connection p",
      '.social-availability[data-state="ready"] strong', '.social-availability[data-state="unavailable"] strong',
      ".social-view .social-tabs button[aria-pressed]", ".social-view .social-layout-switch button[aria-pressed]"]) {
      readable(resolveColor(declaration(selector, "color"), variables), panel);
    }
    for (const selector of [".social-friends", ".social-community", ".social-session", ".social-session:hover", ".social-connection"]) {
      assert.equal(resolveColor(declaration(selector, "background"), variables), panel);
    }
    for (const selector of [".social-avatar", '.social-view .social-tabs button[aria-pressed="true"]',
      '.social-view .social-layout-switch button[aria-pressed="true"]']) {
      readable(resolveColor(declaration(selector, "color"), variables), resolveColor(declaration(selector, "background"), variables));
    }
  }
});

test("social errors and warnings retain official opaque contrast independently of the theme", () => {
  for (const kind of ["error", "warning"]) {
    const selector = `.social-notice[data-kind="${kind}"]`;
    readable(resolveColor(declaration(selector, "color"), {}), resolveColor(declaration(selector, "background"), {}));
  }
});
