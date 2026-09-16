import { useEffect, useRef, useState } from "react";
import { IdleAnimation, SkinViewer } from "skinview3d";

export function SkinPreviewCanvas({ skinModel, skinUrl }: { skinModel: "classic" | "slim"; skinUrl: string }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const viewerRef = useRef<SkinViewer | null>(null);
  const [previewError, setPreviewError] = useState(false);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const idle = new IdleAnimation();
    idle.speed = 0.65;
    const viewer = new SkinViewer({
      animation: idle,
      background: 0x090714,
      canvas,
      enableControls: true,
      height: 390,
      width: 320,
      zoom: 0.76,
    });
    viewer.autoRotate = true;
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
    if (!viewer) return;
    setPreviewError(false);
    if (!skinUrl.trim()) {
      viewer.resetSkin();
      return;
    }
    const image = new Image();
    let cancelled = false;
    image.crossOrigin = "anonymous";
    image.onload = () => {
      if (cancelled || viewerRef.current !== viewer) return;
      try { viewer.loadSkin(image, { model: skinModel === "classic" ? "default" : "slim" }); }
      catch { setPreviewError(true); }
    };
    image.onerror = () => { if (!cancelled) setPreviewError(true); };
    image.src = skinUrl;
    return () => { cancelled = true; image.onload = null; image.onerror = null; image.src = ""; };
  }, [skinModel, skinUrl]);

  return (
    <div className="skin-viewer-shell">
      <canvas aria-label="Prévia 3D interativa da skin selecionada" className="skin-preview-canvas" ref={canvasRef} />
      {!skinUrl.trim() && <div className="skin-preview-placeholder"><span>◇</span><strong>Selecione uma skin</strong></div>}
      {previewError && <div className="skin-preview-placeholder error"><span>!</span><strong>Não foi possível abrir este PNG</strong></div>}
      <span className="skin-viewer-help">Arraste para girar · role para aproximar</span>
    </div>
  );
}
