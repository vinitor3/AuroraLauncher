import type { DoctorReport } from "../doctor/types";

export const supportSources = ["Launcher", "Minecraft", "Servidor"] as const;
export type SupportSource = (typeof supportSources)[number];

// Emit only known labels and bounded version/exit numbers. Server logs may
// contain arbitrary chat, launch credentials, invitations and private paths.
const signals: [RegExp, string][] = [
  [/OutOfMemoryError|Java heap space|could not reserve enough space/i, "Memória insuficiente na JVM"],
  [/UnsupportedClassVersionError|compiled by a more recent version/i, "Versão incompatível do Java (UnsupportedClassVersionError)"],
  [/ClassNotFoundException|NoClassDefFoundError/i, "Classe ausente (ClassNotFoundException / NoClassDefFoundError)"],
  [/MixinApplyError|InvalidMixinException|MixinTransformerError|mixin apply.*failed/i, "Falha ao aplicar um mixin"],
  [/incompatible mod set|incompatible mods|incompatible version/i, "Versões incompatíveis de mods ou loader"],
  [/missing (?:required |mandatory )?dependenc|requires.+which is missing/i, "Dependência obrigatória ausente"],
  [/duplicate mods?|duplicate mod id/i, "Mod duplicado"],
  [/ZipException|invalid (?:loc|cen) header/i, "Arquivo ZIP ou JAR inválido (ZipException)"],
  [/failed to bind to port|BindException|address already in use/i, "Falha ao abrir a porta de rede"],
  [/UnknownHostException|unresolvedaddress/i, "Falha ao resolver o endereço de rede"],
  [/SocketTimeoutException|timed? out|tempo limite/i, "Tempo limite de conexão excedido"],
  [/ConnectException|connection refused|conexão recusada/i, "Conexão recusada"],
  [/connection reset|SocketException|Broken pipe/i, "Conexão interrompida"],
  [/SSLHandshakeException|PKIX path building failed/i, "Falha na validação do certificado TLS"],
  [/server watchdog|a single server tick took/i, "O watchdog detectou um tick de servidor muito longo"],
  [/ticking (?:entity|block entity)/i, "Falha ao atualizar entidade ou bloco"],
  [/NullPointerException/i, "NullPointerException"],
  [/StackOverflowError/i, "StackOverflowError"],
  [/AccessDeniedException|permission denied|acesso negado/i, "Acesso ao arquivo ou recurso negado"],
  [/FileNotFoundException|NoSuchFileException/i, "Arquivo não encontrado"],
  [/GLFW error|Failed to create window|pixel format not accelerated/i, "Falha gráfica ao iniciar o jogo"],
  [/failed to verify username|authentication servers are down|invalid session/i, "Falha de autenticação da sessão"],
  [/failed to synchronize registry|mismatched mod channel|registry remapping failed/i, "Cliente e servidor divergiram ao sincronizar mods ou registros"],
];

export function technicalSignals(log: string): string[] {
  // Bound work even on huge pasted crash reports. Preserve header and tail.
  const sample = log.length > 65536 ? log.slice(0, 32768) + "\n" + log.slice(-32768) : log;
  const result: string[] = [];
  const version = /\bMinecraft(?: Version)?[:\s]+(\d{1,2}\.\d{1,2}(?:\.\d{1,2})?)\b/i.exec(sample);
  const exit = /\b(?:exit code|código de saída)[:\s]+(-?\d{1,10})\b/i.exec(sample);
  if (version) result.push("Minecraft " + version[1]);
  if (exit) result.push("Código de saída " + exit[1]);
  for (const [matches, label] of signals) if (matches.test(sample)) result.push(label);
  return result;
}

// Only run when the player explicitly adds an excerpt to the editable message.
export function supportExcerpt(raw: string): string {
  const input = raw.slice(-65536);
  if (/-----BEGIN [^-]*(?:PRIVATE KEY|CERTIFICATE)-----/i.test(input)) return "";
  const sensitive = /(?:token|password|passwd|senha|authorization|cookie|sessionid|credential|secret|api[_ -]?key|private[_ -]?key|bearer\s|--username|--uuid|invitation|convite|webhook|<[^>\s]+>\s|\[chat\]|eyJ[A-Za-z0-9_-]+\.)/i;
  const lines = input.split(/\r?\n/).slice(-40).filter(line => line.trim() && !sensitive.test(line));
  return lines.map(line => line
    .replace(/(?:https?|wss?|ftp):\/\/\S+/gi, "[URL]")
    .replace(/\b[\w.%+-]+@[\w.-]+\.[a-z]{2,}\b/gi, "[e-mail]")
    .replace(/(?:[a-z]:[\\/]|\\\\|\/(?:Users|home|mnt|private|var|tmp)\/)[^\r\n]*/gi, "[caminho]")
    .replace(/\b(?:\d{1,3}\.){3}\d{1,3}(?::\d+)?\b/g, "[endereço]")
    .replace(/(?<!\w)(?=[a-f0-9:]*:[a-f0-9:]*:)[a-f0-9:]{3,}(?!\w)/gi, "[endereço]")
    .replace(/\b[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}\b/gi, "[ID]")
    .slice(0, 320)).join("\n").slice(-4000).trim();
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

export function buildSupportPrompt(source: SupportSource, context: string, log: string) {
  const findings = technicalSignals(log);
  return [
    `Preciso de ajuda com ${source === "Launcher" ? "o Aurora Launcher" : source === "Minecraft" ? "o Minecraft Java" : "um servidor de Minecraft Java"}.`,
    "Explique em português o que os dados permitem concluir, quais informações ainda faltam e qual teste reversível posso fazer primeiro.",
    context,
    findings.length ? "Sinais encontrados no texto local:\n" + findings.map(item => "• " + item).join("\n")
      : log.trim() ? "Nenhum sinal técnico reconhecido. Descreva aqui o erro observado." : "",
  ].filter(Boolean).join("\n\n");
}
