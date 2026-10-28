import type { AuroraTheme, NavId } from "./model";

const ICONS: Record<string, string> = { star: "★", leaf: "❧", moon: "☾", heart: "♥", gamepad: "✜", sparkles: "✧" };

export function ThemeNavIcon({ theme, section, fallback, onError }: { theme: AuroraTheme; section: NavId; fallback: string; onError: (message: string) => void }) {
  const icon = theme.icons[section];
  const asset = icon.startsWith("asset:") ? theme.assets[icon.slice(6)] : undefined;
  if (asset?.mime === "image/png") return <img className="theme-nav-image" src={`data:image/png;base64,${asset.data}`} alt="" onError={() => onError("Não foi possível abrir o ícone do tema. A aparência padrão foi restaurada.")} />;
  return <>{ICONS[icon] ?? fallback}</>;
}

export function ThemeLogo({ theme, onError }: { theme: AuroraTheme; onError: (message: string) => void }) {
  const asset = theme.media.logo ? theme.assets[theme.media.logo] : undefined;
  return asset?.mime === "image/png"
    ? <img className="theme-brand-image" src={`data:image/png;base64,${asset.data}`} alt="Logotipo personalizado" onError={() => onError("Não foi possível abrir o logotipo do tema. A aparência padrão foi restaurada.")} />
    : <>✦</>;
}
