import type { ConnectRole, ConnectSnapshot } from "./types";

export type ConnectDeadline = { key: string; label: string; expiresAtMs: number };
export type ConnectGuidance = { action: string; detail: string };
const finished = (value: ConnectSnapshot) => ["idle", "closed", "failed"].includes(value.phase);

// Presentation only: every deadline comes from the last validated native
// snapshot. No duration is added to Date.now() and no authority is renewed.
export function connectDeadlines(snapshot: ConnectSnapshot | undefined): ConnectDeadline[] {
  if (!snapshot || finished(snapshot)) return [];
  const result: ConnectDeadline[] = [];
  const preparation = snapshot.preparation ?? snapshot.hostGroup?.preparation;
  if (preparation) result.push({ key: "preparation", label: "Preparação", expiresAtMs: preparation.expiresAtMs });
  else if (snapshot.phase === "hostPreparing" && snapshot.expiresAtMs !== null)
    result.push({ key: "preparation", label: "Preparação", expiresAtMs: snapshot.expiresAtMs });
  const review = snapshot.review ?? snapshot.hostGroup?.review;
  if (review) result.push({ key: "review", label: "Revisão para autorizar", expiresAtMs: review.expiresAtMs });
  if (snapshot.sessionTiming) {
    result.push({ key: "admission", label: "Prazo para entrar", expiresAtMs: snapshot.sessionTiming.admissionExpiresAtMs });
    result.push({ key: "session", label: "Encerramento da sessão", expiresAtMs: snapshot.sessionTiming.sessionExpiresAtMs });
  } else if (!snapshot.hostGroup) {
    const expiresAtMs = snapshot.review?.invitationExpiresAtMs
      ?? (["starting", "hostListening", "guestConnected"].includes(snapshot.phase) ? snapshot.expiresAtMs : null);
    if (expiresAtMs !== null) result.push({ key: "shared", label: "Convite e conexão", expiresAtMs });
  }
  return result;
}

export function preparationExpired(snapshot: ConnectSnapshot | undefined, nowMs: number): boolean {
  return Boolean(snapshot?.preparation && nowMs >= snapshot.preparation.expiresAtMs);
}
export function reviewExpired(snapshot: ConnectSnapshot | undefined, nowMs: number): boolean {
  return Boolean(snapshot?.review && nowMs >= snapshot.review.expiresAtMs);
}
export function remainingText(expiresAtMs: number, nowMs: number): string {
  const seconds = Math.max(0, Math.ceil((expiresAtMs - nowMs) / 1_000));
  if (seconds === 0) return "Prazo vencido";
  const hours = Math.floor(seconds / 3_600);
  const minutes = Math.floor(seconds % 3_600 / 60);
  const rest = seconds % 60;
  return `Restam ${hours ? `${hours}h ` : ""}${minutes}min ${String(rest).padStart(2, "0")}s`;
}

