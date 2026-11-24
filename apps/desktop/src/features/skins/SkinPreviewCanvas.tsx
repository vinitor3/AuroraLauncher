import { useEffect, useRef, useState } from "react";
import { WalkingAnimation, SkinViewer } from "skinview3d";
import { SkinThumbnail } from "./SkinThumbnail";

type PreviewState = { skinUrl: string; skinModel: "classic" | "slim"; status: "loading" | "ready" | "error" };

export function SkinPreviewCanvas({ skinModel, skinUrl }: { skinModel: "classic" | "slim"; skinUrl: string }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const viewerRef = useRef<SkinViewer | null>(null);
  const [viewerUnavailable, setViewerUnavailable] = useState(false);
  const [preview, setPreview] = useState<PreviewState | null>(null);
  const [walking, setWalking] = useState(false);
  const [rotating, setRotating] = useState(false);
  // Hide the old texture in the render that changes the selection, before effects run.
  const status = !skinUrl.trim() ? "empty" : viewerUnavailable ? "error"
    : preview?.skinUrl === skinUrl && preview.skinModel === skinModel ? preview.status : "loading";

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    let viewer: SkinViewer;
    try {
      viewer = new SkinViewer({
        renderPaused: true,
        canvas,
        enableControls: true,
        height: 390,
        width: 320,
        zoom: 0.76,
      });
    } catch {
      setViewerUnavailable(true);
      return;
    }
    setViewerUnavailable(false);
    viewer.resetSkin();
    viewer.autoRotateSpeed = 0.32;
    viewer.controls.enablePan = false;
    viewer.controls.minDistance = 24;
    viewer.controls.maxDistance = 90;
    viewerRef.current = viewer;
    return () => {
      viewerRef.current = null;
      viewer.dispose();
    };
  }, []);

  useEffect(() => {
    const viewer = viewerRef.current;
    if (!viewer || status !== "ready") return;
    viewer.animation = walking ? new WalkingAnimation() : null;
    viewer.autoRotate = rotating;
    const redraw = () => { if (viewer.renderPaused && !document.hidden) viewer.render(); };
    const sync = () => {
      viewer.renderPaused = document.hidden || (!rotating && !walking);
      redraw();
    };
    sync();
    viewer.controls.addEventListener("change", redraw);
    document.addEventListener("visibilitychange", sync);
    return () => {
      viewer.renderPaused = true;
      viewer.controls.removeEventListener("change", redraw);
      document.removeEventListener("visibilitychange", sync);
    };
  }, [rotating, walking, status]);

  useEffect(() => {
    const viewer = viewerRef.current;
    if (!viewer) return;
    viewer.resetSkin();
    if (!skinUrl.trim()) return;
    setPreview({ skinUrl, skinModel, status: "loading" });
    const image = new Image();
    let cancelled = false;
    let settled = false;
    const isCurrent = () => !cancelled && !settled && viewerRef.current === viewer;
    const fail = () => {
      if (!isCurrent()) return;
      settled = true;
      window.clearTimeout(timeout);
      viewer.resetSkin();
      setPreview({ skinUrl, skinModel, status: "error" });
    };
    const timeout = window.setTimeout(fail, 15_000);
    image.crossOrigin = "anonymous";
    image.onload = () => {
      if (!isCurrent()) return;
      if (image.naturalWidth !== 64 || (image.naturalHeight !== 32 && image.naturalHeight !== 64)) {
        fail();
        return;
      }
      try {
        // Decoded image input is synchronous in skinview3d; never let a late URL load
        // mutate the shared viewer after a different skin has been selected.
        viewer.loadSkin(image, { model: skinModel === "classic" ? "default" : "slim" });
        settled = true;
        window.clearTimeout(timeout);
        setPreview({ skinUrl, skinModel, status: "ready" });
      } catch {
        fail();
      }
    };
    image.onerror = fail;
    image.src = skinUrl;
    return () => {
      cancelled = true;
      window.clearTimeout(timeout);
      image.onload = null;
      image.onerror = null;
      image.src = "";
    };
  }, [skinModel, skinUrl]);

  return (
    <><div className="skin-viewer-shell">
      <canvas aria-hidden={status !== "ready"} aria-label="Prévia 3D interativa da skin selecionada" className="skin-preview-canvas" ref={canvasRef} style={{ visibility: status === "ready" ? "visible" : "hidden" }} />
      {viewerUnavailable && skinUrl ? <SkinThumbnail src={skinUrl} name="selecionada" model={skinModel} className="skin-preview-flat" /> : status !== "ready" && <div aria-live="polite" className={`skin-preview-placeholder${status === "error" ? " error" : ""}`} role="status">
        <span aria-hidden="true">{status === "error" ? "!" : status === "loading" ? "…" : "◇"}</span>
        <strong>{status === "empty" ? "Selecione uma skin" : status === "loading" ? "Carregando prévia…" : "Não foi possível abrir esta skin"}</strong>
      </div>}
      {status === "ready" && <span className="skin-viewer-help">Arraste para girar · role para aproximar</span>}
      {viewerUnavailable && skinUrl && <span className="skin-viewer-help">Prévia 2D</span>}
    </div>
    {status === "ready" && <div className="skin-preview-controls" role="group" aria-label="Movimento da prévia">
      <button type="button" className={rotating ? "compact" : "secondary compact"} aria-pressed={rotating} onClick={() => setRotating(!rotating)}>Girar</button>
      <button type="button" className={walking ? "compact" : "secondary compact"} aria-pressed={walking} onClick={() => setWalking(!walking)}>Caminhar</button>
    </div>}</>
  );
}
