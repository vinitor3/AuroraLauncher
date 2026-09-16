import { useEffect, useRef } from "react";

/** Face + hat layer from the already verified texture; no paid remote render per card. */
export function SkinFace({ url, name }: { url: string; name: string }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const image = new Image();
    let disposed = false;
    image.crossOrigin = "anonymous";
    image.onload = () => {
      const context = canvas.current?.getContext("2d");
      if (disposed || !context || image.width !== 64 || ![32, 64].includes(image.height)) return;
      context.clearRect(0, 0, 8, 8);
      context.drawImage(image, 8, 8, 8, 8, 0, 0, 8, 8);
      context.drawImage(image, 40, 8, 8, 8, 0, 0, 8, 8);
    };
    image.src = url;
    return () => { disposed = true; image.onload = null; image.src = ""; };
  }, [url]);
  return <canvas ref={canvas} width={8} height={8} role="img" aria-label={`Rosto da skin ${name}`} />;
}
