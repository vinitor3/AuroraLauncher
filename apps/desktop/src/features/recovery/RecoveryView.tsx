import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { RECOVERY_EXCLUDED, RECOVERY_SCOPE, recoveryBytes, recoveryDate, recoveryError, type RecoveryOverview, type RecoveryPoint, type RecoveryPreview, type RecoveryResult, type RecoveryUnavailablePoint } from "./types";
import "./recovery.css";

type Props = {
  instanceId: string;
  busy: boolean;
  running: boolean;
  onOperationChange: (busy: boolean) => void;
  onChanged: () => Promise<void>;
  initialPointId?: string;
  onInitialPointConsumed?: () => void;
};

type Operation = "list" | "create" | "details" | "preview" | "restore" | "delete";
type Review = { kind: "restore" | "undo" | "apply"; preview: RecoveryPreview } | { kind: "delete"; point: RecoveryPoint | RecoveryUnavailablePoint };
const operationLabels: Record<Operation, string> = {
  list: "Verificando pontos e operações interrompidas…",
  create: "Copiando e verificando os arquivos protegidos…",
  details: "Verificando os detalhes do ponto…",
  preview: "Comparando o ponto com o estado atual…",
  restore: "Protegendo o estado anterior e restaurando os arquivos…",
  delete: "Excluindo o ponto de restauração…",
};
const actionLabels = { add: "Adicionar", replace: "Substituir", remove: "Remover" };

function Scope({ scope = RECOVERY_SCOPE, excluded = RECOVERY_EXCLUDED }: { scope?: string[]; excluded?: string[] }) {
  const [requestedPage, setPage] = useState(0);
  const pageCount = Math.max(1, Math.ceil(excluded.length / 100));
  const page = Math.min(requestedPage, pageCount - 1);
  return <details className="recovery-scope"><summary>Escopo protegido e exclusões</summary>
    <div className="recovery-scope-columns"><div><h4>Incluído</h4><ul>{scope.map((item) => <li key={item}>{item}</li>)}</ul></div>
      <div><h4>Fora do ponto ({excluded.length.toLocaleString("pt-BR")})</h4><ul>{excluded.slice(page * 100, (page + 1) * 100).map((item) => <li key={item}>{item}</li>)}</ul>
        {pageCount > 1 && <nav aria-label="Páginas das exclusões" className="recovery-pagination"><span>Página {page + 1} de {pageCount}</span><button className="secondary compact" disabled={page === 0} onClick={() => setPage(page - 1)} type="button">Exclusões anteriores</button><button className="secondary compact" disabled={page + 1 >= pageCount} onClick={() => setPage(page + 1)} type="button">Próximas exclusões</button></nav>}
      </div></div>
    <p>Somente os campos permitidos dos perfis são recuperados. A identidade da instância e seus vínculos internos são preservados.</p>
  </details>;
}

function PointMeta({ point }: { point: RecoveryPoint }) {
  return <dl className="recovery-meta">
    <div><dt>Criado em</dt><dd>{recoveryDate(point.createdAt)}</dd></div>
    <div><dt>Tamanho lógico</dt><dd>{recoveryBytes(point.logicalBytes)}</dd></div>
    <div><dt>Arquivos protegidos</dt><dd>{point.fileCount.toLocaleString("pt-BR")}</dd></div>
  </dl>;
}

