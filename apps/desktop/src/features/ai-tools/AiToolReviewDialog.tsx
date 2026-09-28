import { useEffect, useId, useRef } from "react";
import { createPortal } from "react-dom";
import type { AiToolPlan } from "./types";

type Props = { plan: AiToolPlan; instanceId: string; busy: boolean; blocked?: boolean; error?: string; onCancel: () => void; onConfirm: () => void };

export function AiToolReviewDialog({ plan, instanceId, busy, blocked, error, onCancel, onConfirm }: Props) {
  const titleId = useId();
  const dialogRef = useRef<HTMLElement>(null);
  const cancelButton = useRef<HTMLButtonElement>(null);
  const current = useRef({ busy, onCancel });
  current.current = { busy, onCancel };
  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const keydown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopImmediatePropagation();
        if (!current.current.busy) current.current.onCancel();
      }
      if (event.key === "Tab") {
        const controls = [...(dialogRef.current?.querySelectorAll<HTMLElement>('button:not([disabled]), [tabindex="0"]') ?? [])];
        const first = controls[0];
        const last = controls[controls.length - 1];
        if (!first) { event.preventDefault(); dialogRef.current?.focus(); }
        else if (event.shiftKey && (document.activeElement === first || !dialogRef.current?.contains(document.activeElement))) { event.preventDefault(); last?.focus(); }
        else if (!event.shiftKey && (document.activeElement === last || !dialogRef.current?.contains(document.activeElement))) { event.preventDefault(); first.focus(); }
      }
    };
    window.addEventListener("keydown", keydown, true);
    cancelButton.current?.focus();
    return () => {
      window.removeEventListener("keydown", keydown, true);
      if (previous?.isConnected) previous.focus();
    };
  }, []);

  return createPortal(<div className="ai-tool-modal-backdrop">
    <section ref={dialogRef} role="dialog" aria-modal="true" aria-labelledby={titleId} aria-busy={busy} tabIndex={-1} className="ai-tool-review">
      <header><p className="eyebrow">CONFIRMAÇÃO DE ALTERAÇÃO</p><h2 id={titleId}>{plan.title}</h2><p>Instância: <strong>{instanceId}</strong></p></header>
      <div className="ai-tool-review-body">
        <p>Confira a alteração preparada pelo launcher. Ela só será aplicada ao confirmar.</p>
        <div className="ai-tool-diff">{plan.changes.map((change, index) => <article key={index}><strong>{change.label}</strong><div><span>Antes</span><p>{change.before}</p></div><div><span>Depois</span><p>{change.after}</p></div></article>)}</div>
        {plan.warnings.length > 0 && <ul className="ai-tool-warnings">{plan.warnings.map((warning, index) => <li key={index}>{warning}</li>)}</ul>}
        {error && <p className="ai-tool-error" role="alert">{error}</p>}
        {busy && <p role="status">Aplicando a alteração confirmada…</p>}
        {blocked && !busy && <p role="status">Aguarde a operação atual do launcher antes de confirmar.</p>}
      </div>
      <footer><button ref={cancelButton} className="secondary" disabled={busy} onClick={onCancel} type="button">Cancelar</button><button disabled={busy || blocked} onClick={onConfirm} type="button">{busy ? "Aplicando…" : "Confirmar alteração"}</button></footer>
    </section>
  </div>, document.body);
}
