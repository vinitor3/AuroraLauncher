import { useEffect, useRef } from "react";
import type { AuroraTheme } from "./model";

/** Artwork is fixed by Aurora; a theme can supply only a validated hex color/size. */
export function themeCursorValue(cursor: AuroraTheme["cursor"]): string {
  if (cursor.style === "system") return "auto";
  const art = cursor.style === "pixel" ? '<path d="M3 2h4v4h4v4h4v4h4v4H12v4H8v-7H3Z" />'
    : cursor.style === "ring" ? '<circle cx="12" cy="12" r="8"/><circle cx="12" cy="12" r="1" fill="currentColor" />'
    : '<path d="M10 2h4v6h8v4h-8v10h-4V12H2V8h8Z" />';
  const body = cursor.style === "ring" ? `fill="none" stroke="${cursor.color}"` : `fill="${cursor.color}" stroke="#101018"`;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${cursor.size}" height="${cursor.size}" viewBox="0 0 24 24" color="${cursor.color}"><g stroke="#ffffff" stroke-width="3" fill="none">${art}</g><g ${body} stroke-width="1.4" stroke-linejoin="round">${art}</g></svg>`;
  const hotspot = cursor.style === "pixel" ? Math.round(cursor.size * .125) : Math.round(cursor.size / 2);
  return `url("data:image/svg+xml,${encodeURIComponent(svg)}") ${hotspot} ${hotspot}, auto`;
}

export function ThemeCursor({ theme }: { theme: AuroraTheme }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    if (theme.cursor.trail === "none") return;
    const target = canvas.current;
    const context = target?.getContext("2d");
    if (!target || !context) return;
    const motion = window.matchMedia("(prefers-reduced-motion: reduce)");
    let particles: Array<{ x: number; y: number; born: number }> = [];
    let frame = 0, last = -Infinity;
    const stop = () => { cancelAnimationFrame(frame); frame = 0; particles = []; context.clearRect(0, 0, target.width, target.height); };
    const resize = () => { target.width = window.innerWidth; target.height = window.innerHeight; stop(); };
    const tick = (now: number) => {
      frame = 0;
      context.clearRect(0, 0, target.width, target.height);
      particles = particles.filter((point) => now - point.born < 360);
      for (const point of particles) {
        const strength = 1 - (now - point.born) / 360;
        context.globalAlpha = strength * .6;
        context.strokeStyle = theme.cursor.color;
        context.lineWidth = 1.5;
        const size = 2 + strength * 3;
        context.beginPath(); context.moveTo(point.x - size, point.y); context.lineTo(point.x + size, point.y);
        context.moveTo(point.x, point.y - size); context.lineTo(point.x, point.y + size); context.stroke();
      }
      context.globalAlpha = 1;
      if (particles.length) frame = requestAnimationFrame(tick);
    };
    const move = (event: PointerEvent) => {
      if (document.hidden || motion.matches || event.pointerType === "touch" || !(event.target instanceof Element)
        || !event.target.closest(".launcher-theme-root") || event.target.closest(".theme-manager")) { stop(); return; }
      const now = performance.now();
      if (now - last < 45) return;
      last = now;
      particles.push({ x: event.clientX, y: event.clientY, born: now });
      if (particles.length > 8) particles.shift();
      if (!frame) frame = requestAnimationFrame(tick);
    };
    resize();
    window.addEventListener("resize", resize);
    window.addEventListener("pointermove", move, { passive: true });
    window.addEventListener("blur", stop);
    document.addEventListener("visibilitychange", stop);
    motion.addEventListener("change", stop);
    return () => { stop(); window.removeEventListener("resize", resize); window.removeEventListener("pointermove", move); window.removeEventListener("blur", stop); document.removeEventListener("visibilitychange", stop); motion.removeEventListener("change", stop); };
  }, [theme.cursor]);
  return theme.cursor.trail === "sparkle" ? <canvas ref={canvas} className="theme-cursor-trail" aria-hidden="true" /> : null;
}
