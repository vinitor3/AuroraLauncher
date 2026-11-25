import DOMPurify from "dompurify";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

type Props = {
  summary: string;
  body: string;
  bodyFormat: "markdown" | "html";
  loading: boolean;
};

function sanitizeProjectDescription(html: string) {
  const container = document.createElement("div");
  container.innerHTML = DOMPurify.sanitize(html);
  for (const link of container.querySelectorAll("a[href]")) {
    link.setAttribute("target", "_blank");
    link.setAttribute("rel", "noopener noreferrer");
  }
  return container.innerHTML;
}

export function ProjectDescription({ summary, body, bodyFormat, loading }: Props) {
  return <section className="project-description-content" aria-label="Descrição do projeto">
      <div className="project-detail-summary"><strong>Sobre este projeto</strong><p>{summary}</p></div>
      {loading ? <div className="detail-loading">Carregando página completa…</div> : bodyFormat === "markdown" ?
        <article className="project-description markdown-body"><ReactMarkdown remarkPlugins={[remarkGfm]} components={{ a: (properties) => <a {...properties} rel="noopener noreferrer" target="_blank" />, img: (properties) => <img {...properties} loading="lazy" /> }}>{body}</ReactMarkdown></article> :
        <article className="project-description" dangerouslySetInnerHTML={{ __html: sanitizeProjectDescription(body) }} />}
  </section>;
}
