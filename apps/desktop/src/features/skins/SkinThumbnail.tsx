import { useState } from "react";

type Props = { src: string; name: string; className?: string };

function ThumbnailImage({ src, name, className }: Props) {
  const [status, setStatus] = useState<"loading" | "ready" | "error">(src.trim() ? "loading" : "error");
  return (
    <span className={`skin-thumbnail${className ? ` ${className}` : ""}`}>
      {src.trim() && <img
        alt={`Skin ${name}`}
        aria-hidden={status !== "ready"}
        onError={() => setStatus("error")}
        onLoad={(event) => {
          const image = event.currentTarget;
          setStatus(image.naturalWidth === 64 && (image.naturalHeight === 32 || image.naturalHeight === 64) ? "ready" : "error");
        }}
        src={src}
        style={{ visibility: status === "ready" ? "visible" : "hidden" }}
      />}
      {status !== "ready" && <span aria-label={status === "loading" ? `Carregando skin ${name}` : `Prévia indisponível para ${name}`} className="skin-thumbnail-fallback" role="img">
        <span aria-hidden="true">{status === "loading" ? "…" : "◇"}</span>
        <strong>{status === "loading" ? "Carregando…" : "Prévia indisponível"}</strong>
      </span>}
    </span>
  );
}

export function SkinThumbnail(props: Props) {
  // A different URL gets a fresh image and state; a late load/error from the old
  // image cannot replace the new selection or leave a previously failed card blank.
  return <ThumbnailImage key={props.src} {...props} />;
}