export function sessionGuidance({ snapshot, selectedRole, capabilityAvailable, pending, uncertain, stale, nowMs }: {
  snapshot: ConnectSnapshot | undefined; selectedRole: ConnectRole; capabilityAvailable: boolean; pending: boolean; uncertain: boolean; stale: boolean; nowMs: number;
}): ConnectGuidance {
  if (!capabilityAvailable) {
    const released = snapshot && (snapshot.phase === "idle" || finished(snapshot) && snapshot.cleanupState === "complete");
    return released ? {
      action: "Reabra o aplicativo para verificar o Connect.",
      detail: "A disponibilidade do Connect não pôde ser confirmada. Atualizar estado consulta somente a tentativa; não verifica novamente a disponibilidade do recurso.",
    } : {
      action: pending ? "Aguarde a operação em andamento." : "Acompanhe ou encerre a tentativa identificada.",
      detail: "A disponibilidade do Connect não pôde ser confirmada. Atualizar estado consulta somente a tentativa. Use Encerrar Connect para liberar os recursos; após o encerramento confirmado, reabra o aplicativo para verificar a disponibilidade do recurso.",
    };
  }
  if (uncertain || stale) return {
    action: "Atualize o estado da tentativa.",
    detail: "O último resultado ainda não está confirmado. Use Atualizar estado antes de seguir; Encerrar Connect continua disponível quando há uma tentativa identificada.",
  };
  if (pending || snapshot && ["hostPreparing", "starting", "closing"].includes(snapshot.phase)) return {
    action: "Aguarde a operação em andamento.",
    detail: "O launcher mostrará o próximo passo quando receber o estado. Você pode encerrar uma tentativa identificada enquanto aguarda.",
  };
  if (!snapshot) return {
    action: "Atualize o estado da tentativa.", detail: "Ainda não há uma observação confirmada. Use Atualizar estado antes de seguir.",
  };
  if (snapshot.cleanupState === "failed" || snapshot.cleanupState === "pending" && finished(snapshot)) return {
    action: "Atualize o estado do encerramento.", detail: "A liberação dos recursos ainda não foi confirmada. Aguarde essa confirmação antes de preparar outra tentativa.",
  };
  if (finished(snapshot)) return {
    action: selectedRole === "host" ? "Prepare o anfitrião." : "Prepare o convidado.",
    detail: selectedRole === "host" ? "Confira o servidor que deseja compartilhar e use Preparar anfitrião. O jogo deve estar aberto e sob seu controle."
      : "Receba o código público diretamente do anfitrião, preencha o campo abaixo e use Preparar convidado.",
  };
  if (snapshot.hostGroup || snapshot.pendingGroupSource || snapshot.invitationProfile && snapshot.invitationProfile !== "sessionGameTcp") return {
    action: "Siga a revisão e os controles deste perfil.",
    detail: "Os perfis de grupo e TCP com UDP mantêm as etapas e os prazos próprios abaixo. Acompanhe o estado observado; ele não confirma entrada no mundo ou áudio.",
  };
  if (preparationExpired(snapshot, nowMs) || reviewExpired(snapshot, nowMs)) return {
    action: "Encerre esta tentativa e prepare uma nova.",
    detail: "O prazo da preparação ou da revisão venceu no relógio deste computador. O launcher não enviará uma autorização vencida; o estado final continua sendo confirmado pelo Connect.",
  };
  if (snapshot.phase === "hostPrepared") return {
    action: "Troque os códigos públicos com o convidado.",
    detail: "Use Copiar meu código para enviá-lo. Receba o código gerado pelo convidado, cole no campo abaixo e use Revisar convite.",
  };
  if (snapshot.phase === "guestPrepared") return {
    action: "Envie seu código público ao anfitrião.",
    detail: "Use Copiar meu código. Quando receber o convite destinado a este código, cole em Convite recebido e use Revisar convite.",
  };
  if (snapshot.phase === "reviewed") return {
    action: "Confira a revisão e autorize a conexão.",
    detail: "Leia o participante, os destinos e os prazos abaixo. Se estiverem corretos, marque a confirmação e use Autorizar conexão. Cancelar tentativa encerra este preparo.",
  };
  const timing = snapshot.sessionTiming;
  const sessionEnd = timing?.sessionExpiresAtMs ?? snapshot.expiresAtMs;
  if (sessionEnd !== null && sessionEnd !== undefined && nowMs >= sessionEnd) return {
    action: "Atualize o estado da tentativa.",
    detail: "O prazo da conexão venceu no relógio deste computador. Aguarde a confirmação do encerramento ou use Encerrar Connect; nenhum convite será renovado.",
  };
  if (snapshot.phase === "hostListening") {
    if (snapshot.transportState === "admitted") return {
      action: "Acompanhe o estado da conexão.",
      detail: "O Connect observou a admissão do convidado. Mantenha o servidor aberto enquanto desejar compartilhar; isso não confirma entrada no mundo.",
    };
    if (timing && nowMs >= timing.admissionExpiresAtMs) return {
      action: "Atualize o estado da tentativa.",
      detail: "O prazo para entrar venceu. O convite não será exportado novamente. O Connect confirma se houve admissão e quando os recursos foram encerrados.",
    };
    return {
      action: "Copie e envie o convite ao convidado.",
      detail: "Use Copiar convite abaixo e envie pelo canal combinado. Se já enviou, aguarde a admissão observada; nenhuma cópia ou novo envio acontece automaticamente.",
    };
  }
  if (snapshot.phase === "guestConnected" && snapshot.tcpAddress) return {
    action: "Copie o endereço do jogo para o Minecraft.",
    detail: "Use Copiar endereço do jogo e, no Minecraft compatível, abra Multiplayer → Conexão direta e cole o endereço. A ponte disponível ainda não confirma entrada no mundo ou áudio.",
  };
  return { action: "Atualize o estado da tentativa.", detail: "Ainda não há um próximo passo confirmado. Acompanhe o estado observado abaixo." };
}
