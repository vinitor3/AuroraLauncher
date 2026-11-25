import assert from "node:assert/strict";
import test from "node:test";
import { formatDoctorContext, readSupportFile, supportExcerpt } from "./diagnosticContext.ts";

test("Doctor: omite ID de execução, nomes e metadados locais", () => {
  const context = formatDoctorContext({
    status: "issues", summary: "Falha durante o carregamento.",
    instance: { minecraftVersion: "1.20.1", loader: "fabric", javaMajor: 17, maxMemoryMb: 4096 },
    lastRun: { runId: "private-run", exitCode: 1, startedAtMs: 424242 },
    issues: [{ kind: "hypothesis", title: "Mods incompatíveis", explanation: "Confira as versões." }],
    logs: [{ source: "/private/path/latest.log" }], limitations: ["Leitura parcial."],
    sanitizedJson: "metadados internos",
  });
  assert.match(context, /Java 17[\s\S]*Hipótese · Mods incompatíveis/);
  assert.doesNotMatch(context, /private|424242|metadados/);
});

test("o trecho opcional mantém erros desconhecidos e retira linhas sensíveis", () => {
  const sample = [
    "BrokenWidgetException: missing copper_panel",
    "Authorization: Bearer private-value",
    "Cookie: private-cookie",
    "[CHAT] <alex> conversa privada",
    "Widget crash in C:\\Users\\fixture\\mods\\widget.jar",
    "Server at [2001:db8::1] and 10.10.0.12:25565 (admin@example.org)",
    "Details: https://example.invalid/private",
  ].join("\n");
  const excerpt = supportExcerpt(sample);
  assert.match(excerpt, /BrokenWidgetException: missing copper_panel/);
  assert.doesNotMatch(excerpt, /private|alex|fixture|db8|10\.10|admin@|https:/i);
  assert.equal(supportExcerpt("-----BEGIN PRIVATE KEY-----\nnot-a-key"), "");
  assert.match(supportExcerpt("Cabeçalho\n" + ("linha de execução\n").repeat(9000) + "Fim da pilha"), /Cabeçalho[\s\S]+trecho omitido[\s\S]+Fim da pilha/);
});

test("lê os extremos de crashes grandes e recusa arquivos binários", async () => {
  const file = new File(["Minecraft Version: 1.20.1\n", "frame\n".repeat(150000), "\nCaused by: WidgetCrash"], "crash.txt");
  assert.match(await readSupportFile(file), /Minecraft Version[\s\S]+trecho omitido[\s\S]+WidgetCrash/);
  await assert.rejects(readSupportFile(new File(["content"], "mods.jar")), /Escolha um arquivo/);
});

test("preserva erros de sintaxe sem copiar o texto citado pelo parser nem credenciais", () => {
  const json = "Caused by: com.google.gson.JsonSyntaxException: Unexpected token";
  const sample = [json + " END_OBJECT", json + " 'PRIVATE_INPUT' in JSON",
    "accessToken=PRIVATE_PREFIX " + json, "token=PRIVATE_TOKEN", "Authorization: Bearer PRIVATE_AUTH"].join("\n");
  assert.equal(supportExcerpt(sample), json + " END_OBJECT\n" + json);
  assert.equal(supportExcerpt("A\0u\0t\0h"), "");
});

function utf16(text, bigEndian, bom) {
  const bytes = Buffer.from((bom ? "\ufeff" : "") + text, "utf16le");
  return bigEndian ? bytes.swap16() : bytes;
}

test("UTF-16 LE e BE com e sem BOM preservam a causa e filtram credenciais antes de copiar", async () => {
  const header = "---- Minecraft Crash Report ----\r\nAuthorization: Bearer PRIVATE_HEADER\r\n";
  const end = "\r\nCookie: PRIVATE_TAIL\r\nCaused by: com.example.Error: Unexpected token END_ARRAY";
  for (const bigEndian of [false, true]) for (const bom of [false, true]) {
    for (const middle of ["Stack trace", "Linha de execução 🎮\r\n".repeat(18000)]) {
      const bytes = utf16(header + middle + end, bigEndian, bom);
      const excerpt = await readSupportFile(new File([bytes], "servidor.log"));
      assert.match(excerpt, /^---- Minecraft Crash Report ----/);
      assert.match(excerpt, /Caused by: com\.example\.Error: Unexpected token END_ARRAY$/);
      assert.doesNotMatch(excerpt, /PRIVATE_|Authorization|Cookie|\0|\ufffd/);
      if (bytes.byteLength > 512 * 1024) assert.match(excerpt, /trecho omitido/);
    }
  }
});

test("recusa dados binários e UTF-16 incompleto mesmo com uma extensão de log", async () => {
  for (const bytes of [
    Uint8Array.of(0, 3, 0x80, 0xff, 0, 1),
    Uint8Array.of(0xff, 0xfe, 0, 0, 65, 0, 0, 0),
    Uint8Array.of(0xff, 0xfe, 65, 0, 66),
    Buffer.from("Normal header\n\x01binary", "utf8"),
  ]) {
    await assert.rejects(readSupportFile(new File([bytes], "servidor.log")), /binários ou uma codificação/);
  }
});

test("crash externo de linha única preserva só cabeçalho e classe da causa nas bordas", async () => {
  const head = "---- Minecraft Crash Report ---- Authorization: Bearer PRIVATE_HEADER ";
  const tail = " PRIVATE_PREFIX Caused by: example.ModFailure: PRIVATE_SUFFIX";
  for (const eol of ["", "\r\n"]) {
    const text = head + "frame ".repeat(100000) + tail + eol;
    for (const bytes of [Buffer.from(text), utf16(text, false, true), utf16(text, true, true)]) {
      assert.ok(bytes.byteLength > 512 * 1024);
      const excerpt = await readSupportFile(new File([bytes], "crash.txt"));
      assert.equal(excerpt, "---- Minecraft Crash Report ----\n[… trecho omitido …]\nCaused by: example.ModFailure");
    }
  }
  const privateLine = "token=" + "x".repeat(600000) + " Caused by: PRIVATE_TOKEN";
  assert.equal(await readSupportFile(new File([privateLine], "private.log")), "");
});
