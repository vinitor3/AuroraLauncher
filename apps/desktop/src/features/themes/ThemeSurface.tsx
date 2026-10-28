import type { ReactNode } from "react";
import "./surface.css";

type ThemeSurfaceProps = {
  /** Stable identifier owned by the feature's source code, never by a theme. */
  surfaceId: string;
  as?: "section" | "article" | "div";
  labelledBy?: string;
  className?: string;
  children: ReactNode;
};

/** Presentation only: callbacks, permissions and feature state stay in children. */
export function ThemeSurface({ surfaceId, as = "section", labelledBy, className, children }: ThemeSurfaceProps) {
  const Tag = as === "article" ? "article" : as === "div" ? "div" : "section";
  return <Tag className={`theme-surface${className ? ` ${className}` : ""}`} data-theme-surface={surfaceId} aria-labelledby={labelledBy}>
    {children}
  </Tag>;
}
