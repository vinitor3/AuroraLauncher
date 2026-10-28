import { inspectGif, type ThemeAsset } from "./model";

const GIF_UNSUPPORTED = "GIF animado não está disponível neste WebView. Atualize o WebView2 ou use um vídeo MP4/WebM.";

/** No <img> animation: decoding frames ourselves makes pause and cleanup enforceable. */
export async function openThemeGif(asset: ThemeAsset, signal?: AbortSignal) {
  if (typeof ImageDecoder === "undefined" || !await ImageDecoder.isTypeSupported("image/gif")) throw new Error(GIF_UNSUPPORTED);
  if (signal?.aborted) throw new DOMException("GIF cancelado", "AbortError");
  const bytes = Uint8Array.from(atob(asset.data), (character) => character.charCodeAt(0));
  const info = inspectGif(bytes);
  const width = Math.min(960, info.width);
  const height = Math.max(1, Math.round(info.height * width / info.width));
  const decoder = new ImageDecoder({ data: bytes, type: "image/gif", desiredWidth: width, desiredHeight: height, preferAnimation: true });
  let timer: ReturnType<typeof setTimeout> | undefined;
  let onAbort: (() => void) | undefined;
  try {
    await Promise.race([decoder.tracks.ready, new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new Error("O GIF demorou para preparar os quadros. Use um arquivo menor.")), 8_000);
      onAbort = () => reject(new DOMException("GIF cancelado", "AbortError"));
      signal?.addEventListener("abort", onAbort, { once: true });
      if (signal?.aborted) onAbort();
    })]);
    return { decoder, info };
  } catch (error) { decoder.close(); throw error; }
  finally { clearTimeout(timer); if (onAbort) signal?.removeEventListener("abort", onAbort); }
}

export async function validateThemeGif(asset: ThemeAsset) {
  const { decoder, info } = await openThemeGif(asset);
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    await Promise.race([
      (async () => {
        await decoder.tracks.ready;
        if (decoder.tracks.selectedTrack?.frameCount !== info.frames) throw new Error("A sequência do GIF não corresponde ao documento.");
        // Decode the end too: later damaged frames must not get a free pass.
        for (const frameIndex of [...new Set([0, info.frames - 1])]) {
          const result = await decoder.decode({ frameIndex, completeFramesOnly: true });
          try {
            if (!result.complete || result.image.displayWidth < 1 || result.image.displayHeight < 1
              || result.image.displayWidth > info.width || result.image.displayHeight > info.height) throw new Error("O GIF possui um quadro inválido.");
          } finally { result.image.close(); }
        }
      })(),
      new Promise<never>((_, reject) => { timer = setTimeout(() => { decoder.close(); reject(new Error("O GIF demorou para abrir. Use um arquivo menor ou um vídeo MP4/WebM.")); }, 8_000); }),
    ]);
  } finally { clearTimeout(timer); decoder.close(); }
}
