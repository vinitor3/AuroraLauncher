import { useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { readSupportFile, supportExcerpt, type SupportSource } from "./diagnosticContext";
import "./support.css";

type Props = {
  source?: SupportSource;
  context?: string;
  log?: string;
  label?: string;
  className?: string;
};

function SupportDialog({ context = "", log = "", close }: Props & { close: () => void }) {
  const titleId = useId();
  const [text, setText] = useState(() => supportExcerpt([context, log].filter(Boolean).join("\n")));
  const [status, setStatus] = useState("");
  const dialog = useRef<HTMLDialogElement>(null);
  const preview = useRef<HTMLTextAreaElement>(null);
  const [manualCopyAttempt, setManualCopyAttempt] = useState(0);
  const generation = useRef(0);
  useEffect(() => {
    const modal = dialog.current;
    modal?.showModal();
    return () => { generation.current++; if (modal?.open) modal.close(); };
  }, []);

  useEffect(() => {
    if (status === "Use Ctrl+C para copiar o texto selecionado.") {
      preview.current?.focus();
      preview.current?.select();
    }
  }, [status, manualCopyAttempt]);

  async function selectFile(file: File) {
    const current = ++generation.current;
    setText("");
    setStatus("Lendo arquivo…");
    try {
      const result = await readSupportFile(file);
      if (current !== generation.current) return;
      setText(result);
      setStatus(result ? "" : "O arquivo está vazio ou contém somente dados filtrados.");
    } catch (error) {
      if (current === generation.current) setStatus(error instanceof Error ? error.message : "Não foi possível ler o arquivo.");
    }
  }
  async function copy() {
    const filtered = supportExcerpt(text);
    setText(filtered);
    if (!filtered) { setStatus("O texto contém somente dados filtrados."); return; }
    try {
      await navigator.clipboard.writeText(filtered);
      setStatus("Texto copiado.");
    } catch {
      setStatus("Use Ctrl+C para copiar o texto selecionado.");
      setManualCopyAttempt(attempt => attempt + 1);
    }
  }

  return createPortal(<dialog ref={dialog} className="support-dialog" aria-labelledby={titleId}
    onCancel={event => { event.preventDefault(); close(); }} onKeyDown={event => event.stopPropagation()}>
    <header><h2 id={titleId}>Copiar log ou crash</h2><button className="secondary compact" type="button" onClick={close}>Fechar</button></header>
    <label>Arquivo do jogo, servidor ou launcher<input type="file" accept=".txt,.log,.crash" onChange={event => {
      const file = event.currentTarget.files?.[0]; event.currentTarget.value = ""; if (file) void selectFile(file);
    }} /></label>
    <label>Texto para copiar<textarea ref={preview} value={text} rows={8} spellCheck={false} maxLength={131072}
      placeholder="Escolha um arquivo ou cole o erro aqui." onChange={event => { generation.current++; setText(event.target.value); setStatus(""); }} /></label>
    <p>O arquivo fica neste computador. Confira nomes e outros dados pessoais antes de compartilhar.</p>
    <footer><button type="button" disabled={!text.trim()} onClick={() => void copy()}>Copiar texto</button><span role="status">{status}</span></footer>
  </dialog>, document.body);
}

export function SupportContextButton({ label, className = "secondary compact", ...data }: Props) {
  const [open, setOpen] = useState(false);
  const [status, setStatus] = useState("");
  async function copy() {
    if (!data.context && !data.log) { setOpen(true); return; }
    const text = supportExcerpt([data.context, data.log].filter(Boolean).join("\n"));
    if (!text) { setOpen(true); return; }
    try { await navigator.clipboard.writeText(text); setStatus("Copiado."); }
    catch { setOpen(true); }
  }
  return <><button type="button" className={className} onClick={() => void copy()}>{label ?? (data.log ? "Copiar log" : "Copiar erro")}</button>
    {status && <small role="status">{status}</small>}
    {open && <SupportDialog {...data} close={() => setOpen(false)} />}</>;
}
