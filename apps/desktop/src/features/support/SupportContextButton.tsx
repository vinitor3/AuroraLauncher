import { useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { buildSupportPrompt, supportExcerpt, supportSources, type SupportSource } from "./diagnosticContext";
import "./support.css";

type Props = {
  source?: SupportSource;
  context?: string;
  log?: string;
  label?: string;
  className?: string;
};

export function SupportDialog({ source, context = "", log = "", close }: Props & { close: () => void }) {
  const titleId = useId();
  const [topic, setTopic] = useState<SupportSource>(source ?? "Launcher");
  const [sample, setSample] = useState("");
  const [edited, setEdited] = useState<string>();
  const [status, setStatus] = useState("");
  const dialog = useRef<HTMLDialogElement>(null);
  const preview = useRef<HTMLTextAreaElement>(null);
  const prompt = edited ?? buildSupportPrompt(topic, context, log + "\n" + sample);

  useEffect(() => {
    const modal = dialog.current;
    modal?.showModal();
    return () => { if (modal?.open) modal.close(); };
  }, []);


  function includeExcerpt() {
    const excerpt = supportExcerpt(sample.trim() || log);
    if (!excerpt) {
      setStatus("O trecho contém apenas dados filtrados. Você pode descrever o erro na mensagem.");
      return;
    }
    const block = "Trecho do log para análise:\n" + excerpt;
    const next = prompt.includes(block) ? prompt : [prompt, block].join("\n\n");
    if (next.length > 24000) {
      setStatus("Encurte a mensagem antes de incluir o trecho.");
      return;
    }
    setEdited(next);
    setStatus("Confira nomes e dados privados antes de copiar.");
    preview.current?.focus();
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(prompt);
      setStatus("Copiado. Cole a mensagem na IA de sua preferência.");
    } catch {
      preview.current?.focus();
      preview.current?.select();
      setStatus("Use Ctrl+C para copiar o texto selecionado.");
    }
  }

  return createPortal(<dialog ref={dialog} className="support-dialog" aria-labelledby={titleId}
    onCancel={event => { event.preventDefault(); close(); }} onKeyDown={event => event.stopPropagation()}>
    <header><h2 id={titleId}>Preparar mensagem para IA</h2><button className="secondary compact" type="button" onClick={close}>Fechar</button></header>
    {!source && <label>Onde ocorreu?<select value={topic} onChange={event => { setTopic(event.target.value as SupportSource); setStatus(""); }}>
      {supportSources.map(item => <option key={item}>{item}</option>)}</select></label>}
    <label>Trecho de log ou crash (opcional)<textarea value={sample} maxLength={65536} rows={3}
      placeholder="Cole um trecho do log do jogo ou do servidor"
      onChange={event => { setSample(event.target.value); setStatus(""); }} /></label>
    <p>O Aurora extrai os sinais técnicos no seu computador. Revise a mensagem e acrescente o que estava fazendo quando o erro ocorreu.</p>
    {(sample.trim() || log.trim()) && <button className="secondary compact" type="button" onClick={includeExcerpt}>Incluir trecho filtrado na mensagem</button>}
    <label>Mensagem que será copiada<textarea ref={preview} value={prompt} rows={8} spellCheck={false}
      maxLength={24000} onChange={event => { setEdited(event.target.value); setStatus(""); }} /></label>
    {edited !== undefined && <button className="text-button" type="button" onClick={() => { setEdited(undefined); setStatus(""); }}>Recriar mensagem com os dados atuais</button>}
    <footer><button type="button" disabled={!prompt.trim()} onClick={() => void copy()}>Copiar para minha IA</button>
      <span role="status">{status}</span></footer>
  </dialog>, document.body);
}

export function SupportContextButton({ label = "Preparar para IA", className = "secondary compact", ...data }: Props) {
  const [open, setOpen] = useState(false);
  return <><button type="button" className={className} aria-haspopup="dialog" onClick={() => setOpen(true)}>{label}</button>
    {open && <SupportDialog {...data} close={() => setOpen(false)} />}</>;
}
