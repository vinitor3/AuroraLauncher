import { useEffect, useRef, useState } from "react";
import DOMPurify from "dompurify";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { FirebaseServices } from "../../lib/firebase";
import { translateProjectSegments } from "../../lib/edge";
import { prepareProjectTranslation, secureProjectLinks, translationBatches, validateProjectTranslations } from "../../lib/projectTranslation";
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
  const [translation, setTranslation] = useState<{ source: string; html: string }>();
  const [showTranslation, setShowTranslation] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const source = JSON.stringify([summary, body, bodyFormat]);
  const translated = showTranslation && translation?.source === source;

  useEffect(() => {
    setTranslation(undefined);
    setShowTranslation(false);
    setPending(false);
    setError("");
    return () => { active.current?.abort(); active.current = null; };
  }, [source]);

  async function toggleTranslation() {
    if (active.current) {
      active.current.abort();
      active.current = null;
      setPending(false);
      return;
    }
    if (translated) { setShowTranslation(false); return; }
    if (translation?.source === source) { setShowTranslation(true); return; }
    if (!services) { setError("Entre na sua conta Aurora para traduzir esta página."); return; }
    if (!original.current) return;
    const controller = new AbortController();
    active.current = controller;
    setPending(true);
    setError("");
    try {
      const plan = prepareProjectTranslation(original.current);
      const translations: string[] = [];
      for (const batch of translationBatches(plan.segments)) {
        controller.signal.throwIfAborted();
        const result = await translateProjectSegments(services, batch, controller.signal);
        translations.push(...validateProjectTranslations(result, batch));
      }
      if (controller.signal.aborted || active.current !== controller) return;
      setTranslation({ source, html: sanitizeProjectDescription(plan.apply(translations)) });
      setShowTranslation(true);
    } catch (cause) {
      if (!controller.signal.aborted && active.current === controller) {
        setError(cause instanceof Error ? cause.message : "Não foi possível traduzir. O texto original foi mantido.");
      }
    } finally {
      if (active.current === controller) { active.current = null; setPending(false); }
    }
  }

  const label = pending ? "Cancelar tradução" : translated ? "Mostrar texto original" : "Traduzir página para português";
  return <section className="project-translation" aria-label="Descrição do projeto">
    <div className="project-translation-toolbar">
      <span aria-live="polite" role="status">{pending ? "Traduzindo página…" : translated ? "Tradução automática · Português" : ""}</span>
      <button aria-label={label} aria-pressed={Boolean(translated)} className={`project-translate-button${pending ? " translating" : ""}`} disabled={loading} onClick={() => void toggleTranslation()} title={label} type="button">
        <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><path d="M3 5h12M9 3v2M5 8c1.1 3.8 3.8 6.6 7 8M12 5c-.7 5-3.6 8.7-8 11M13 21l4-10 4 10M14.4 17.5h5.2" /></svg>
      </button>
    </div>
    {error && <p className="project-translation-error" role="alert">{error}</p>}
    {translated ? <div className="project-description-content" dangerouslySetInnerHTML={{ __html: translation!.html }} lang="pt-BR" /> : <div className="project-description-content" ref={original}>
      <div className="project-detail-summary"><strong data-translation-preserve>Sobre este projeto</strong><p>{summary}</p></div>
      {loading ? <div className="detail-loading">Carregando página completa…</div> : bodyFormat === "markdown" ?
        <article className="project-description markdown-body"><ReactMarkdown remarkPlugins={[remarkGfm]} components={{ a: (properties) => <a {...properties} rel="noopener noreferrer" target="_blank" />, img: (properties) => <img {...properties} loading="lazy" /> }}>{body}</ReactMarkdown></article> :
        <article className="project-description" dangerouslySetInnerHTML={{ __html: sanitizeProjectDescription(body) }} />}
    </div>}
  </section>;
}
