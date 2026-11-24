import { useRef, useState } from "react";
import { drawSkinPortrait } from "../../lib/skinFace";
import "./skinPreview.css";

type Props = { src: string; name: string; model?: "classic" | "slim"; className?: string };

function ThumbnailImage({ src, name, model = "classic", className }: Props) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [status, setStatus] = useState<"loading" | "ready" | "error">(src.trim() ? "loading" : "error");
  return (
    <span className={`skin-thumbnail${className ? ` ${className}` : ""}`}>
      <canvas ref={canvas} width={16} height={32} role="img" aria-label={`Skin ${name}`} aria-hidden={status !== "ready"} style={{ visibility: status === "ready" ? "visible" : "hidden" }} />
      {src.trim() && <img
        alt="" aria-hidden="true" loading="lazy" decoding="async"
        onError={() => setStatus("error")}
        onLoad={(event) => {
          const image = event.currentTarget;
          const context = canvas.current?.getContext("2d");
          if (!context || image.naturalWidth !== 64 || ![32, 64].includes(image.naturalHeight)) {
            setStatus("error");
            return;
          }
          drawSkinPortrait(context, image, model);
          setStatus("ready");
        }}
        src={src}
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
  return <ThumbnailImage key={JSON.stringify([props.src, props.model])} {...props} />;
}
