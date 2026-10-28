import { Fragment, type ReactNode } from "react";
import type { AuroraTheme } from "./model";

/** Reorder real DOM regions, keeping reading and keyboard order with the layout. */
export function ThemeWorkspace({ theme, editing, className, header, decorations, footer, children }: {
  theme: AuroraTheme; editing: boolean; className: string;
  header: ReactNode; decorations: ReactNode; footer: ReactNode; children: ReactNode;
}) {
  const regions = { header, decorations, content: children };
  const order = editing ? ["header", "content"] as const : theme.layout.sectionOrder;
  return <section className={className} data-theme-editing={editing ? "true" : "false"}>
    {order.map((region) => <Fragment key={region}>{regions[region]}</Fragment>)}
    {footer}
  </section>;
}
