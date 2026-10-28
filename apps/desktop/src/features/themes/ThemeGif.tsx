import { useEffect, useRef } from "react";
import type { ThemeAsset } from "./model";
import { openThemeGif } from "./gif";

export function ThemeGif({ asset, paused, onError }: { asset: ThemeAsset; paused: boolean; onError?: (message: string) => void }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const errorHandler = useRef(onError);
  errorHandler.current = onError;
  useEffect(() => {
    if (paused && document.hidden) return;
    const controller = new AbortController();
    let disposed = false;
    let decoder: ImageDecoder | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let watchdog: ReturnType<typeof setTimeout> | undefined;
    const close = () => { clearTimeout(timer); clearTimeout(watchdog); decoder?.close(); decoder = undefined; };
    const fail = () => { if (!disposed) { disposed = true; close(); errorHandler.current?.("Não foi possível reproduzir o GIF. A aparência padrão foi restaurada."); } };
    void (async () => {
      const opened = await openThemeGif(asset, controller.signal);
      if (disposed) { opened.decoder.close(); return; }
      decoder = opened.decoder;
      const target = canvas.current;
      const context = target?.getContext("2d");
      if (!target || !context) throw new Error("Canvas unavailable");
      let index = 0;
      async function draw() {
        if (disposed || !decoder || !target || !context) return;
        watchdog = setTimeout(fail, 8_000);
        const result = await decoder.decode({ frameIndex: index, completeFramesOnly: true });
        clearTimeout(watchdog);
        let delay = 100;
        try {
          if (disposed) return;
          if (target.width !== result.image.displayWidth || target.height !== result.image.displayHeight) {
            target.width = result.image.displayWidth; target.height = result.image.displayHeight;
          }
          context.clearRect(0, 0, target.width, target.height);
          context.drawImage(result.image, 0, 0, target.width, target.height);
          delay = Math.max(40, Math.min(60_000, (result.image.duration ?? 100_000) / 1000));
          target.dataset.frame = String(index);
        } finally { result.image.close(); }
        if (paused || opened.info.frames === 1) { close(); return; }
        index = (index + 1) % opened.info.frames;
        timer = setTimeout(() => { void draw().catch(fail); }, delay);
      }
      await draw();
    })().catch(fail);
    return () => { disposed = true; controller.abort(); close(); };
  }, [asset, paused]);
  return <canvas ref={canvas} className="theme-background-media" aria-hidden="true" />;
}
