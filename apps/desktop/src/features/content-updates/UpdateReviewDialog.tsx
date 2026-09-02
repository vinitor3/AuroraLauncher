import { useEffect } from "react";
import {
  currentVersionLabel,
  modpackCurrentVersionLabel,
  modpackTargetVersionLabel,
  targetVersionLabel,
  type UpdateReviewRequest,
} from "./types";

type UpdateReviewDialogProps = {
  request: UpdateReviewRequest;
  instanceName: string;
  minecraftVersion: string;
  loader?: string;
  busy: boolean;
  error?: string;
  onCancel: () => void;
  onConfirm: () => void;
};

function reviewTitle(request: UpdateReviewRequest) {
  if (request.kind === "modpack") return "Revisar atualização do modpack";
  if (request.kind === "rollback") return "Desfazer última atualização";
  return request.items.length === 1
    ? "Revisar atualização"
    : `Revisar ${request.items.length} atualizações`;
}

function confirmationLabel(request: UpdateReviewRequest, busy: boolean) {
  if (request.kind === "rollback") return busy ? "Restaurando…" : "Desfazer atualização";
  if (request.kind === "modpack") return busy ? "Atualizando modpack…" : "Confirmar atualização";
  return busy ? "Atualizando…" : `Confirmar ${request.items.length === 1 ? "atualização" : `${request.items.length} atualizações`}`;
}

export function UpdateReviewDialog({
  request,
  instanceName,
  minecraftVersion,
  loader,
  busy,
  error,
  onCancel,
  onConfirm,
}: UpdateReviewDialogProps) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) {
        event.preventDefault();
        onCancel();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [busy, onCancel]);

  const descriptionId = "update-review-description";
  return (
    <section
      aria-describedby={descriptionId}
      aria-labelledby="update-review-title"
      aria-modal="true"
      className="update-review-dialog"
      role="dialog"
    >
      <header className="update-review-header">
        <div>
          <p className="eyebrow">ALTERAÇÃO PROTEGIDA</p>
          <h2 id="update-review-title">{reviewTitle(request)}</h2>
        </div>
        <button aria-label="Fechar revisão" className="editor-close" disabled={busy} onClick={onCancel} type="button">×</button>
      </header>

      <div className="update-review-body">
        <div className="update-safety-note" id={descriptionId}>
          <span aria-hidden="true">◇</span>
          <div>
            <strong>{request.kind === "rollback" ? "O Aurora restaurará o snapshot anterior" : "Compatibilidade preservada"}</strong>
            <p>
              {request.kind === "rollback"
                ? `Os arquivos da última atualização de “${instanceName}” serão restaurados. O jogo deve permanecer fechado.`
                : `Somente versões compatíveis com Minecraft ${minecraftVersion}${loader ? ` e ${loader}` : ""} serão aplicadas. O Aurora cria um backup antes de trocar qualquer arquivo.`}
            </p>
          </div>
        </div>

        {request.kind === "content" && (
          <div className="update-review-list">
            {request.items.map((item) => (
              <article className="update-review-item" key={`${item.contentType ?? "content"}-${item.filename}`}>
                <div>
                  <strong>{item.title || item.filename}</strong>
                  <small title={item.filename}>{item.filename}</small>
                </div>
                <span>{currentVersionLabel(item)}</span>
                <b aria-hidden="true">→</b>
                <span className="target">{targetVersionLabel(item)}</span>
                <em>{item.enabled ? "Continuará ativo" : "Continuará pausado"}</em>
              </article>
            ))}
          </div>
        )}

        {request.kind === "modpack" && (
          <article className="modpack-review-card">
            <div className="modpack-review-mark" aria-hidden="true">◈</div>
            <div>
              <strong>{request.modpack.name || (request.modpack.projectId ? `Modpack ${request.modpack.projectId}` : "Modpack desta instância")}</strong>
              <small>{request.modpack.source === "curseforge" ? "CurseForge" : request.modpack.source === "modrinth" ? "Modrinth" : "Origem não identificada"}</small>
            </div>
            <p><span>{modpackCurrentVersionLabel(request.modpack)}</span><b>→</b><span>{modpackTargetVersionLabel(request.modpack)}</span></p>
            <small className="modpack-review-warning">Arquivos e configurações do pacote serão revisados pelo plano transacional antes da troca.</small>
          </article>
        )}

        {request.kind === "rollback" && (
          <div className="rollback-review-copy">
            <strong>O conteúdo atual será substituído pela cópia de segurança mais recente.</strong>
            <p>Use esta opção se uma atualização causou incompatibilidade. Nenhum arquivo fora desta instância será alterado.</p>
          </div>
        )}

        {error && <p aria-live="assertive" className="update-review-error">{error}</p>}
      </div>

      <footer className="update-review-actions">
        <button autoFocus className="secondary" disabled={busy} onClick={onCancel} type="button">Cancelar</button>
        <button disabled={busy} onClick={onConfirm} type="button">{confirmationLabel(request, busy)}</button>
      </footer>
    </section>
  );
}