function FileTable({ files }: { files: RecoveryPoint["files"] | RecoveryPreview["changes"] }) {
  const [requestedPage, setPage] = useState(0);
  const pageSize = 100;
  const pageCount = Math.max(1, Math.ceil(files.length / pageSize));
  const page = Math.min(requestedPage, pageCount - 1);
  const start = page * pageSize;
  const changes = files.length > 0 && "action" in files[0];
  return <>
    <div className="recovery-table-scroll"><table><thead><tr>{changes ? <><th>Ação</th><th>Arquivo da instância</th></> : <><th>Arquivo</th><th>Tamanho</th><th>SHA-256</th></>}</tr></thead><tbody>{files.slice(start, start + pageSize).map((file) => <tr key={file.path}>{"action" in file ? <><td className={`recovery-action-${file.action}`}>{actionLabels[file.action]}</td><td><code>{file.path}</code></td></> : <><td><code>{file.path}</code></td><td>{recoveryBytes(file.size)}</td><td className="recovery-hash"><code>{file.sha256}</code></td></>}</tr>)}</tbody></table></div>
    {pageCount > 1 && <nav aria-label="Páginas dos arquivos" className="recovery-pagination"><span>{(start + 1).toLocaleString("pt-BR")}–{Math.min(start + pageSize, files.length).toLocaleString("pt-BR")} de {files.length.toLocaleString("pt-BR")} arquivos</span><button className="secondary compact" disabled={page === 0} onClick={() => setPage(page - 1)} type="button">Arquivos anteriores</button><button className="secondary compact" disabled={page + 1 >= pageCount} onClick={() => setPage(page + 1)} type="button">Próximos arquivos</button></nav>}
  </>;
}

