import { useEffect, useRef, useState } from "react";
import { drawSkinFace } from "../../lib/skinFace";

/** Face + hat layer from the already verified texture; no paid remote render per card. */
export function SkinFace({ url, name }: { url: string; name: string }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [loadedUrl, setLoadedUrl] = useState("");
  useEffect(() => {
    const image = new Image();
    let disposed = false;
    canvas.current?.getContext("2d")?.clearRect(0, 0, 8, 8);
    image.crossOrigin = "anonymous";
    image.onload = () => {
      const context = canvas.current?.getContext("2d");
      if (disposed || !context || image.naturalWidth !== 64 || ![32, 64].includes(image.naturalHeight)) return;
      drawSkinFace(context, image);
      setLoadedUrl(url);
    };
    image.src = url;
    return () => { disposed = true; image.onload = null; image.src = ""; };
  }, [url]);
  return <span className="skin-face" role="img" aria-label={loadedUrl === url ? `Rosto da skin ${name}` : `Skin ${name}`}>
    <canvas ref={canvas} width={8} height={8} aria-hidden="true" style={{ visibility: loadedUrl === url ? "visible" : "hidden" }} />
    {loadedUrl !== url && <span aria-hidden="true" className="skin-face-fallback">◇</span>}
  </span>;
}
