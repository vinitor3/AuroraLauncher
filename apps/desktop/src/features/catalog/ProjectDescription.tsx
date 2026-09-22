import { useEffect, useRef, useState } from "react";
import DOMPurify from "dompurify";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { FirebaseServices } from "../../lib/firebase";
import { translateProjectSegments } from "../../lib/edge";
import { createProjectTranslationJob, secureProjectLinks, type ProjectTranslationProgress } from "../../lib/projectTranslation";
import "./projectDescription.css";

type Props = {
  services?: FirebaseServices | null;
  summary: string;
  body: string;
  bodyFormat: "markdown" | "html";
  loading: boolean;
};

function sanitizeProjectDescription(html: string) {
  const container = document.createElement("div");
  container.innerHTML = DOMPurify.sanitize(html, { ADD_ATTR: ["target"] });
  secureProjectLinks(container);
  return container.innerHTML;
}

export function ProjectDescription({ services, summary, body, bodyFormat, loading }: Props) {
  const original = useRef<HTMLDivElement>(null);
  const active = useRef<AbortController | null>(null);
  const job = useRef<{ source: string; value: ReturnType<typeof createProjectTranslationJob> } | null>(null);
  const [translation, setTranslation] = useState<ProjectTranslationProgress & { source: string }>();
  const [showTranslation, setShowTranslation] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const source = JSON.stringify([summary, body, bodyFormat]);
  const translated = showTranslation && translation?.source === source;

  useEffect(() => {
    job.current = null;
    setTranslation(undefined);
    setShowTranslation(false);
    setPending(false);
    setError("");
    return () => { active.current?.abort(); active.current = null; };
  }, [source]);

  async function startTranslation(restart = false) {
    if (active.current) return;
    if (!services) { setError("Entre na sua conta Aurora para traduzir esta página."); return; }
    if (!original.current) return;
    const controller = new AbortController();
    active.current = controller;
    setPending(true);
    setError("");
    try {
      if (restart || job.current?.source !== source) {
        job.current = { source, value: createProjectTranslationJob(original.current) };
        if (restart) setTranslation(undefined);
      }
      const publish = (progress: ProjectTranslationProgress) => {
        if (controller.signal.aborted || active.current !== controller) return;
        setTranslation({ ...progress, source, html: sanitizeProjectDescription(progress.html) });
        setShowTranslation(progress.changed);
      };
      publish(await job.current.value.run(
        (segments, signal) => translateProjectSegments(services, segments, signal),
        controller.signal,
        publish,
      ));
    } catch (cause) {
      if (!controller.signal.aborted && active.current === controller) {
        setError(cause instanceof Error ? cause.message : "Não foi possível traduzir este trecho. Tente novamente.");
      }
    } finally {
      if (active.current === controller) { active.current = null; setPending(false); }
    }
  }

  function toggleTranslation() {
    if (active.current) {
      active.current.abort();
      active.current = null;
      setPending(false);
      return;
    }
    if (translated) { setShowTranslation(false); return; }
    if (translation?.source === source && translation.changed) {
      setShowTranslation(true);
      return;
    }
    void startTranslation(translation?.complete);
  }

  const current = translation?.source === source ? translation : undefined;
  const percentage = current?.total ? Math.round(current.completed / current.total * 100) : 0;
  const status = pending ? `Traduzindo para português… ${percentage}%`
    : current && !current.complete ? `Tradução pausada · ${percentage}% · Os trechos restantes estão no idioma original.`
      : current?.complete && !current.changed ? current.total
        ? "Nenhum texto foi alterado. Confira se a página já está em português ou contém apenas nomes."
        : "Não há texto disponível para traduzir. Textos dentro de imagens permanecem no original."
        : translated ? `Tradução automática · Português${job.current?.value.hasImages ? " · Textos dentro de imagens permanecem no original." : ""}` : "";
  const label = pending ? "Cancelar tradução" : translated ? "Mostrar texto original" : "Traduzir página para português";
  return <section className="project-translation" aria-label="Descrição do projeto">
    <div className="project-translation-toolbar">
      <span aria-live="polite" role="status">{status}</span>
      {!pending && current && !current.complete && <button className="project-translation-resume" type="button" disabled={loading} onClick={() => void startTranslation()}>Continuar tradução</button>}
      <button aria-label={label} aria-pressed={Boolean(translated)} className={`project-translate-button${pending ? " translating" : ""}`} disabled={loading} onClick={() => void toggleTranslation()} title={label} type="button">
        <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="M3 5h12M9 3v2M5 8c1.1 3.8 3.8 6.6 7 8M12 5c-.7 5-3.6 8.7-8 11M13 21l4-10 4 10M14.4 17.5h5.2" /></svg>
      </button>
    </div>
    {error && <p className="project-translation-error" role="alert">{error} {current && !current.complete ? "A tradução feita até aqui foi mantida." : "Você pode tentar novamente pelo botão de tradução."}</p>}
    {translated && <div className="project-description-content" dangerouslySetInnerHTML={{ __html: translation!.html }} lang="pt-BR" />}
    <div className="project-description-content" ref={original} hidden={Boolean(translated)}>
      <div className="project-detail-summary"><strong data-translation-preserve>Sobre este projeto</strong><p>{summary}</p></div>
      {loading ? <div className="detail-loading">Carregando página completa…</div> : bodyFormat === "markdown" ?
        <article className="project-description markdown-body"><ReactMarkdown remarkPlugins={[remarkGfm]} components={{ a: (properties) => <a {...properties} rel="noopener noreferrer" target="_blank" />, img: (properties) => <img {...properties} loading="lazy" /> }}>{body}</ReactMarkdown></article> :
        <article className="project-description" dangerouslySetInnerHTML={{ __html: sanitizeProjectDescription(body) }} />}
    </div>
  </section>;
}