export function RecoveryReviewDialog({ review, busy, blocked, onCancel, onConfirm }: {
  review: Review; busy: boolean; blocked: boolean; onCancel: () => void; onConfirm: () => void;
}) {
  const dialog = useRef<HTMLElement>(null);
  const cancel = useRef(onCancel);
  cancel.current = onCancel;
  const isBusy = useRef(busy);
  isBusy.current = busy;
  const [acknowledged, setAcknowledged] = useState(false);
  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    dialog.current?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault(); event.stopPropagation();
        if (!isBusy.current) cancel.current();
      }
      if (event.key === "Tab") {
        const controls = [...(dialog.current?.querySelectorAll<HTMLElement>('button:not([disabled]), input:not([disabled]), summary, [tabindex="0"]') ?? [])];
        const first = controls[0]; const last = controls[controls.length - 1];
        if (!first) { event.preventDefault(); dialog.current?.focus(); }
        else if (event.shiftKey && (document.activeElement === first || !controls.includes(document.activeElement as HTMLElement))) { event.preventDefault(); last.focus(); }
        else if (!event.shiftKey && (document.activeElement === last || !controls.includes(document.activeElement as HTMLElement))) { event.preventDefault(); first.focus(); }
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => { window.removeEventListener("keydown", onKeyDown, true); if (previous?.isConnected) previous.focus(); };
  }, []);
  const deleting = review.kind === "delete";
  const applying = review.kind === "apply";
  const point = review.kind === "delete" ? review.point : review.preview.point;
  const title = deleting ? "Excluir ponto de restauração" : applying ? "Revisar alterações para a instância original" : review.kind === "undo" ? "Revisar desfazer restauração" : "Revisar restauração";
  return <div className="recovery-review-overlay"><section aria-labelledby="recovery-review-title" aria-describedby="recovery-review-description" aria-modal="true" aria-busy={busy} className="recovery-review" ref={dialog} role="dialog" tabIndex={-1}>
    <header><div><p className="eyebrow">RECUPERAÇÃO DA INSTÂNCIA</p><h2 id="recovery-review-title">{title}</h2></div><button aria-label="Fechar revisão de recuperação" className="editor-close" disabled={busy} onClick={onCancel} type="button">×</button></header>
    <div className="recovery-review-body">
      <strong className="recovery-point-description">{"description" in point ? point.description || "Ponto sem descrição" : "Ponto indisponível"}</strong>{"logicalBytes" in point ? <PointMeta point={point} /> : <p>Criado em {recoveryDate(point.createdAt)}. O tamanho e o inventário não puderam ser verificados.</p>}
      {deleting ? <p id="recovery-review-description">Este ponto deixará de estar disponível. Os arquivos atuais da instância e os dados usados por outros pontos serão preservados. O espaço compartilhado pode continuar ocupado após a exclusão.</p>
        : <>
          <p id="recovery-review-description">{applying ? "Somente as mudanças selecionadas abaixo serão transferidas da cópia de teste para a instância original. A origem será verificada novamente e conflitos bloquearão a aplicação." : review.kind === "undo" ? "O estado protegido anterior à última restauração será recuperado. Alterações feitas depois também entram na comparação abaixo." : "Os arquivos dentro do escopo voltarão ao estado salvo. Arquivos adicionados depois do ponto serão removidos conforme a lista abaixo."}</p>
          <div className="recovery-worlds-warning" role="note"><strong>Mundos não serão restaurados.</strong><span>Os mundos e demais caminhos excluídos permanecem intactos.</span></div>
          <Scope scope={review.preview.point.scope} excluded={review.preview.excluded} />
          <div className="recovery-change-counts" aria-label="Resumo das alterações">{(["add", "replace", "remove"] as const).map((action) => <span className={`recovery-action-${action}`} key={action}>{review.preview.changes.filter((change) => change.action === action).length.toLocaleString("pt-BR")} · {actionLabels[action]}</span>)}</div>
          <details className="recovery-file-review" open><summary>Arquivos que serão alterados ({review.preview.changes.length.toLocaleString("pt-BR")})</summary>
            {review.preview.changes.length ? <FileTable files={review.preview.changes} /> : <p>Nenhuma diferença nos arquivos protegidos.</p>}
          </details>
          <details className="recovery-file-review" open><summary>Configurações do perfil ({review.preview.profileChanges.length})</summary>
            {review.preview.profileChanges.length ? <div className="recovery-table-scroll"><table><thead><tr><th>Campo</th><th>Atual</th><th>Após restaurar</th></tr></thead><tbody>{review.preview.profileChanges.map((change) => <tr key={`${change.file}:${change.field}`}><td><code>{change.file} · {change.field}</code></td><td>{change.before ?? "Não definido"}</td><td>{change.after ?? "Não definido"}</td></tr>)}</tbody></table></div> : <p>Nenhuma alteração nos campos protegidos do perfil.</p>}
          </details>
          <p className="recovery-managed-note">Módulos Aurora administrados ficam fora da recuperação: Companion não é reinstalado e Core/Skins não são substituídos por versões do ponto antigo. A matriz ativa continua sendo respeitada.</p>
          <p>Antes de alterar a instância, o Aurora verificará novamente os arquivos e protegerá o estado atual para permitir desfazer. Se o estado mudar ou faltar espaço para essa proteção, a operação será bloqueada.</p>
          <label className="recovery-acknowledge"><input checked={acknowledged} disabled={busy} onChange={(event) => setAcknowledged(event.target.checked)} type="checkbox" /><span>{applying ? "Revisei as mudanças selecionadas e autorizo aplicá-las à instância original com proteção para desfazer. Mundos não serão transferidos." : "Revisei as alterações e entendo que mundos não serão restaurados."}</span></label>
        </>}
    </div>
    <footer>{busy && <p role="status">{deleting ? "Excluindo ponto…" : applying ? "Protegendo a origem e aplicando mudanças selecionadas…" : "Restaurando com proteção do estado anterior…"} Aguarde a conclusão.</p>}<div><button className="secondary" disabled={busy} onClick={onCancel} type="button">Cancelar</button><button className={deleting ? "danger" : ""} disabled={busy || blocked || (!deleting && !acknowledged)} onClick={onConfirm} type="button">{deleting ? "Confirmar exclusão" : applying ? "Confirmar aplicação à original" : review.kind === "undo" ? "Confirmar desfazer restauração" : "Confirmar restauração"}</button></div></footer>
  </section></div>;
}

