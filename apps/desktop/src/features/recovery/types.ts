export type RecoveryFile = { path: string; size: number; sha256: string };

export type RecoveryPoint = {
  id: string;
  createdAt: number;
  description: string;
  scope: string[];
  logicalBytes: number;
  fileCount: number;
  kind: "manual" | "undo";
  excluded: string[];
  profile: Record<string, Record<string, string | null>>;
  files: RecoveryFile[];
};

export type RecoveryOverview = {
  points: RecoveryPoint[];
  unavailablePoints: RecoveryUnavailablePoint[];
  undoPoint: RecoveryPoint | null;
  undoUnavailable: boolean;
  physicalBytes: number;
  scope: string[];
  excluded: string[];
};

export type RecoveryUnavailablePoint = { id: string; createdAt: number; reason: string };

export type RecoveryPreview = {
  id: string;
  point: RecoveryPoint;
  changes: { path: string; action: "add" | "replace" | "remove" }[];
  profileChanges: { file: string; field: string; before: string | null; after: string | null }[];
  excluded: string[];
  currentFingerprint: string;
};

export type RecoveryResult = {
  restoredPointId: string;
  undoAvailable: boolean;
  recoveredInterrupted: boolean;
};

export const RECOVERY_SCOPE = ["mods/** (incluindo mods desativados e excluindo módulos Aurora administrados)", "Configurações locais de texto consideradas seguras em config/** e defaultconfigs/**", "options.txt", "optionsof.txt", "optionsshaders.txt", "Nome de apresentação do modpack (aurora-modpack.json.name)"];
export const RECOVERY_EXCLUDED = ["Mundos (saves/)", "Screenshots, logs e caches", "Java, bibliotecas e instalações compartilhadas", "Shaders e pacotes de recursos", "Identidade da instância, contas, sessões, tokens e caminhos externos", "Módulos administrados pelo Aurora"];

// Native errors never become arbitrary UI text: even unexpected exceptions or
// sensitive filesystem errors are reduced to these fixed public messages.
const PUBLIC_ERRORS = new Set([
  "Não foi possível gravar ou ler a recuperação. Verifique o espaço livre e as permissões; o estado anterior foi preservado ou aguarda recuperação segura.",
  "O armazenamento da recuperação está indisponível ou corrompido. Nenhum ponto foi aplicado.",
  "O ponto contém dados ausentes, corrompidos ou um manifesto inválido. A restauração foi bloqueada.",
  "Um caminho, link ou arquivo não é seguro para recuperação. A operação foi bloqueada.",
  "Os arquivos mudaram durante a operação. Feche programas que alteram a instância e tente novamente.",
  "A prévia ficou desatualizada. Gere uma nova prévia antes de confirmar.",
  "A instância original mudou desde a criação desta cópia. Crie uma nova cópia para testar o estado atual.",
  "O ponto não pertence a esta instância. Instâncias recriadas não reutilizam pontos antigos.",
  "A versão ou o loader da instância mudou. Este ponto não pode restaurar mods para outro ambiente.",
  "Não há espaço suficiente para proteger o estado anterior e preparar a restauração. Nenhum arquivo foi alterado.",
  "Não existe um ponto disponível para esta operação.",
  "A recuperação requer o bloqueio exclusivo desta instância com o jogo fechado.",
  "A instância ultrapassa o limite de recuperação: 30 mil arquivos ou 8 GiB protegidos.",
  "Há uma recuperação interrompida. Abra Recuperação com o jogo fechado antes de alterar ou iniciar esta instância.",
  "A Recuperação requer a verificação de processos disponível no Windows.",
  "Esta instância está em uso. Aguarde a operação em andamento terminar.",
  "Não foi possível bloquear a instância com segurança. Verifique suas permissões e tente novamente.",
  "Identificador de instância inválido.",
  "A instância não existe mais.",
  "Feche o Minecraft antes de usar a Recuperação desta instância.",
  "A operação de recuperação foi interrompida. Abra Recuperação novamente para verificar o estado seguro da instância.",
  "Esta instância já está aberta no Minecraft, inclusive se o launcher que a iniciou foi fechado. Use outra instância para jogar com duas contas.",
  "Não foi possível confirmar que esta instância está livre. Feche o Minecraft/Java em execução e tente novamente.",
]);
export function recoveryError(cause: unknown): string {
  if (typeof cause === "string" && PUBLIC_ERRORS.has(cause)) return cause;
  return "Não foi possível concluir a operação com segurança. Atualize a lista para verificar operações pendentes e tente novamente. Se persistir, confira o espaço livre e se a instância está em uso.";
}

export function recoveryBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "Indisponível";
  if (bytes < 1024) return `${bytes.toLocaleString("pt-BR")} B`;
  const units = ["KiB", "MiB", "GiB", "TiB"];
  let number = bytes / 1024;
  let unit = 0;
  while (number >= 1024 && unit < units.length - 1) { number /= 1024; unit += 1; }
  return `${number.toLocaleString("pt-BR", { maximumFractionDigits: 1 })} ${units[unit]}`;
}

export function recoveryDate(timestamp: number): string {
  const date = new Date(timestamp);
  return Number.isNaN(date.getTime()) ? "Data indisponível" : date.toLocaleString("pt-BR");
}
