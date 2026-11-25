import type { DoctorReport } from "../doctor/types";

export type SupportSource = "Launcher" | "Minecraft" | "Servidor";
const limit = 64 * 1024;
const omitted = "\n[… trecho omitido …]\n";
const sensitive = /(?:token|password|passwd|senha|authorization|cookie|session[_-]?id|credential|secret|api[_ -]?key|private[_ -]?key|bearer\s|--username|--uuid|invitation|convite|webhook|<[^>\s]+>\s|\[chat\]|eyJ[A-Za-z0-9_-]+\.|^[A-Za-z0-9+/=]{48,}$)/i;
const parserDiagnostic = /\b(unexpected token)\b([ \t]+(?:BEGIN_OBJECT|END_OBJECT|BEGIN_ARRAY|END_ARRAY|END_DOCUMENT|NAME|STRING|NUMBER|BOOLEAN|NULL)\b)?.*$/i;

export function supportExcerpt(raw: string): string {
  if (raw.includes("\0")) return "";
  const text = raw.replace(/-----BEGIN [^\r\n]*?-----[\s\S]*?(?:-----END [^\r\n]*?-----|$)/g, "")
    .split(/\r?\n/).filter(line => !sensitive.test(line.replace(parserDiagnostic, "")))
    // Parsers can quote arbitrary input after this message, including credentials.
    .map(line => line.replace(parserDiagnostic, "$1$2")
      .replace(/(?:https?|wss?|ftp):\/\/\S+/gi, "[URL]")
      .replace(/\b[\w.%+-]+@[\w.-]+\.[a-z]{2,}\b/gi, "[e-mail]")
      .replace(/(?:[a-z]:[\\/]|\\\\|\/(?:Users|home|mnt|private|var|tmp)\/)[^\r\n]*/gi, "[caminho]")
      .replace(/\b(?:\d{1,3}\.){3}\d{1,3}(?::\d+)?\b/g, "[endereço]")
      .replace(/(?<!\w)(?=[a-f0-9:]*:[a-f0-9:]*:)[a-f0-9:]{3,}(?!\w)/gi, "[endereço]")
      .replace(/\b[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}\b/gi, "[ID]"))
    .join("\n").trim();
  return text.length <= limit ? text : text.slice(0, limit / 2) + omitted + text.slice(-limit / 2);
}

type LogEncoding = "utf-8" | "utf-16le" | "utf-16be";
const invalidText = "O arquivo contém dados binários ou uma codificação de texto não reconhecida.";

function logEncoding(bytes: Uint8Array): LogEncoding {
  if (bytes[0] === 0xff && bytes[1] === 0xfe) return "utf-16le";
  if (bytes[0] === 0xfe && bytes[1] === 0xff) return "utf-16be";
  // BOM-less Java and Windows logs commonly start with ASCII text. Require
  // printable ASCII code units with consistently aligned zero bytes.
  const pairs = Math.min(128, Math.floor(bytes.length / 2));
  let little = 0, big = 0;
  const printable = (byte: number) => (byte >= 32 && byte <= 126) || byte === 9 || byte === 10 || byte === 13;
  for (let i = 0; i < pairs * 2; i += 2) {
    if (bytes[i + 1] === 0 && printable(bytes[i])) little++;
    if (bytes[i] === 0 && printable(bytes[i + 1])) big++;
  }
  if (pairs >= 8 && little >= pairs * .7 && big === 0) return "utf-16le";
  if (pairs >= 8 && big >= pairs * .7 && little === 0) return "utf-16be";
  return "utf-8";
}

function decodeLog(bytes: Uint8Array, encoding: LogEncoding, partialEnd = false): string {
  let text: string;
  try {
    text = new TextDecoder(encoding, { fatal: true }).decode(bytes, { stream: partialEnd });
  } catch {
    throw new Error(invalidText);
  }
  if (/[\u0000-\u0008\u000b\u000e-\u001a\u001c-\u001f\u007f]/.test(text)) throw new Error(invalidText);
  return text;
}

export async function readSupportFile(file: File): Promise<string> {
  const maxBytes = 512 * 1024;
  if (!/\.(?:log|txt|crash)$/i.test(file.name)) throw new Error("Escolha um arquivo .log, .txt ou .crash.");
  const clipped = file.size > maxBytes;
  const firstBytes = new Uint8Array(await file.slice(0, clipped ? maxBytes / 2 : file.size).arrayBuffer());
  const encoding = logEncoding(firstBytes);
  if (encoding !== "utf-8" && file.size % 2 !== 0) throw new Error(invalidText);
  const head = decodeLog(firstBytes, encoding, clipped);
  if (!clipped) return supportExcerpt(head);
  let lastBytes = new Uint8Array(await file.slice(-maxBytes / 2).arrayBuffer());
  if (encoding === "utf-8") {
    let offset = 0;
    while (offset < 3 && (lastBytes[offset] & 0xc0) === 0x80) offset++;
    lastBytes = lastBytes.subarray(offset);
  } else {
    const unit = new DataView(lastBytes.buffer).getUint16(0, encoding === "utf-16le");
    if (unit >= 0xdc00 && unit <= 0xdfff) lastBytes = lastBytes.subarray(2);
  }
  const tail = decodeLog(lastBytes, encoding);
  // Drop incomplete lines and PEM fragments crossing either read boundary.
  const first = head.slice(0, Math.max(0, head.lastIndexOf("\n")));
  const end = tail.indexOf("\n");
  const last = end === -1 ? "" : tail.slice(end + 1).replace(/^[\s\S]*?-----END [^\r\n]*?-----/, "");
  const parts = [supportExcerpt(first), supportExcerpt(last)].filter(Boolean);
  return parts.length ? supportExcerpt(parts.join(omitted)) : "";
}

export function formatDoctorContext(report: DoctorReport): string {
  // Doctor constructs these fields locally using fixed rules. Exclude the
  // instance/run identity, file metadata, recovery points and timestamps.
  const { instance, lastRun } = report;
  return [
    report.summary,
    instance.minecraftVersion && `Minecraft ${instance.minecraftVersion}`,
    instance.loader && `Loader ${instance.loader}`,
    instance.javaMajor && `Java ${instance.javaMajor}`,
    instance.maxMemoryMb > 0 && `Memória máxima ${instance.maxMemoryMb} MB`,
    lastRun?.exitCode != null && `Código de saída ${lastRun.exitCode}`,
    ...report.issues.map(issue => `${issue.kind === "fact" ? "Fato observado" : "Hipótese"} · ${issue.title}. ${issue.explanation}`),
    ...report.limitations,
  ].filter(Boolean).join("\n");
}