export function RecoveryView({ instanceId, busy, running, onOperationChange, onChanged, initialPointId, onInitialPointConsumed }: Props) {
  const [overview, setOverview] = useState<RecoveryOverview>();
  const [operation, setOperation] = useState<Operation>();
  const [description, setDescription] = useState("");
  const [creating, setCreating] = useState(false);
  const [selected, setSelected] = useState<RecoveryPoint>();
  const [review, setReview] = useState<Review>();
  const [message, setMessage] = useState("");
  const [warning, setWarning] = useState("");
  const [recoveredInterrupted, setRecoveredInterrupted] = useState(false);
  const [error, setError] = useState("");
  const generation = useRef(0);
  const inFlight = useRef(false);
  const previewEpoch = useRef(0);
  const context = useRef(instanceId);
  context.current = instanceId;
  const callbacks = useRef({ onOperationChange, onChanged });
  callbacks.current = { onOperationChange, onChanged };
  const disabled = busy || running || !!operation;

  async function perform<T,>(kind: Operation, command: string, args: Record<string, unknown>, accept: (value: T, current: () => boolean) => void | Promise<void>) {
    if (inFlight.current) return;
    const request = ++generation.current;
    const instance = instanceId;
    const current = () => generation.current === request && context.current === instance;
    inFlight.current = true;
    setOperation(kind); setError(""); setMessage(""); setWarning("");
    callbacks.current.onOperationChange(true);
    try {
      const result = await invoke<T>(command, { id: instance, ...args });
      if (current()) await accept(result, current);
    } catch (cause) {
      if (current()) { setError(recoveryError(cause)); setReview(undefined); }
    } finally {
      if (current()) { inFlight.current = false; setOperation(undefined); callbacks.current.onOperationChange(false); }
    }
  }

  const reload = () => perform<RecoveryOverview>("list", "recovery_list", {}, async (result) => {
    setOverview(result);
    await callbacks.current.onChanged().catch(() => undefined);
  });
  useEffect(() => {
    setOverview(undefined); setSelected(undefined); setReview(undefined); setCreating(false); setDescription("");
    setRecoveredInterrupted(false); setOperation("list");
    inFlight.current = false;
    callbacks.current.onOperationChange(true);
    let disposed = false;
    let stopListening: (() => void) | undefined;
    void listen<{ instanceId: string; recoveredInterrupted?: boolean }>("aurora-recovery-changed", (event) => {
      if (!disposed && event.payload.instanceId === instanceId && event.payload.recoveredInterrupted) setRecoveredInterrupted(true);
    }).then((stop) => { if (disposed) stop(); else stopListening = stop; }).catch(() => undefined).finally(() => { if (!disposed) void reload(); });
    return () => { disposed = true; stopListening?.(); generation.current += 1; inFlight.current = false; callbacks.current.onOperationChange(false); };
  }, [instanceId]);

  useEffect(() => {
    let disposed = false;
    const stops: Array<() => void> = [];
    for (const event of ["aurora-recovery-changed", "aurora-doctor-run", "aurora-ai-tool-changed", "aurora-diagnostic-lab-changed"]) {
      void listen<{ instanceId: string }>(event, ({ payload }) => {
        if (!disposed && payload.instanceId === instanceId) { previewEpoch.current += 1; setReview(undefined); }
      }).then((stop) => { if (disposed) stop(); else stops.push(stop); }).catch(() => undefined);
    }
    return () => { disposed = true; stops.forEach((stop) => stop()); };
  }, [instanceId]);

  useEffect(() => {
    if (!initialPointId || !overview || disabled) return;
    const point = overview.points.find((item) => item.id === initialPointId);
    onInitialPointConsumed?.();
    if (point) preview(point);
    else setError("O ponto indicado pelo Doctor não está mais disponível. Atualize a lista e escolha um ponto verificado.");
  }, [initialPointId, overview, disabled]);

  async function refreshOverview(current: () => boolean) {
    try {
      const result = await invoke<RecoveryOverview>("recovery_list", { id: instanceId });
      if (current()) setOverview(result);
    } catch {
      if (current()) setWarning("A operação foi concluída, mas a lista não pôde ser atualizada. Clique em Atualizar lista para conferir os pontos disponíveis.");
    }
  }

  function createPoint() {
    if (disabled) return;
    void perform<RecoveryPoint>("create", "recovery_create", { description: description.trim() || null }, async (point, current) => {
      setCreating(false); setDescription(""); setSelected(point);
      setMessage("Ponto de restauração criado e verificado. Mundos não foram incluídos.");
      await refreshOverview(current);
    });
  }

  function preview(point?: RecoveryPoint) {
    if (disabled) return;
    const epoch = previewEpoch.current;
    void perform<RecoveryPreview>("preview", point ? "recovery_preview" : "recovery_preview_undo", point ? { pointId: point.id } : {}, (result) => {
      if (previewEpoch.current === epoch) setReview({ kind: point ? "restore" : "undo", preview: result });
      else setError("A prévia ficou desatualizada. Gere uma nova prévia antes de confirmar.");
    });
  }

  function confirm() {
    if (!review || disabled) return;
    if (review.kind === "delete") {
      void perform<void>("delete", "recovery_delete", { pointId: review.point.id }, async (_, current) => {
        if (selected?.id === review.point.id) setSelected(undefined);
        setReview(undefined);
        setMessage("Ponto excluído. Dados compartilhados com outros pontos foram preservados.");
        await refreshOverview(current);
      });
    } else {
      const kind = review.kind;
      void perform<RecoveryResult>("restore", "recovery_restore", { previewId: review.preview.id }, async (result, current) => {
        setReview(undefined); setSelected(undefined);
        setMessage(`${kind === "undo" ? "Restauração desfeita" : "Estado protegido restaurado"}. Mundos e caminhos excluídos foram preservados.${result.undoAvailable ? " O estado anterior está disponível em “Desfazer restauração”." : ""}`);
        await refreshOverview(current);
        // A successful restore must remain a success if only refreshing the
        // parent editor fails; the next list read retries that projection.
        if (current()) await callbacks.current.onChanged().catch(() => {
          if (current()) setWarning("A operação foi concluída, mas alguns dados da tela não puderam ser atualizados. Clique em Atualizar lista para conferir o estado atual.");
        });
      });
    }
  }

  return <section aria-label="Recuperação da instância" aria-busy={!!operation} className="recovery-view">
    <header className="recovery-heading"><div><p className="eyebrow">RECUPERAÇÃO</p><h2>Pontos de restauração</h2><p>Guarde o estado protegido desta instância e revise as diferenças antes de recuperar.</p></div>
      <button disabled={disabled || creating} onClick={() => { setCreating(true); setError(""); setMessage(""); }} type="button">Criar ponto de restauração</button></header>
    <div className="recovery-worlds-warning" role="note"><strong>Este ponto não inclui mundos.</strong><span>Mundos não serão restaurados. Faça uma cópia separada dos seus mundos quando precisar protegê-los.</span></div>
    <Scope scope={overview?.scope} excluded={overview?.excluded} />
    <p className="recovery-managed-note">Core e Skins administrados pelo Aurora são preservados conforme a matriz ativa. Um ponto antigo não reinstala Companion nem troca esses módulos por artefatos incompatíveis.</p>
    {running && <p className="recovery-message recovery-warning" role="status">Encerre o Minecraft desta instância para criar ou restaurar pontos. Processos externos também são verificados antes da operação.</p>}
    {creating && <form className="recovery-create" onSubmit={(event) => { event.preventDefault(); createPoint(); }}><label>Descrição opcional<input autoFocus disabled={!!operation} maxLength={240} onChange={(event) => setDescription(event.target.value)} placeholder="Ex.: antes de testar novos mods" value={description} /></label><p>Mantenha outros programas sem editar os arquivos durante a captura. O ponto só fica disponível após a verificação.</p><div><button className="secondary" disabled={!!operation} onClick={() => setCreating(false)} type="button">Cancelar</button><button disabled={disabled} type="submit">Criar e verificar ponto</button></div></form>}
    {operation && <div className="recovery-progress" role="status"><span aria-hidden="true" className="recovery-spinner" /><div><strong>{operationLabels[operation]}</strong><p>A interface continua respondendo. Aguarde a conclusão; esta etapa não pode ser cancelada com segurança.</p></div></div>}
    {error && <div className="recovery-message recovery-error" role="alert"><strong>Operação não concluída</strong><p>{error}</p></div>}
    {message && <p className="recovery-message recovery-success" role="status">{message}</p>}
    {recoveredInterrupted && <p className="recovery-message recovery-success" role="status">A operação interrompida foi recuperada com segurança. Confira os arquivos e gere uma nova prévia antes de continuar.</p>}
    {warning && <p className="recovery-message recovery-warning" role="status">{warning}</p>}
    {overview?.undoUnavailable && <p className="recovery-message recovery-warning" role="status">O estado anterior à última restauração está ausente ou não pôde ser verificado. Desfazer está indisponível. Seus pontos manuais íntegros continuam disponíveis.</p>}
    <div className="recovery-list-heading"><h3>{overview ? `${overview.points.length} ${overview.points.length === 1 ? "ponto manual" : "pontos manuais"}` : "Pontos salvos"}</h3><div>{overview?.undoPoint && <button className="secondary compact" disabled={disabled} onClick={() => preview()} type="button">Desfazer restauração</button>}<button className="secondary compact" disabled={!!operation || busy} onClick={() => void reload()} type="button">Atualizar lista</button></div></div>
    {overview && <p className="recovery-storage">Conteúdo físico compartilhado no armazenamento local: <strong>{recoveryBytes(overview.physicalBytes)}</strong>. O tamanho lógico de cada ponto conta seus arquivos completos; conteúdo idêntico é compartilhado. A exclusão de um ponto não garante liberação imediata de espaço.</p>}
    {overview?.points.length === 0 && overview.unavailablePoints.length === 0 && <div className="recovery-empty"><span aria-hidden="true">↶</span><strong>Nenhum ponto manual nesta instância</strong><p>Crie um ponto para guardar mods, configurações e campos protegidos do perfil. Mundos ficam fora desta recuperação.</p></div>}
    {!!overview?.unavailablePoints.length && <div className="recovery-unavailable"><h3>Pontos indisponíveis ({overview.unavailablePoints.length})</h3><p>Alguns pontos não passaram pela verificação. Os pontos íntegros continuam disponíveis abaixo.</p>{overview.unavailablePoints.map((point) => <article className="recovery-point" key={point.id}><div><h3>Ponto indisponível · {recoveryDate(point.createdAt)}</h3><p>Os dados estão ausentes, corrompidos ou não puderam ser validados. O tamanho e o inventário não estão disponíveis.</p></div><div className="recovery-point-actions"><button className="danger compact" disabled={disabled} onClick={() => setReview({ kind: "delete", point })} type="button">Excluir ponto indisponível</button></div></article>)}</div>}
    {overview && <div className="recovery-points">{overview.points.map((point) => <article className={`recovery-point${selected?.id === point.id ? " selected" : ""}`} key={point.id}><div><h3>{point.description || "Ponto sem descrição"}</h3><PointMeta point={point} /><p className="recovery-point-scope">Escopo: {point.scope.join(" · ")}</p></div><div className="recovery-point-actions"><button className="secondary compact" disabled={disabled} onClick={() => void perform<RecoveryPoint>("details", "recovery_details", { pointId: point.id }, (value) => setSelected(value))} type="button">Detalhes</button><button disabled={disabled} onClick={() => preview(point)} type="button">Restaurar</button><button className="danger compact" disabled={disabled} onClick={() => setReview({ kind: "delete", point })} type="button">Excluir</button></div></article>)}</div>}
    {selected && <details className="recovery-details" open><summary>Detalhes · {selected.description || "Ponto sem descrição"}</summary><PointMeta point={selected} /><Scope scope={selected.scope} excluded={selected.excluded} /><h4>Inventário salvo</h4>{selected.files.length ? <FileTable files={selected.files} key={selected.id} /> : <p>Nenhum arquivo no escopo protegido.</p>}<h4>Campos do perfil salvos</h4><div className="recovery-table-scroll"><table><thead><tr><th>Perfil · campo</th><th>Valor</th></tr></thead><tbody>{Object.entries(selected.profile).flatMap(([file, fields]) => Object.entries(fields).map(([field, value]) => <tr key={`${file}:${field}`}><td><code>{file} · {field}</code></td><td>{value ?? "Não definido"}</td></tr>))}</tbody></table></div></details>}
    {review && <RecoveryReviewDialog blocked={busy || running} busy={!!operation} key={review.kind === "delete" ? review.point.id : review.preview.id} onCancel={() => setReview(undefined)} onConfirm={confirm} review={review} />}
  </section>;
}
