import activeMods from "../../../scripts/active-mod-matrix.json";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { type FormEvent, type KeyboardEvent as ReactKeyboardEvent, useEffect, useMemo, useRef, useState } from "react";
import { lazy, Suspense } from "react";
import { drawSkinFace } from "./lib/skinFace";
import { CapePanel, useWardrobeCape } from "./features/skins/CapeWardrobe";
import { ProjectDialog } from "./features/catalog/ProjectDialog";
import { SkinPreview } from "./features/skins/SkinPreview";
import { SkinThumbnail } from "./features/skins/SkinThumbnail";
import { StartupScreen } from "./features/auth/StartupScreen";
import { PasswordResetForm, RecoveryEmailDialog } from "./features/account/AccountRecovery";
import { useThemes } from "./features/themes/ThemeProvider";
import { ThemeManager } from "./features/themes/ThemeManager";
import { ThemeScene, ThemeDecorations } from "./features/themes/ThemeScene";
import { ThemeSurface } from "./features/themes/ThemeSurface";
import { ThemeWorkspace } from "./features/themes/ThemeWorkspace";
import { ThemeCursor, themeCursorValue } from "./features/themes/ThemeCursor";
import { ThemeLogo, ThemeNavIcon } from "./features/themes/ThemeIcon";
import { GlobalMemoryPanel } from "./features/java-engine/GlobalMemoryPanel";
import { LaunchSettingsPanel } from "./features/java-engine/LaunchSettingsPanel";
import { DEFAULT_THEME, themeVariables } from "./features/themes/model";
import { guardSessionProfile, startSessionStartup, type SessionStartupState } from "./lib/sessionStartup";
import { browserLauncherAuthScope, prepareLauncherServices } from "./lib/launcherAuthScope";
import {
  bundledFirebaseConfig,
  socialFeatureBuild,
  socialLabBuild,
  createFirebaseServices,
  clearRemoteSkinLibrary,
  loadAuroraProfile,
  loginAuroraUser,
  logoutAuroraUser,
  observeAuroraSession,
  registerAuroraUser,
  saveAuroraAppearance,
  syncAuroraPublicProfile,
  uploadFirebaseAppearanceImage,
  type AuroraUserProfile,
  type FirebasePublicConfig,
  type FirebaseServices,
} from "./lib/firebase";
import {
  requestCurseForgeCatalog,
  uploadAuroraAppearanceImage,
  type CurseForgeUpdateFile,
  type CurseForgeUpdatesResponse,
} from "./lib/edge";
import {
  catalogCategoryLabel,
  groupedCatalogCategories,
  type CatalogCategoryOption,
} from "./lib/catalogCategories";
import { cacheLocalSkinImage, deleteLocalSkin, isRetiredLocalSkin, listLocalSkins, saveLocalSkin, updateLocalSkinSource, type LocalSkin } from "./lib/localSkins";
import { isRetiredSkinSource, publicCatalogSource, PUBLIC_SKIN_WORKER, skinSha256, verifyPublicCatalogSkin } from "./lib/skinCatalogSources";
import { findMatchingLocalSkin, parseEquippedSkinSelection, type EquippedSkinSelection } from "./lib/equippedSkin";
import { ExternalDownloadQueue, type ExternalDownloadEvent } from "./lib/externalDownloadQueue";
import { observeDownloadProgress, type DownloadProgress } from "./lib/downloadProgress";
import { OnlineSkinSearch } from "./features/skins/OnlineSkinSearch";
import { ContentUpdatesView } from "./features/content-updates/ContentUpdatesView";
import { UpdateReviewDialog } from "./features/content-updates/UpdateReviewDialog";
import { InstanceLogsPanel } from "./features/support/InstanceLogsPanel";
import { WorldBackupsPanel } from "./features/world-backups/WorldBackupsPanel";
import { SupportContextButton } from "./features/support/SupportContextButton";
import { DoctorView } from "./features/doctor/DoctorView";
import { RecoveryView } from "./features/recovery/RecoveryView";
import { DiagnosticLabView } from "./features/diagnostic-lab/DiagnosticLabView";
import { ConnectView } from "./features/connect/ConnectView";
import { NativeVoicePanel } from "./features/connect-native-voice/NativeVoicePanel";
import { ExitLauncherButton } from "./features/connect/ExitLauncherButton";
import { useFriendInbox } from "./features/social/useFriendInbox";
import { SocialView } from "./features/social/SocialView";
import { socialPlayRequest, type SocialPlayRequest } from "./features/connect-game-launch/socialPlay";
import { useDiscordPresence } from "./features/social/useDiscordPresence";
import { usePublicInvite } from "./features/social/usePublicInvite";
import { SocialBuildBanner } from "./features/social/SocialBuildBanner";
import { createSocialConnectController, emptySocialConnectState, type SocialConnectController, type SocialConnectState } from "./features/social/connect-controller";
import { createSocialFirebaseService } from "./lib/socialFirebase";
import { createAccountTransition, stopConnectForAccountTransition } from "./features/connect/account-transition";
import { VoiceAnnouncementProvider, useVoiceAnnouncement, withVoiceAnnouncementLogout } from "./features/voice-announcement/VoiceAnnouncementProvider";
import { capabilitiesFrom } from "./features/connect/types";
import type { DoctorRun } from "./features/doctor/types";
import type {
  ContentRollbackSummary,
  ContentUpdateFilter,
  ContentUpdateItem,
  ContentUpdateScanResult,
  ContentUpdateScanView,
  ContentUpdateSummary,
  ModpackUpdate,
  ModpackUpdateSummary,
  UpdateReviewRequest,
} from "./features/content-updates/types";

const KeymapPanel = lazy(() => import("./features/keymap/KeymapPanel"));
const ProjectDescription = lazy(() => import("./features/catalog/ProjectDescription").then(module => ({ default: module.ProjectDescription })));

type EngineStatus = { dataDirectory: string; ready: boolean };
type Instance = {
  id: string;
  path: string;
  hasModsDirectory: boolean;
  hasInstalledVersion: boolean;
  displayName?: string;
  iconUrl?: string;
  projectId?: string;
};
type JavaRuntime = { executable: string; version: string };
type JavaReleaseCatalog = { available: number[]; lts: number[] };
type RunningInstance = { instanceId: string; processId: number };
type AppearanceImage = { url: string; dataBase64: string };
type InstallSummary = { minecraftVersion: string; versionId: string; clientJar: string; libraryCount: number; assetCount: number };
type ModrinthPack = {
  projectId: string;
  slug: string;
  title: string;
  description: string;
  versions: string[];
  loaders: string[];
  iconUrl?: string;
  downloads: number;
  follows: number;
  author: string;
  dateModified: string;
};
type ModrinthSearchPage = { items: ModrinthPack[]; totalHits: number; offset: number; limit: number };
type CatalogPack = ModrinthPack & {
  source: CatalogSource;
  curseForgeId?: number;
  curseForgeHasOnlyUnsupportedLoaders?: boolean;
  websiteUrl?: string;
  gallery?: string[];
};
type ModpackInstallSummary = {
  name: string;
  versionName: string;
  minecraftVersion: string;
  loader: string;
  downloadedFiles: number;
  overrideFiles: number;
  minecraft: InstallSummary;
};
type InstanceContentFile = { name: string; enabled: boolean };
type InstanceContent = { mods: InstanceContentFile[]; shaderpacks: InstanceContentFile[]; resourcepacks: InstanceContentFile[] };

type InstanceLaunchProfile = { versionId?: string; minecraftVersion?: string };
type ModrinthContent = {
  projectId: string;
  slug: string;
  title: string;
  description: string;
  iconUrl?: string;
  versions: string[];
  loaders: string[];
  downloads: number;
  author: string;
  dateModified: string;
};
type ContentType = "mod" | "shader" | "resourcepack";
type EditorView = "logs" | "worlds" | "installed" | "keymap" | "memory" | "updates" | "discover" | "doctor" | "recovery" | "diagnostic-lab";
type ContentSort = "popular" | "relevance" | "updated";
type CatalogContent = ModrinthContent & {
  source: CatalogSource;
  curseForgeId?: number;
  websiteUrl?: string;
  gallery?: string[];
};
type CurseForgeProject = {
  id: number;
  classId?: number;
  name: string;
  slug: string;
  summary?: string;
  downloadCount?: number;
  dateModified?: string;
  logo?: { thumbnailUrl?: string; url?: string };
  links?: { websiteUrl?: string };
  authors?: Array<{ name?: string }>;
  categories?: Array<{ name?: string }>;
  screenshots?: Array<{ thumbnailUrl?: string; url?: string }>;
  latestFilesIndexes?: Array<{ gameVersion?: string; modLoader?: number }>;
};
type CurseForgeFile = {
  id: number;
  modId?: number;
  fileName: string;
  displayName?: string;
  downloadUrl?: string;
  fileLength?: number;
  isServerPack?: boolean;
  gameVersions?: string[];
  hashes?: Array<{ algo: number; value: string }>;
  dependencies?: Array<{ modId: number; relationType: number }>;
  project?: CurseForgeProject;
};
type CurseForgeSearchResponse = { data?: CurseForgeProject[]; pagination?: { totalCount?: number } };
type CurseForgeCategory = { id: number; name: string; slug?: string; group?: string };
type ModrinthCategory = { name: string; projectType: string; header: string };
type CurseForgeManifestSummary = {
  name: string;
  versionName: string;
  minecraftVersion: string;
  loader: LoaderChoice;
  loaderVersion: string;
  files: Array<{ projectId: number; fileId: number; required: boolean }>;
};
type CurseForgeResolvedFile = {
  projectId: number;
  fileId: number;
  fileName: string;
  downloadUrl?: string;
  sha1: string;
  fileSize?: number;
};
type ProjectDetail = {
  source: CatalogSource;
  kind: "content" | "modpack";
  title: string;
  author?: string;
  iconUrl?: string;
  summary: string;
  body: string;
  bodyFormat: "markdown" | "html";
  gallery: string[];
  websiteUrl?: string;
  content?: CatalogContent;
  pack?: CatalogPack;
};
type ContentArtworkEntry = { iconUrl?: string; projectId: string; source?: CatalogSource; title: string };
type ResolvedContentArtwork = ContentArtworkEntry & { filename: string };
type ContentArtworkMigration = {
  contentType?: ContentType;
  fromFilename: string;
  toFilename: string;
  fallback?: ContentArtworkEntry;
};
type LoaderChoice = "fabric" | "forge";
type LauncherSection = "instances" | "discover" | "appearance" | "java" | "connect" | "social";
type CatalogSource = "modrinth" | "curseforge";
type LaunchSummary = {
  processId: number;
  runId: string;
  versionId: string;
  coreInstalled: boolean;
  skinsInstalled: boolean;
};
type CoreConnection =
  | { kind: "connected"; loader: string; minecraft_version: string }
  | { kind: "disconnected" };
type IpcEvent = CoreConnection | { kind: "moduleEvent"; name: string; payload: unknown };
type IpcSessionEvent = { processId: number; event: IpcEvent };
type AuthMode = "login" | "register";
type SkinAddMode = "file" | "url";
type LocalSkinView = LocalSkin & { previewUrl: string };
type EquippedSkinView = EquippedSkinSelection & { name: string; image: Blob; previewUrl: string };
// Published build targets and loader pins govern Aurora's integrated profiles.
const integratedTarget = (minecraft: string, loader: string) =>
  activeMods.targets.some((target) => target.minecraft === minecraft && target.loader === loader);
const DEFAULT_FORGE_VERSION: Record<string, string> = {
  "26.3": "66.1.9",
  "26.2": "65.1.5",
  "26.1.2": "64.1.5",
  "1.21.11": "61.2.4",
  "1.18.2": "40.3.0",
  ...Object.fromEntries(activeMods.targets.filter((target) => target.loader === "forge")
    .reverse().map((target) => [target.minecraft, target.defaultInstallLoaderVersion ?? target.loaderVersion])),
};
const INSTALLABLE_VERSIONS = Object.keys(DEFAULT_FORGE_VERSION)
  .sort((left, right) => right.localeCompare(left, "en", { numeric: true }));
const UPDATE_CONTENT_TYPES: ContentType[] = ["mod", "shader", "resourcepack"];
const EDITOR_SECTIONS = [
  { label: "Conteúdo", views: [["installed", "Instalados"], ["discover", "Adicionar"], ["updates", "Atualizações"]] },
  { label: "Jogo", views: [["keymap", "Teclas"], ["memory", "Memória"], ["worlds", "Backups de mundos"]] },
  { label: "Suporte", views: [["logs", "Logs e crashes"], ["doctor", "Diagnóstico"], ["recovery", "Recuperação"], ["diagnostic-lab", "Ambiente de teste"]] },
] as const;
const EDITOR_VIEWS: EditorView[] = EDITOR_SECTIONS.flatMap(({ views }) => views.map(([view]) => view));

function formatBytes(value?: number) {
  if (!value || value < 1) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const unit = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1);
  const amount = value / 1024 ** unit;
  return `${amount >= 100 || unit === 0 ? amount.toFixed(0) : amount.toFixed(1)} ${units[unit]}`;
}

function contentDisplayName(filename: string) {
  const basename = filename
    .replace(/\.disabled$/i, "")
    .replace(/\.(jar|zip)$/i, "");
  const pieces = basename.split(/[-_]+/).filter(Boolean);
  const versionStart = pieces.findIndex((piece, index) => index > 0 && /^v?\d+(?:\.|$)/i.test(piece));
  const readablePieces = versionStart > 0 ? pieces.slice(0, versionStart) : pieces;
  return readablePieces
    .join(" ")
    .replace(/\b\w/g, (letter) => letter.toUpperCase())
    .replace(/\bApi\b/g, "API");
}

function updaterErrorMessage(error: unknown) {
  const message = error instanceof Error ? error.message : String(error);
  return message.replace(/^Error:\s*/i, "").trim() || "O Aurora não conseguiu concluir esta operação.";
}

function supportedLoaderFromVersionId(versionId: string): LoaderChoice | undefined {
  const normalized = versionId.toLowerCase();
  if (normalized.includes("neoforge")) return undefined;
  if (normalized.includes("fabric")) return "fabric";
  if (normalized.includes("forge")) return "forge";
  return undefined;
}

function missingRequiredCurseForgeDependencies(
  target: CurseForgeUpdateFile | undefined,
  installedProjectIds: ReadonlySet<number>,
) {
  return [...new Set((target?.dependencies ?? [])
    .filter((dependency) => dependency.relationType === 3 && !installedProjectIds.has(dependency.modId))
    .map((dependency) => dependency.modId))]
    .sort((left, right) => left - right);
}

function contentArtworkKey(contentType: ContentType, filename: string) {
  return `${contentType}:${filename}`;
}

function registeredArtworkSource(artwork?: ContentArtworkEntry): CatalogSource | undefined {
  if (artwork?.source) return artwork.source;
  if (!artwork?.projectId) return undefined;
  return /^\d+$/.test(artwork.projectId) ? "curseforge" : "modrinth";
}

function curseForgeDownloadPage(slug: string, fileId: number, contentType: ContentType | "modpack", websiteUrl?: string) {
  if (websiteUrl) {
    try {
      const url = new URL(websiteUrl);
      if (url.protocol === "https:" && (url.hostname === "curseforge.com" || url.hostname.endsWith(".curseforge.com"))) {
        url.search = "";
        url.hash = "";
        url.pathname = `${url.pathname.replace(/\/+$/, "")}/download/${fileId}`;
        return url.toString();
      }
    } catch {
      // Usa abaixo a rota oficial construída a partir do slug conhecido.
    }
  }
  const category = contentType === "modpack" ? "modpacks" : contentType === "mod" ? "mc-mods" : contentType === "shader" ? "shaders" : "texture-packs";
  return `https://www.curseforge.com/minecraft/${category}/${encodeURIComponent(slug)}/download/${fileId}`;
}

function ContentArtwork({ compact = false, iconUrl, title, type }: { compact?: boolean; iconUrl?: string; title: string; type: ContentType }) {
  const fallback = type === "mod" ? "◆" : type === "shader" ? "✦" : "▧";
  return (
    <span className={`content-artwork ${type} ${compact ? "compact-artwork" : ""}`}>
      <span aria-hidden="true">{fallback}</span>
      {iconUrl && <img alt={`Imagem de ${title}`} onError={(event) => { event.currentTarget.style.display = "none"; }} src={iconUrl} />}
    </span>
  );
}
const MINECRAFT_NICK_PATTERN = /^[A-Za-z0-9_]{3,16}$/;

function sanitizeMinecraftNickname(value: string) {
  return value.replace(/[^A-Za-z0-9_]/g, "").slice(0, 16);
}

async function validateAppearanceFile(kind: "skin" | "cape", file: File) {
  if (file.type !== "image/png" || file.size === 0 || file.size > 5 * 1024 * 1024) {
    throw new Error(`Escolha ${kind === "skin" ? "uma skin" : "uma capa"} PNG de até 5 MB.`);
  }
  const bitmap = await createImageBitmap(file);
  const { width, height } = bitmap;
  bitmap.close();
  if (kind === "skin" && !(width === 64 && (height === 64 || height === 32))) {
    throw new Error(`A skin precisa ter 64x64 ou 64x32 pixels; este arquivo tem ${width}x${height}.`);
  }
  if (kind === "cape" && (width === 0 || height === 0 || width > 1024 || height > 1024)) {
    throw new Error(`As dimensões da capa são inválidas: ${width}x${height}.`);
  }
}

function SkinHeadAvatar({ source }: { source?: string | null }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [mode, setMode] = useState<"loading" | "head" | "image" | "empty">(source ? "loading" : "empty");
  useEffect(() => {
    let cancelled = false;
    if (!source) {
      setMode("empty");
      return undefined;
    }
    setMode("loading");
    const image = new Image();
    image.onload = () => {
      if (cancelled) return;
      if (image.naturalWidth !== 64 || ![32, 64].includes(image.naturalHeight)) {
        setMode("image");
        return;
      }
      const canvas = canvasRef.current;
      const context = canvas?.getContext("2d");
      if (!canvas || !context) {
        setMode("image");
        return;
      }
      drawSkinFace(context, image);
      setMode("head");
    };
    image.onerror = () => {
      if (!cancelled) setMode("empty");
    };
    image.src = source;
    return () => {
      cancelled = true;
      image.onload = null;
      image.onerror = null;
    };
  }, [source]);
  return (
    <span className="user-avatar" aria-hidden="true">
      <canvas className={mode === "head" ? "" : "hidden"} height={64} ref={canvasRef} width={64} />
      {mode === "image" ? <img alt="" src={source ?? ""} /> : null}
      {mode === "loading" || mode === "empty" ? "✦" : null}
    </span>
  );
}

function safeHttpsUrl(value?: string | null) {
  if (!value) return "";
  try {
    const url = new URL(value);
    return url.protocol === "https:" ? url.toString() : "";
  } catch {
    return "";
  }
}

function normalizeProjectMarkdown(value: string) {
  if (!/[<&]|\\[<>]/.test(value)) return value;
  const decoder = document.createElement("textarea");
  decoder.innerHTML = value.replace(/\\([<>])/g, "$1");
  const documentValue = new DOMParser().parseFromString(decoder.value, "text/html");
  const renderNode = (node: Node): string => {
    if (node.nodeType === Node.TEXT_NODE) return node.textContent ?? "";
    if (!(node instanceof HTMLElement)) return "";
    const tag = node.tagName.toLowerCase();
    if (["script", "style", "object", "embed", "form"].includes(tag)) return "";
    const children = Array.from(node.childNodes).map(renderNode).join("");
    if (tag === "iframe") {
      const source = safeHttpsUrl(node.getAttribute("src"));
      return source ? `\n\n[▶ Assistir vídeo](${source})\n\n` : "";
    }
    if (tag === "img") {
      const source = safeHttpsUrl(node.getAttribute("src"));
      const alt = (node.getAttribute("alt") ?? "Imagem do projeto").replace(/[\[\]]/g, "");
      return source ? `\n\n![${alt}](${source})\n\n` : "";
    }
    if (tag === "a") {
      const href = safeHttpsUrl(node.getAttribute("href"));
      return href ? `[${children.trim() || href}](${href})` : children;
    }
    if (["strong", "b"].includes(tag)) return children.trim() ? `**${children.trim()}**` : "";
    if (["em", "i"].includes(tag)) return children.trim() ? `*${children.trim()}*` : "";
    if (/^h[1-6]$/.test(tag)) return `\n\n${"#".repeat(Number(tag[1]))} ${children.trim()}\n\n`;
    if (tag === "li") return `\n- ${children.trim()}`;
    if (tag === "br") return "\n";
    if (["p", "div", "center", "section", "ul", "ol", "blockquote"].includes(tag)) {
      return `\n\n${children.trim()}\n\n`;
    }
    return children;
  };
  return Array.from(documentValue.body.childNodes)
    .map(renderNode)
    .join("")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

function fileAsDataUrl(file: File) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result === "string") resolve(reader.result);
      else reject(new Error("não foi possível codificar a imagem"));
    };
    reader.onerror = () => reject(new Error("não foi possível ler a imagem"));
    reader.readAsDataURL(file);
  });
}

function requiredJavaMajor(minecraftVersion: string, versionId = "") {
  const [generation, minor, patch = 0] = minecraftVersion.split(".").map(Number);
  if (generation >= 26) return 25;
  if (generation === 1 && (minor >= 21 || (minor === 20 && patch >= 5))) return 21;
  if (minor >= 18) return 17;
  if (minor === 17) return 16;
  return minecraftVersion === "1.16.5" && versionId.startsWith("fabric-loader-") ? 17 : 8;
}

function fabricLoaderNeedsUpgrade(versionId: string, minecraftVersion: string) {
  if (!versionId.startsWith("fabric-loader-")) return false;
  const match = versionId.match(/^fabric-loader-(\d+)\.(\d+)\.(\d+)(?:[-+]|$)/);
  if (!match) return true;
  const current = match.slice(1, 4).map(Number);
  const target = activeMods.targets.find((item) => item.minecraft === minecraftVersion && item.loader === "fabric");
  if (!target) return false;
  const minimum = target.loaderVersionRange.slice(2).split(".").map(Number);
  return current.some((component, index) => component !== minimum[index]
    ? component < minimum[index] && current.slice(0, index).every((value, prior) => value === minimum[prior])
    : false);
}

const FALLBACK_JAVA_MAJORS = [8, 11, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26];
const RECOMMENDED_JAVA_MAJORS = [8, 16, 17, 21, 25];

function javaVersionDescription(major: number) {
  if (major === 8) return "Minecraft e Forge legados";
  if (major === 16) return "Mods Fabric 1.16.5 antigos";
  if (major === 17) return "Minecraft 1.18–1.20.4";
  if (major === 21) return "Minecraft 1.20.5–1.21.11";
  if (major === 25) return "Minecraft 26.1–26.3";
  return "Runtime alternativo para mods específicos";
}

function javaMajor(version: string) {
  const match = version.match(/version\s+"?(\d+)(?:\.(\d+))?/i);
  if (!match) return 0;
  return Number(match[1]) === 1 ? Number(match[2] ?? 0) : Number(match[1]);
}

function chooseJavaRuntime(runtimes: JavaRuntime[], minecraftVersion: string) {
  const required = requiredJavaMajor(minecraftVersion);
  return runtimes.find((runtime) => javaMajor(runtime.version) === required) ?? runtimes[0];
}

function safeInstanceId(value: string) {
  const base = value
    .toLowerCase()
    .replace(/[^a-z0-9_.-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 52);
  return base || "modpack";
}

function preferredPackVersion(pack: CatalogPack | ModrinthPack, fallback: string) {
  return pack.versions.includes(fallback) ? fallback : pack.versions[0] ?? fallback;
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function catalogCategoryKey(source: CatalogSource, contentType: ContentType | "modpack") {
  return `${source}:${contentType}`;
}

function curseForgeProjectToPack(project: CurseForgeProject): CatalogPack {
  const loaderIndexes = project.latestFilesIndexes ?? [];
  const supportedIndexes = loaderIndexes.filter((entry) => entry.modLoader === 1 || entry.modLoader === 4);
  return {
    source: "curseforge",
    curseForgeId: project.id,
    projectId: String(project.id),
    slug: project.slug,
    title: project.name,
    description: project.summary ?? "",
    versions: [...new Set(supportedIndexes.map((entry) => entry.gameVersion).filter((value): value is string => Boolean(value)))],
    loaders: [...new Set(supportedIndexes.map((entry) => entry.modLoader === 1 ? "forge" : "fabric"))],
    curseForgeHasOnlyUnsupportedLoaders: loaderIndexes.length > 0 && supportedIndexes.length === 0,
    iconUrl: project.logo?.thumbnailUrl ?? project.logo?.url,
    downloads: project.downloadCount ?? 0,
    follows: 0,
    author: project.authors?.[0]?.name ?? "",
    dateModified: project.dateModified ?? "",
    websiteUrl: project.links?.websiteUrl,
    gallery: (project.screenshots ?? []).map((image) => image.url ?? image.thumbnailUrl ?? "").filter(Boolean),
  };
}

function curseForgeProjectContentType(project: CurseForgeProject | undefined, fallback: ContentType): ContentType {
  if (project?.classId === 6) return "mod";
  if (project?.classId === 6552) return "shader";
  if (project?.classId === 12) return "resourcepack";
  return fallback;
}

function AuthScreen({
  authenticate,
  services,
  independent = false,
}: {
  authenticate: (mode: AuthMode, nickname: string, password: string) => Promise<void>;
  services: FirebaseServices;
  independent?: boolean;
}) {
  const [mode, setMode] = useState<AuthMode>("login");
  const [recovering, setRecovering] = useState(false);
  const [nickname, setNickname] = useState("");
  const [password, setPassword] = useState("");
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (mode === "register" && !MINECRAFT_NICK_PATTERN.test(nickname)) {
      setMessage("Use um nick Minecraft válido: 3–16 letras, números ou _. Sem espaços, acentos ou símbolos.");
      return;
    }
    setBusy(true);
    setMessage("");
    try {
      await authenticate(mode, nickname, password);
    } catch (error) {
      setMessage(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  }
  return (
    <main className="auth-shell">
      <section className="auth-card">
        <div className="aurora-mark">✦</div>
        <p className="eyebrow">AURORA SMART LAUNCHER</p>
        <h1>
          {recovering ? "Recuperar senha" : mode === "login" ? "Bem-vindo de volta." : "Crie sua constelação."}
        </h1>
        {recovering ? <PasswordResetForm services={services} back={() => setRecovering(false)} /> : <>
        <p className="lede">{mode === "login" ? "Entre com o e-mail confirmado ou, se ainda não vinculou um e-mail, com seu nick." : "Escolha seu nick no Minecraft. Depois de criar a conta, você poderá confirmar um e-mail para recuperar a senha."}</p>
        {independent && <p className="muted">Esta janela tem uma sessão independente. O login desta janela é temporário e termina quando você encerra este launcher.</p>}
        <div className="auth-tabs">
          <button
            className={mode === "login" ? "selected" : "secondary"}
            onClick={() => { setMode("login"); setMessage(""); setPassword(""); }}
            disabled={busy}
            type="button"
          >
            Entrar
          </button>
          <button
            className={mode === "register" ? "selected" : "secondary"}
            onClick={() => { setMode("register"); setNickname(""); setMessage(""); setPassword(""); }}
            disabled={busy}
            type="button"
          >
            Criar conta
          </button>
        </div>
        <form onSubmit={submit} className="auth-form">
          <label>
            {mode === "login" ? "Nick ou e-mail confirmado" : "Nick"}
            <input
              value={nickname}
              onChange={(event) => {
                if (mode === "login") { setNickname(event.target.value); return; }
                const next = sanitizeMinecraftNickname(event.target.value);
                setNickname(next);
                if (next !== event.target.value) {
                  setMessage("O nick aceita somente letras, números e _. Espaços e símbolos foram removidos.");
                }
              }}
              placeholder="Aurora_Player"
              autoComplete="username"
              minLength={3}
              maxLength={mode === "login" ? 254 : 16}
              pattern={mode === "register" ? "[A-Za-z0-9_]{3,16}" : undefined}
              title={mode === "register" ? "3–16 letras, números ou _." : undefined}
              required
            />
            {mode === "register" && <small className="field-hint">Esse será exatamente o nome usado dentro do Minecraft.</small>}
          </label>
          <label>
            Senha
            <input
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              placeholder="Mínimo de 8 caracteres"
              type="password"
              autoComplete={
                mode === "login" ? "current-password" : "new-password"
              }
              minLength={8}
              required
            />
          </label>
          {message && <><p className="form-error">{message}</p><SupportContextButton source="Launcher" context={mode === "login" ? "Falha ao entrar na conta Aurora." : "Falha ao criar a conta Aurora."} log={message} /></>}
          <button disabled={busy} type="submit">
            {busy
              ? "Aguarde…"
              : mode === "login"
                ? "Entrar no Aurora"
                : "Criar conta Aurora"}
          </button>
        </form>
        {mode === "login" && <button type="button" disabled={busy} className="text-button" onClick={() => { setPassword(""); setMessage(""); setRecovering(true); }}>Esqueci minha senha</button>}
        </>}
      </section>
    </main>
  );
}


function DeleteInstanceDialog({ instance, onCancel, onConfirm, disabled }: { instance: Instance; onCancel: () => void; onConfirm: () => void; disabled: boolean }) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const dialog = ref.current!;
    dialog.showModal();
    return () => dialog.close();
  }, []);
  return <dialog className="instance-delete-dialog theme-protected" ref={ref} aria-labelledby="delete-instance-title"
    onCancel={event => { event.preventDefault(); onCancel(); }}>
    <h2 id="delete-instance-title">Excluir {instance.displayName || instance.id}?</h2>
    <p>A exclusão remove os arquivos desta instância, os mundos locais e seus backups.</p>
    <div className="form-actions">
      <button autoFocus className="secondary compact" onClick={onCancel} type="button">Cancelar</button>
      <button className="danger compact" disabled={disabled} onClick={onConfirm} type="button">Excluir instância</button>
    </div>
  </dialog>;
}
function LauncherStartup() {
  const [services, setServices] = useState<FirebaseServices>();
  const [error, setError] = useState("");
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let disposed = false;
    prepareLauncherServices({
      loadScope: () => isTauri() ? invoke("launcher_auth_scope") : Promise.resolve(browserLauncherAuthScope()),
      loadConfig: () => invoke<FirebasePublicConfig | null>("load_firebase_config"),
      bundledConfig: bundledFirebaseConfig,
      create: createFirebaseServices,
    }).then((ready) => { if (!disposed) setServices(ready); })
      .catch(() => { if (!disposed) setError("Não foi possível preparar a sessão desta janela. Tente novamente."); });
    return () => { disposed = true; };
  }, [attempt]);
  if (error) return <StartupScreen error={error}
    onRetry={() => { setError(""); setAttempt((value) => value + 1); }} />;
  if (!services) return <StartupScreen />;
  return <LauncherApp services={services} />;
}

function LauncherApp({ services }: { services: FirebaseServices }) {
  const themes = useThemes();
  const [themesOpen, setThemesOpen] = useState(false);
  const [manualConnectRequested, setManualConnectRequested] = useState(false);
  const [accountProfile, setProfileState] = useState<AuroraUserProfile>();
  const workerUrl = services.config.workerUrl || PUBLIC_SKIN_WORKER;
  const retiredProfileSkin = isRetiredSkinSource(accountProfile?.skinUrl, workerUrl);
  const profile = useMemo(() => {
    if (!accountProfile) return undefined;
    const retiredAvatar = isRetiredSkinSource(accountProfile.avatarUrl, workerUrl);
    if (!retiredProfileSkin && !retiredAvatar) return accountProfile;
    return { ...accountProfile, skinUrl: retiredProfileSkin ? null : accountProfile.skinUrl,
      avatarUrl: retiredAvatar ? null : accountProfile.avatarUrl };
  }, [accountProfile, retiredProfileSkin, workerUrl]);
  function setProfile(next: AuroraUserProfile | undefined) {
    setProfileState((current) => guardSessionProfile(current, next, services.auth.currentUser?.uid));
  }
  const [sessionStartup, setSessionStartup] = useState<SessionStartupState<AuroraUserProfile>>({ phase: "loading" });
  const [sessionRetry, setSessionRetry] = useState(0);
  const [recoverySettingsUid, setRecoverySettingsUid] = useState<string>();
  const sessionController = useRef<ReturnType<typeof startSessionStartup> | undefined>(undefined);
  const [connectAccountBarrier, setConnectAccountBarrier] = useState<"idle" | "closing" | "blocked">("idle");
  const socialConnectOwner = useRef<SocialConnectController | undefined>(undefined);
  const [socialConnectRetry, setSocialConnectRetry] = useState(0);
  const [socialConnectState, setSocialConnectState] = useState<{ accountId: string; value: SocialConnectState }>({ accountId: "", value: emptySocialConnectState() });
  const accountTransition = useRef<ReturnType<typeof createAccountTransition<AuroraUserProfile>> | undefined>(undefined);
  if (!accountTransition.current) {
    accountTransition.current = createAccountTransition<AuroraUserProfile>({
      publish: (state) => { setSessionStartup(state); setProfile(state.profile); },
      closeConnect: async () => { socialConnectOwner.current?.invalidate(); if (isTauri()) await stopConnectForAccountTransition(invoke); },
      onBarrier: setConnectAccountBarrier,
    });
  }
  const [status, setStatus] = useState<EngineStatus>();
  const [instances, setInstances] = useState<Instance[]>([]);
  const [instanceName, setInstanceName] = useState("");
  const [targetInstance, setTargetInstance] = useState("");
  useEffect(() => { themes.setInstance(targetInstance); }, [targetInstance, themes.setInstance]);
  const [minecraftVersion, setMinecraftVersion] = useState("1.20.1");
  const [loaderChoice, setLoaderChoice] = useState<LoaderChoice>("fabric");
  const [installedVersionId, setInstalledVersionId] = useState("");
  const [installedMinecraftVersion, setInstalledMinecraftVersion] = useState("");
  const [modpackQuery, setModpackQuery] = useState("");
  const [modrinthPacks, setModrinthPacks] = useState<CatalogPack[]>([]);
  const [modrinthTotalHits, setModrinthTotalHits] = useState(0);
  const [modrinthPage, setModrinthPage] = useState(0);
  const [modrinthPageSize, setModrinthPageSize] = useState(10);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [catalogError, setCatalogError] = useState("");
  const catalogRequest = useRef(0);
  const [modpackVersionFilter, setModpackVersionFilter] = useState(minecraftVersion);
  const [modpackLoaderFilter, setModpackLoaderFilter] = useState<"" | LoaderChoice>("");
  const [modpackCategoryFilter, setModpackCategoryFilter] = useState("");
  const [modpackSort, setModpackSort] = useState<ContentSort>("popular");
  const [packVersions, setPackVersions] = useState<Record<string, string>>({});
  const [editingInstance, setEditingInstance] = useState("");
  const [contentType, setContentType] = useState<ContentType>("mod");
  const [editorView, setEditorView] = useState<EditorView>("installed");
  const [doctorRunId, setDoctorRunId] = useState<string>();
  const [doctorRecoveryPointId, setDoctorRecoveryPointId] = useState<string>();
  const [doctorFailure, setDoctorFailure] = useState<{ instanceId: string; runId?: string }>();
  const [installedContentFilter, setInstalledContentFilter] = useState("");
  const [contentArtwork, setContentArtwork] = useState<Record<string, ContentArtworkEntry>>({});
  const [contentQuery, setContentQuery] = useState("");
  const [contentResults, setContentResults] = useState<CatalogContent[]>([]);
  const [contentSource, setContentSource] = useState<CatalogSource>("modrinth");
  const [contentSort, setContentSort] = useState<ContentSort>("popular");
  const [contentVersionFilter, setContentVersionFilter] = useState("");
  const [contentLoaderFilter, setContentLoaderFilter] = useState<"" | LoaderChoice>("");
  const [contentCategoryFilter, setContentCategoryFilter] = useState("");
  const [catalogCategories, setCatalogCategories] = useState<Record<string, CatalogCategoryOption[]>>({});
  const [catalogCategoryErrors, setCatalogCategoryErrors] = useState<Record<string, string>>({});
  const catalogCategoryCache = useRef<Record<string, CatalogCategoryOption[]>>({});
  const catalogCategoryRequests = useRef(new Set<string>());
  const [projectDetail, setProjectDetail] = useState<ProjectDetail>();
  const [detailLoading, setDetailLoading] = useState(false);
  const projectDetailRequestId = useRef(0);
  const [instanceContent, setInstanceContent] = useState<InstanceContent>({ mods: [], shaderpacks: [], resourcepacks: [] });
  const [selectedContentNames, setSelectedContentNames] = useState<string[]>([]);
  const [contentUpdateScans, setContentUpdateScans] = useState<Partial<Record<ContentType, ContentUpdateScanView>>>({});
  const [selectedUpdateFilenames, setSelectedUpdateFilenames] = useState<string[]>([]);
  const [contentUpdateFilter, setContentUpdateFilter] = useState<ContentUpdateFilter>("all");
  const [contentUpdateQuery, setContentUpdateQuery] = useState("");
  const [updatesScanning, setUpdatesScanning] = useState(false);
  const [updatesApplying, setUpdatesApplying] = useState(false);
  const [lastBackupAvailable, setLastBackupAvailable] = useState(false);
  const [modpackUpdate, setModpackUpdate] = useState<ModpackUpdate | null>();
  const [modpackScanError, setModpackScanError] = useState("");
  const [updateReview, setUpdateReview] = useState<UpdateReviewRequest>();
  const [updateReviewError, setUpdateReviewError] = useState("");
  const [javaPath, setJavaPath] = useState("");
  const [javaRuntimes, setJavaRuntimes] = useState<JavaRuntime[]>([]);
  const [javaInstallOpen, setJavaInstallOpen] = useState(false);
  const [installingJavaMajor, setInstallingJavaMajor] = useState<number>();
  const [javaInstallMajor, setJavaInstallMajor] = useState(16);
  const [availableJavaMajors, setAvailableJavaMajors] = useState(FALLBACK_JAVA_MAJORS);
  const [javaLtsMajors, setJavaLtsMajors] = useState([8, 11, 17, 21, 25]);
  const [notice, setNotice] = useState("Preparando o núcleo Aurora…");
  const [noticeHovered, setNoticeHovered] = useState(false);
  const [downloadProgress, setDownloadProgress] = useState<DownloadProgress>();
  const [coreConnection, setCoreConnection] = useState<CoreConnection>();
  const [skinUrl, setSkinUrl] = useState("");
  const capeWardrobe = useWardrobeCape(services, profile, (owner, url) => {
    setProfileState(current => current?.uid === owner ? { ...current, capeUrl: url } : current);
  });
  const [skinModel, setSkinModel] = useState<"classic" | "slim">("classic");
  const [favoriteName, setFavoriteName] = useState("");
  const [favoriteSearch, setFavoriteSearch] = useState("");
  const [skinAddMode, setSkinAddMode] = useState<SkinAddMode>("file");
  const [importSkinName, setImportSkinName] = useState("");
  const [importSkinUrl, setImportSkinUrl] = useState("");
  const [localSkins, setLocalSkins] = useState<LocalSkinView[]>([]);
  const [selectedLocalSkinId, setSelectedSkinIdState] = useState("");
  const selectedSkinIdRef = useRef("");
  function setSelectedLocalSkinId(id: string) { selectedSkinIdRef.current = id; setSelectedSkinIdState(id); }
  const [localSkinPath, setLocalSkinPath] = useState("");
  const [equippedSkin, setEquippedSkin] = useState<EquippedSkinView>();
  const [wardrobeView, setWardrobeView] = useState<"explore" | "library" | "import" | "equipped" | "cape">("explore");
  const [wardrobeLoading, setWardrobeLoading] = useState(false);
  const [wardrobeReadyOwner, setWardrobeReadyOwner] = useState("");
  const [wardrobeRetry, setWardrobeRetry] = useState(0);
  const skinOwnerRef = useRef("");
  const libraryLoadGeneration = useRef(0);
  const [instanceFilter, setInstanceFilter] = useState("");
  const [activeSection, setActiveSection] = useState<LauncherSection>(socialFeatureBuild ? "social" : "instances");
  const baseSocialService = useMemo(() => socialFeatureBuild && profile && services.auth.currentUser?.uid === profile.uid
    ? createSocialFirebaseService(services, { privateOnlineCapability: () => socialConnectOwner.current?.supportsPrivateInvitations() === true }) : null, [services, profile?.uid, services.auth.currentUser]);
  useEffect(() => {
    const user = services.auth.currentUser, accountId = profile?.uid;
    setSocialConnectState({ accountId: accountId ?? "", value: emptySocialConnectState() });
    if (!socialFeatureBuild || !isTauri() || !accountId || user?.uid !== accountId || connectAccountBarrier !== "idle") return;
    const controller = createSocialConnectController({ accountId, user, currentUser: () => services.auth.currentUser, invoke,
      publish: value => setSocialConnectState({ accountId, value }) });
    socialConnectOwner.current = controller;
    void controller.start();
    return () => { controller.invalidate(); if (socialConnectOwner.current === controller) socialConnectOwner.current = undefined; };
  }, [services, services.auth.currentUser, profile?.uid, connectAccountBarrier, socialConnectRetry]);
  const socialConnection = profile && socialConnectState.accountId === profile.uid && socialConnectOwner.current
    ? { state: socialConnectState.value, controller: socialConnectOwner.current } : undefined;
  const socialService = useMemo(() => {
    if (!baseSocialService) return null;
    const controller = socialConnectOwner.current, user = services.auth.currentUser;
    return { ...baseSocialService,
      privateInvitations: baseSocialService.privateInvitations?.environment === "local" || controller?.supportsPrivateInvitations() === true
        ? baseSocialService.privateInvitations : undefined,
      capabilities: { ...baseSocialService.capabilities, join: !!controller && socialConnectState.accountId === profile?.uid && socialConnectState.value.available },
      prepareJoin: async (sessionId: string) => {
        // A detached card must never redirect its action to a later account's
        // controller, including a new login under the same public UID.
        if (!controller || socialConnectOwner.current !== controller || services.auth.currentUser !== user) throw { code: "signed_out" };
        await controller.join(sessionId);
      },
    };
  }, [baseSocialService, services, profile?.uid, socialConnectState.accountId, socialConnectState.value.available, socialConnectState.value.privateInvitations]);
  const [connectVisited, setConnectVisited] = useState(false);
  const [dedicatedRevealRequest, setDedicatedRevealRequest] = useState(0);
  const [gameRevealRequest, setGameRevealRequest] = useState(0);
  const [socialPlay, setSocialPlay] = useState<SocialPlayRequest>();
  const [connectViewReady, setConnectViewReady] = useState(false);
  useEffect(() => { if (activeSection === "connect") setConnectVisited(true); }, [activeSection]);
  const [connectAvailable, setConnectAvailable] = useState(false);
  const voice = useVoiceAnnouncement();
  const [signingOut, setSigningOut] = useState(false);
  useEffect(() => { if (activeSection !== "connect" || signingOut) setSocialPlay(undefined); }, [activeSection, signingOut, profile?.uid]);
  const friendInbox = useFriendInbox(signingOut ? null : baseSocialService);
  const [friendInboxReveal, setFriendInboxReveal] = useState(0);
  const signOutPending = useRef(false);
  useEffect(() => {
    if (!isTauri()) return;
    let active = true;
    void invoke("connect_capabilities").then(capabilitiesFrom).then((capabilities) => {
      if (active) setConnectAvailable(capabilities.available);
    }).catch(() => { if (active) setConnectAvailable(false); });
    return () => { active = false; };
  }, []);
  async function signOutWithConnectCleanup() {
    if (signOutPending.current) return;
    signOutPending.current = true;
    socialConnectOwner.current?.invalidate();
    setSigningOut(true);
    try {
      await withVoiceAnnouncementLogout(voice, async () => {
        if (isTauri()) await stopConnectForAccountTransition(invoke);
        await logoutAuroraUser(services);
        setProfile(undefined);
        setSessionStartup({ phase: "signed-out" });
      });
    } catch {
      const message = "Não foi possível concluir a saída. Confira a conexão e o anúncio de voz antes de tentar novamente.";
      setSocialConnectRetry(value => value + 1);
      setNotice(message);
      if (sessionStartup.phase === "error") setSessionStartup({ phase: "error", error: message });
    } finally {
      signOutPending.current = false;
      setSigningOut(false);
    }
  }
  const [catalogSource, setCatalogSource] = useState<CatalogSource>("modrinth");
  const [showCreateModal, setShowCreateModal] = useState(false);

  const [runningInstances, setRunningInstances] = useState<RunningInstance[]>([]);
  const inviteAccount = socialService && !socialLabBuild && !signingOut && connectAccountBarrier === "idle" ? profile?.uid : undefined;
  const revealSocial = () => { setThemesOpen(false); setActiveSection("social"); };
  const discord = useDiscordPresence(inviteAccount, runningInstances, socialConnection?.state, revealSocial);
  const publicInvite = usePublicInvite(inviteAccount, revealSocial);
  const [deleteCandidate, setDeleteCandidate] = useState<Instance>();
  const [renameDraft, setRenameDraft] = useState("");
  const [launcherBusy, setBusy] = useState(false);
  const [recoveryBusy, setRecoveryBusy] = useState(false);
  const recoveryBusyRef = useRef(false);
  const busy = launcherBusy || recoveryBusy || !!voice?.blocking;
  const avatarMigrationRef = useRef("");
  const localAvatarMigrationRef = useRef("");
  const appearanceMigrationPending = useRef<Promise<void>>(Promise.resolve());
  const activeInstanceRef = useRef({ editingInstance, targetInstance });
  const editorPanelRef = useRef<HTMLElement>(null);
  const editorOpener = useRef<HTMLElement | null>(null);
  useEffect(() => {
    if (!editingInstance) return;
    const panel = editorPanelRef.current;
    panel?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')?.focus({ preventScroll: true });
    return () => {
      if (editorOpener.current?.isConnected) editorOpener.current.focus({ preventScroll: true });
    };
  }, [editingInstance]);
  const editorOpenRequestRef = useRef({ generation: 0, id: "" });
  const lastArtworkMigrationRef = useRef<{ id: string; migrations: ContentArtworkMigration[] } | null>(null);
  const updateScanRequestRef = useRef({ generation: 0, id: "" });
  const curseForgeUpdateCacheRef = useRef(new Map<string, CurseForgeUpdatesResponse["data"][number]>());
  const curseForgeCompletedScansRef = useRef(new Set<string>());
  const [externalDownloadQueue] = useState(() => new ExternalDownloadQueue({
    open: (request) => invoke("open_manual_content_download", {
      ...request,
      projectId: request.projectId ?? null,
      fileId: request.fileId ?? null,
    }),
    cancel: (requestId) => invoke("cancel_manual_content_download", { requestId }),
    onStart: ({ filename }) => setNotice(`Buscando download automático de “${filename}”… Se o site pedir confirmação, conclua na página.`),
  }));
  useEffect(() => {
    activeInstanceRef.current = { editingInstance, targetInstance };
  }, [editingInstance, targetInstance]);
  function clearContentUpdatePlan() {
    setContentUpdateScans({});
    setSelectedUpdateFilenames([]);
    setModpackUpdate(undefined);
    setModpackScanError("");
    setUpdateReview(undefined);
    setUpdateReviewError("");
  }
  function invalidateContentUpdatePlan() {
    updateScanRequestRef.current = {
      generation: updateScanRequestRef.current.generation + 1,
      id: "",
    };
    setUpdatesScanning(false);
    clearContentUpdatePlan();
  }
  function closeInstanceEditor() {
    if (recoveryBusyRef.current || voice?.isBlocking()) return;
    editorOpenRequestRef.current = { generation: editorOpenRequestRef.current.generation + 1, id: "" };
    invalidateContentUpdatePlan();
    activeInstanceRef.current = { ...activeInstanceRef.current, editingInstance: "" };
    setEditingInstance("");
  }
  function setRecoveryOperation(active: boolean) {
    recoveryBusyRef.current = active;
    setRecoveryBusy(active);
  }
  async function refreshAfterRecovery() {
    const id = editingInstance;
    const generation = editorOpenRequestRef.current.generation;
    const [content, saved, nextInstances] = await Promise.all([
      invoke<InstanceContent>("list_instance_content", { id }),
      invoke<InstanceLaunchProfile>("read_instance_launch_profile", { id }),
      invoke<Instance[]>("list_instances"),
    ]);
    if (activeInstanceRef.current.editingInstance !== id || editorOpenRequestRef.current.generation !== generation) return;
    invalidateContentUpdatePlan();
    setSelectedContentNames([]);
    setInstanceContent(content);
    setInstances(nextInstances);
    setInstalledVersionId(saved.versionId ?? "");
    setInstalledMinecraftVersion(saved.minecraftVersion ?? "");
    if (saved.minecraftVersion) setMinecraftVersion(saved.minecraftVersion);
  }
  useEffect(() => voice?.registerNavigator((id, target) => {
    setActiveSection("instances");
    void openEditor(id, target === "recovery" ? "recovery" : "diagnostic-lab");
  }));
  useEffect(() => {
    if (!editingInstance) return undefined;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.key !== "Escape" || updatesApplying || updateReview || projectDetail || document.querySelector("dialog:modal")) return;
      const options = editorPanelRef.current?.querySelector<HTMLDetailsElement>("details.editor-options[open]");
      if (options) {
        options.open = false;
        options.querySelector("summary")?.focus();
        event.preventDefault();
        return;
      }
      event.preventDefault();
      if (!busy) closeInstanceEditor();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [busy, editingInstance, projectDetail, updateReview, updatesApplying]);
  async function refresh() {
    const [nextStatus, nextInstances] = await Promise.all([
      invoke<EngineStatus>("engine_status"),
      invoke<Instance[]>("list_instances"),
    ]);
    setStatus(nextStatus);
    setInstances(nextInstances);
    void hydrateInstanceArtwork(nextInstances);
  }
  async function hydrateInstanceArtwork(nextInstances: Instance[]) {
    const missingArtwork = nextInstances.filter((instance) => instance.projectId && !instance.iconUrl);
    if (missingArtwork.length === 0) return;
    const hydrated = await Promise.all(missingArtwork.map(async (instance) => {
      try {
        const response = await fetch(`https://api.modrinth.com/v2/project/${encodeURIComponent(instance.projectId!)}`);
        if (!response.ok) return undefined;
        const project = await response.json() as { title?: string; icon_url?: string };
        return await invoke<Instance>("set_instance_presentation", {
          id: instance.id,
          displayName: project.title?.trim() || instance.displayName || instance.id,
          iconUrl: safeHttpsUrl(project.icon_url) || null,
        });
      } catch {
        return undefined;
      }
    }));
    const updates = new Map(hydrated.filter(Boolean).map((instance) => [instance!.id, instance!]));
    if (updates.size > 0) {
      setInstances((current) => current.map((instance) => updates.get(instance.id) ?? instance));
    }
  }
  function isCurrentSkinOwner(ownerId: string) {
    return skinOwnerRef.current === ownerId && services.auth.currentUser?.uid === ownerId;
  }
  async function refreshLocalSkinLibrary(ownerId: string, selectId?: string) {
    if (!isCurrentSkinOwner(ownerId)) return [];
    const generation = ++libraryLoadGeneration.current;
    const records = await listLocalSkins(ownerId, services.config?.workerUrl);
    const views = records.map((skin) => {
      const previewUrl = skin.image ? URL.createObjectURL(skin.image) : skin.sourceUrl ?? "";
      return { ...skin, previewUrl };
    });
    if (!isCurrentSkinOwner(ownerId) || generation !== libraryLoadGeneration.current) {
      views.forEach((skin) => { if (skin.image) URL.revokeObjectURL(skin.previewUrl); });
      return [];
    }
    setLocalSkins(views);
    const selected = views.find((skin) => skin.id === (selectId ?? selectedSkinIdRef.current));
    if (selected) {
      setSelectedLocalSkinId(selected.id);
      setSkinUrl(selected.previewUrl);
    }
    return views;
  }
  async function appearanceBlob(dataBase64: string) {
    const blob = await (await fetch(dataBase64)).blob();
    await validateAppearanceFile("skin", new File([blob], "skin.png", { type: "image/png" }));
    return blob;
  }
  async function cacheSkin(skin: LocalSkin) {
    if (skin.image) return skin;
    if (!skin.sourceUrl) throw new Error("O PNG desta skin não está salvo. Importe o arquivo novamente.");
    const loaded = await invoke<AppearanceImage>("load_appearance_url", { url: skin.sourceUrl, kind: "skin" });
    const image = await appearanceBlob(loaded.dataBase64);
    if (!isCurrentSkinOwner(skin.ownerId)) return undefined;
    return cacheLocalSkinImage(skin.ownerId, skin.id, image, loaded.url);
  }
  function rememberEquippedSkin(ownerId: string, skin: LocalSkin, model: "classic" | "slim", previewUrl: string, path: string) {
    if (!isCurrentSkinOwner(ownerId) || !skin.image) return;
    window.localStorage.setItem(`aurora.equippedSkin.${ownerId}`, JSON.stringify({ id: skin.id, skinModel: model }));
    window.localStorage.setItem(`aurora.localSkinPath.${ownerId}`, path);
    setLocalSkinPath(path);
    setEquippedSkin({ id: skin.id, name: skin.name, skinModel: model, previewUrl, image: skin.image });
  }
  useEffect(() => {
    const savedJava = window.localStorage.getItem("aurora.javaExecutable");
    if (savedJava) setJavaPath(savedJava);
    invoke<JavaRuntime[]>("discover_java")
      .then((runtimes) => {
        setJavaRuntimes(runtimes);
        const selected = savedJava
          ? runtimes.find((runtime) => runtime.executable === savedJava)
          : chooseJavaRuntime(runtimes, minecraftVersion);
        if (!savedJava && selected) {
          setJavaPath(selected.executable);
          window.localStorage.setItem("aurora.javaExecutable", selected.executable);
        }
      })
      .catch(() => undefined);
    invoke<JavaReleaseCatalog>("available_java_versions")
      .then((catalog) => {
        if (catalog.available.length === 0) return;
        setAvailableJavaMajors(catalog.available);
        setJavaLtsMajors(catalog.lts);
        setJavaInstallMajor((current) => catalog.available.includes(current)
          ? current
          : catalog.available.includes(16) ? 16 : catalog.available[0]);
      })
      .catch(() => undefined);
  }, []);
  useEffect(() => {
    let disposed = false;
    let stopListening: (() => void) | undefined;
    void listen<DoctorRun>("aurora-doctor-run", (event) => {
      if (disposed || event.payload.outcome !== "failed") return;
      setDoctorFailure({ instanceId: event.payload.instanceId, runId: event.payload.runId });
    }).then((unlisten) => {
      if (disposed) unlisten();
      else stopListening = unlisten;
    }).catch(() => {
      // Manual diagnosis and persisted run history remain available if an event
      // listener cannot be registered. Normal exits never become crash notices.
    });
    return () => { disposed = true; stopListening?.(); };
  }, []);
  useEffect(() => observeDownloadProgress(listen, setDownloadProgress), []);
  useEffect(() => {
    let disposed = false;
    let stopListening: (() => void) | undefined;
    void listen<ExternalDownloadEvent>("aurora-manual-download", (event) => {
      if (disposed) return;
      const result = event.payload;
      if (!externalDownloadQueue.accept(result)) return;
      if (result.status === "resolving") {
        setNotice(`Buscando download automático de “${result.filename}”… Se o site pedir confirmação, conclua na página.`);
        return;
      }
      if (result.status === "downloading") {
        setNotice(`Recebendo “${result.filename}” dentro do Aurora…`);
        return;
      }
      setDownloadProgress(undefined);
      if (result.status === "failed") {
        setNotice(`O download interno de “${result.filename}” falhou: ${result.error ?? "arquivo não validado"}`);
        return;
      }
      setNotice(`“${result.filename}” foi validado e instalado automaticamente.`);
      void refresh();
      if (result.contentType !== "modpack" && activeInstanceRef.current.editingInstance === result.instanceId) {
        void invoke<InstanceContent>("list_instance_content", { id: result.instanceId })
          .then((content) => {
            if (activeInstanceRef.current.editingInstance === result.instanceId) {
              setInstanceContent(content);
              setEditorView("installed");
            }
          })
          .catch((error: unknown) => setNotice(`O arquivo foi instalado, mas a lista não pôde ser atualizada: ${String(error)}`));
      }
    }).then((unlisten) => {
      if (disposed) unlisten();
      else stopListening = unlisten;
    });
    return () => {
      disposed = true;
      stopListening?.();
      void externalDownloadQueue.cancelAll();
    };
  }, [externalDownloadQueue]);
  useEffect(() => {
    if (!notice || noticeHovered || busy) return;
    const timer = window.setTimeout(() => setNotice(""), 6500);
    return () => window.clearTimeout(timer);
  }, [notice, noticeHovered, busy]);
  useEffect(() => {
    const controller = startSessionStartup({
      observe: (next, error) => observeAuroraSession(services, next, error),
      currentUser: () => services.auth.currentUser,
      loadProfile: (user) => loadAuroraProfile(services, user),
      onState: (state) => {
        accountTransition.current?.accept(state);
      },
    });
    sessionController.current = controller;
    return () => {
      controller.dispose();
      if (sessionController.current === controller) sessionController.current = undefined;
    };
  }, [services, sessionRetry]);
  useEffect(() => {
    if (!profile || !services) return;
    void syncAuroraPublicProfile(services, profile).catch(() => undefined);
  }, [services, profile]);
  const accountId = profile?.uid;
  useEffect(() => {
    if (!accountId) return;
    let active = true;
    refresh()
      .then(() => {
        if (active) setNotice("Núcleo pronto.");
      })
      .catch((error: unknown) => {
        if (active) setNotice(`Não foi possível iniciar o Aurora: ${String(error)}`);
      });
    return () => { active = false; };
  }, [accountId]);
  useEffect(() => {
    if (!profile?.skinUrl || profile.avatarUrl === profile.skinUrl || !services || busy || wardrobeLoading || wardrobeReadyOwner !== profile.uid) return;
    const migrationKey = `${profile.uid}:${profile.skinUrl}`;
    if (avatarMigrationRef.current === migrationKey) return;
    avatarMigrationRef.current = migrationKey;
    let cancelled = false;
    const migrate = appearanceMigrationPending.current.then(async () => {
      try {
        if (cancelled || !isCurrentSkinOwner(profile.uid)) return;
        const next = await saveAuroraAppearance(services, profile, {
          avatarUrl: profile.skinUrl,
          skinUrl: profile.skinUrl,
          skinModel: profile.skinModel,
        });
        if (!cancelled && isCurrentSkinOwner(profile.uid)) setProfileState(current => current?.uid === next.uid ? { ...next, capeUrl: current.capeUrl } : current);
      } catch {
        // O perfil continua utilizável; uma nova tentativa ocorre quando a skin for equipada novamente.
      }
    });
    appearanceMigrationPending.current = migrate;
    return () => { cancelled = true; };
  }, [profile?.uid, profile?.skinUrl, profile?.avatarUrl, services, busy, wardrobeLoading, wardrobeReadyOwner]);
  useEffect(() => {
    if (!profile || profile.avatarUrl || profile.skinUrl || !localSkinPath || !services || busy || wardrobeLoading || wardrobeReadyOwner !== profile.uid) return;
    const migrationKey = `${profile.uid}:${localSkinPath}`;
    if (localAvatarMigrationRef.current === migrationKey) return;
    localAvatarMigrationRef.current = migrationKey;
    let cancelled = false;
    const migrate = appearanceMigrationPending.current.then(async () => {
      try {
        if (cancelled || !isCurrentSkinOwner(profile.uid)) return;
        const loaded = await invoke<AppearanceImage>("load_local_appearance", { userId: profile.uid, kind: "skin" });
        const blob = await (await fetch(loaded.dataBase64)).blob();
        if (cancelled || !isCurrentSkinOwner(profile.uid)) return;
        const skinUrl = await uploadAppearanceImage("skin", new File([blob], "skin.png", { type: "image/png" }));
        if (cancelled || !isCurrentSkinOwner(profile.uid)) return;
        const next = await saveAuroraAppearance(services, profile, {
          avatarUrl: skinUrl,
          skinUrl,
          skinModel: equippedSkin?.skinModel ?? profile.skinModel,
        });
        if (!cancelled && isCurrentSkinOwner(profile.uid)) setProfileState(current => current?.uid === next.uid ? { ...next, capeUrl: current.capeUrl } : current);
      } catch {
        // A skin continua equipada localmente; o usuário pode sincronizá-la novamente no guarda-roupa.
      }
    });
    appearanceMigrationPending.current = migrate;
    return () => { cancelled = true; };
  }, [profile?.uid, profile?.avatarUrl, profile?.skinUrl, localSkinPath, services, busy, wardrobeLoading, wardrobeReadyOwner]);
  useEffect(() => {
    ++libraryLoadGeneration.current;
    skinOwnerRef.current = profile?.uid ?? "";
    setWardrobeReadyOwner("");
    setSelectedLocalSkinId("");
    setLocalSkins([]);
    setEquippedSkin(undefined);
    setLocalSkinPath("");
    setSkinUrl("");
    setSkinModel(profile?.skinModel ?? "classic");
    setFavoriteName("");
    setFavoriteSearch("");
    setWardrobeView("explore");
    setSkinAddMode("file");
    setImportSkinName("");
    setImportSkinUrl("");
    if (!profile) { setWardrobeLoading(false); return; }
    const ownerId = profile.uid;
    let cancelled = false;
    setWardrobeLoading(true);
    void (async () => {
      let stored = await listLocalSkins(ownerId, workerUrl);
      if (cancelled || !isCurrentSkinOwner(ownerId)) return;
      const knownUrls = new Set(stored.map((skin) => skin.sourceUrl).filter(Boolean));
      for (const favorite of profile.skinFavorites) {
        if (isRetiredSkinSource(favorite.skinUrl, workerUrl)) continue;
        if (!knownUrls.has(favorite.skinUrl)) {
          await saveLocalSkin({
            id: favorite.id,
            ownerId,
            name: favorite.name,
            skinModel: favorite.skinModel,
            sourceUrl: favorite.skinUrl,
          });
          knownUrls.add(favorite.skinUrl);
        }
      }
      stored = await listLocalSkins(ownerId, workerUrl);
      if (cancelled || !isCurrentSkinOwner(ownerId)) return;
      await refreshLocalSkinLibrary(ownerId);
      let selection = parseEquippedSkinSelection(window.localStorage.getItem(`aurora.equippedSkin.${ownerId}`));
      let path = window.localStorage.getItem(`aurora.localSkinPath.${ownerId}`) ?? "";
      if (selection ? await isRetiredLocalSkin(ownerId, selection.id, workerUrl) : retiredProfileSkin) {
        if (cancelled || !isCurrentSkinOwner(ownerId)) return;
        window.localStorage.removeItem(`aurora.equippedSkin.${ownerId}`);
        window.localStorage.removeItem(`aurora.localSkinPath.${ownerId}`);
        selection = undefined;
        path = "";
      }
      let equipped: LocalSkin | undefined;
      let png = "";
      if (path || selection) {
        try {
          const loaded = await invoke<AppearanceImage>("load_local_appearance", { userId: ownerId, kind: "skin" });
          const image = await appearanceBlob(loaded.dataBase64);
          if (cancelled || !isCurrentSkinOwner(ownerId)) return;
          equipped = await findMatchingLocalSkin(stored, image, selection?.id);
          if (!equipped) equipped = await saveLocalSkin({ ownerId, name: "Skin equipada recuperada", skinModel: selection?.skinModel ?? profile.skinModel, image });
          png = loaded.dataBase64;
        } catch {
          // Repair a missing native copy from the explicitly equipped library ID.
          path = "";
          equipped = stored.find((skin) => skin.id === selection?.id && skin.image);
          if (equipped?.image) png = await fileAsDataUrl(new File([equipped.image], "skin.png", { type: "image/png" }));
        }
      }
      if (!equipped && profile.skinUrl) {
        try {
          const source = stored.find((skin) => skin.sourceUrl === profile.skinUrl);
          if (source) equipped = await cacheSkin(source);
          else {
            const loaded = await invoke<AppearanceImage>("load_appearance_url", { url: profile.skinUrl, kind: "skin" });
            const image = await appearanceBlob(loaded.dataBase64);
            if (cancelled || !isCurrentSkinOwner(ownerId)) return;
            const matching = await findMatchingLocalSkin(stored, image);
            equipped = matching
              ? await updateLocalSkinSource(ownerId, matching.id, loaded.url)
              : await saveLocalSkin({ ownerId, name: "Skin equipada", skinModel: profile.skinModel, sourceUrl: loaded.url, image });
          }
          if (equipped?.image) png = await fileAsDataUrl(new File([equipped.image], "skin.png", { type: "image/png" }));
        } catch { /* Keep the library usable offline; never replace a failed image with an empty PNG. */ }
      }
      if (cancelled || !isCurrentSkinOwner(ownerId)) return;
      if (equipped?.image && png) {
        if (!path) path = await invoke<string>("save_local_appearance", { userId: ownerId, kind: "skin", dataBase64: png });
        if (cancelled || !isCurrentSkinOwner(ownerId)) return;
        const model = equipped.id === selection?.id ? selection.skinModel : selection ? equipped.skinModel : profile.skinModel;
        rememberEquippedSkin(ownerId, equipped, model, png, path);
        await refreshLocalSkinLibrary(ownerId, equipped.id);
        if (!cancelled && isCurrentSkinOwner(ownerId)) {
          setSkinUrl(png);
          setSkinModel(model);
          setFavoriteName(equipped.name);
        }
      }
      if (!cancelled && isCurrentSkinOwner(ownerId)) {
        setWardrobeReadyOwner(ownerId);
        setWardrobeLoading(false);
      }
      if (profile.skinFavorites.length > 0 && !cancelled) {
        await clearRemoteSkinLibrary(services, profile);
        if (!cancelled && isCurrentSkinOwner(ownerId)) setProfileState((current) => current?.uid === ownerId ? { ...current, skinFavorites: [] } : current);
      }
      // Migrate URL-only entries in the background, with bounded concurrency.
      const pending = (await listLocalSkins(ownerId, workerUrl)).filter((skin) => !skin.image && skin.sourceUrl);
      await Promise.all([0, 1].map(async () => {
        while (pending.length && !cancelled && isCurrentSkinOwner(ownerId)) {
          const skin = pending.shift()!;
          try {
            if (await cacheSkin(skin)) await refreshLocalSkinLibrary(ownerId);
          } catch { /* An unavailable legacy URL remains listed with a recovery message. */ }
        }
      }));
    })().catch((error: unknown) => {
      if (!cancelled && isCurrentSkinOwner(ownerId)) setNotice(`Não foi possível carregar a biblioteca local: ${String(error)}`);
    }).finally(() => {
      if (!cancelled && isCurrentSkinOwner(ownerId)) {
        setWardrobeReadyOwner(ownerId);
        setWardrobeLoading(false);
      }
    });
    return () => { cancelled = true; ++libraryLoadGeneration.current; skinOwnerRef.current = ""; };
  }, [profile?.uid, services, wardrobeRetry]);
  // Revoke only after React has committed the replacement list and preview URL.
  useEffect(() => () => localSkins.forEach((skin) => {
    if (skin.image) URL.revokeObjectURL(skin.previewUrl);
  }), [localSkins]);
  useEffect(() => {
    const refreshRunningInstances = () => {
      invoke<RunningInstance[]>("list_running_instances")
        .then(setRunningInstances)
        .catch(() => undefined);
    };
    refreshRunningInstances();
    const timer = window.setInterval(() => {
      refreshRunningInstances();
      invoke<IpcSessionEvent[]>("poll_ipc_events")
        .then((sessionEvents) => {
          for (const { event } of sessionEvents) {
            if (event.kind === "connected" || event.kind === "disconnected") {
              setCoreConnection(event);
            }
          }
        })
        .catch(() => undefined);
    }, 1_000);
    return () => window.clearInterval(timer);
  }, []);
  async function downloadCurseForgeContentAutomatically(options: {
    id: string;
    pageUrl: string;
    filename: string;
    contentType: ContentType | "modpack";
    sha1: string;
    projectId?: number;
    fileId?: number;
  }) {
    await externalDownloadQueue.enqueue({ ...options, requestId: crypto.randomUUID() });
  }

  async function completeInstanceInstall(id: string, message: string) {
    setTargetInstance(id);
    closeProjectDetail();
    setShowCreateModal(false);
    closeInstanceEditor();
    setInstanceFilter("");
    setActiveSection("instances");
    setDownloadProgress(undefined);
    setNotice(message);
    try {
      await refresh();
    } catch {
      setNotice(message + " A instalação foi concluída, mas a lista não pôde ser atualizada. Reabra o launcher para recarregá-la.");
    }
  }
  async function createInstance(event: FormEvent) {
    event.preventDefault();
    if (!instanceName.trim()) return;
    setBusy(true);
    try {
      const instance = await invoke<Instance>("create_instance", {
        id: instanceName.trim(),
      });
      setInstances((current) =>
        [...current.filter(({ id }) => id !== instance.id), instance].sort(
          (a, b) => a.id.localeCompare(b.id),
        ),
      );
      setTargetInstance(instance.id);
      setInstanceName("");
      setShowCreateModal(false);
      setNotice(
        `Instância “${instance.id}” criada com diretórios de mods, natives e logs.`,
      );
    } catch (error) {
      setNotice(String(error));
    } finally {
      setBusy(false);
    }
  }
  async function installMinecraft(event: FormEvent) {
    event.preventDefault();
    if (!targetInstance || !minecraftVersion.trim()) return;
    const forgeVersion = DEFAULT_FORGE_VERSION[minecraftVersion.trim()];
    if (loaderChoice === "forge" && !forgeVersion) {
      setNotice("O Aurora ainda não possui uma versão Forge verificada para esse Minecraft.");
      return;
    }
    setBusy(true);
    const loadingLabel = loaderChoice === "forge" ? "Forge" : "Fabric";
    setNotice(`Preparando ${loadingLabel}. Isso pode levar alguns minutos na primeira vez…`);
    try {
      const result = loaderChoice === "fabric"
        ? await invoke<InstallSummary>("install_fabric", { id: targetInstance, minecraftVersion: minecraftVersion.trim() })
        : await invoke<InstallSummary>("install_forge", {
            id: targetInstance,
            minecraftVersion: minecraftVersion.trim(),
            forgeVersion,
          });
      setInstalledVersionId(result.versionId);
      setInstalledMinecraftVersion(result.minecraftVersion);
      await completeInstanceInstall(targetInstance, `Minecraft ${result.minecraftVersion} com ${loadingLabel} instalado! Sua instância “${targetInstance}” está pronta para iniciar.`);
    } catch (error) {
      setDownloadProgress(undefined);
      setNotice(`Não foi possível instalar Minecraft: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }
  async function loadCatalogCategories(source: CatalogSource, type: ContentType | "modpack") {
    const key = catalogCategoryKey(source, type);
    if (catalogCategoryCache.current[key] || catalogCategoryRequests.current.has(key)) return;
    catalogCategoryRequests.current.add(key);
    setCatalogCategoryErrors((current) => {
      const next = { ...current };
      delete next[key];
      return next;
    });
    try {
      let categories: CatalogCategoryOption[];
      if (source === "modrinth") {
        categories = (await invoke<ModrinthCategory[]>("list_modrinth_categories", { contentType: type })).map((category) => ({
          value: category.name,
          name: category.name,
          group: category.header,
        }));
      } else {
        if (!services) throw new Error("Entre na conta Aurora para consultar as categorias do CurseForge.");
        const response = await requestCurseForgeCatalog<{ data?: CurseForgeCategory[] }>(services, {
          action: "categories",
          contentType: type,
        });
        categories = (response.data ?? []).map((category) => ({
          value: String(category.id),
          name: catalogCategoryLabel(category.name, category.slug),
          group: category.group || "Categorias",
        }));
      }
      catalogCategoryCache.current[key] = categories;
      setCatalogCategories((current) => ({ ...current, [key]: categories }));
    } catch (error) {
      setCatalogCategoryErrors((current) => ({ ...current, [key]: errorMessage(error) }));
    } finally {
      catalogCategoryRequests.current.delete(key);
    }
  }

  async function searchModrinth(event: FormEvent) {
    event.preventDefault();
    await loadModrinthPage(0);
  }
  async function loadModrinthPage(
    page: number,
    requestedPageSize = modrinthPageSize,
    requestedSource: CatalogSource = catalogSource,
    requestedCategory = modpackCategoryFilter,
  ) {
    const request = ++catalogRequest.current;
    void loadCatalogCategories(requestedSource, "modpack");
    setCatalogLoading(true);
    setCatalogError("");
    try {
      let items: CatalogPack[];
      let totalHits: number;
      if (requestedSource === "modrinth") {
        const result = await invoke<ModrinthSearchPage>("search_modrinth_modpacks", {
          query: modpackQuery.trim(),
          minecraftVersion: modpackVersionFilter || null,
          loader: modpackLoaderFilter || null,
          category: requestedCategory || null,
          sort: modpackSort,
          offset: page * requestedPageSize,
          limit: requestedPageSize,
        });
        items = result.items.map((pack) => ({ ...pack, source: "modrinth" as const }));
        totalHits = result.totalHits;
      } else {
        if (!services) throw new Error("Entre na conta Aurora para consultar o CurseForge.");
        const result = await requestCurseForgeCatalog<CurseForgeSearchResponse>(services, {
          action: "search",
          contentType: "modpack",
          query: modpackQuery.trim() || undefined,
          gameVersion: modpackVersionFilter || undefined,
          loader: modpackLoaderFilter || undefined,
          categoryId: requestedCategory ? Number(requestedCategory) : undefined,
          sort: modpackSort,
          pageSize: requestedPageSize,
          index: page * requestedPageSize,
        });
        items = (result.data ?? [])
          .map(curseForgeProjectToPack)
          .filter((pack) => !pack.curseForgeHasOnlyUnsupportedLoaders);
        totalHits = result.pagination?.totalCount ?? items.length;
      }
      if (request !== catalogRequest.current) return;
      setModrinthPacks(items);
      setPackVersions((current) => Object.fromEntries(items.map((pack) => [
        pack.projectId,
        modpackVersionFilter && pack.versions.includes(modpackVersionFilter)
          ? modpackVersionFilter
          : current[pack.projectId] || preferredPackVersion(pack, minecraftVersion),
      ])));
      setModrinthTotalHits(totalHits);
      setModrinthPage(page);
    } catch (error) {
      if (request !== catalogRequest.current) return;
      setModrinthPacks([]);
      setModrinthTotalHits(0);
      setModrinthPage(0);
      setCatalogError(`Não foi possível consultar ${requestedSource === "modrinth" ? "Modrinth" : "CurseForge"}: ${errorMessage(error)}`);
    } finally {
      if (request === catalogRequest.current) setCatalogLoading(false);
    }
  }

  async function installCurseForgePack(pack: CatalogPack, id: string) {
    if (!services || !pack.curseForgeId) throw new Error("O modpack CurseForge é inválido.");
    const selectedVersion = packVersions[pack.projectId] ?? preferredPackVersion(pack, minecraftVersion);
    const selectedLoader = modpackLoaderFilter || (pack.loaders.length === 1 && (pack.loaders[0] === "fabric" || pack.loaders[0] === "forge") ? pack.loaders[0] : undefined);
    const filesResponse = await requestCurseForgeCatalog<{ data?: CurseForgeFile[] }>(services, {
      action: "files",
      modId: pack.curseForgeId,
      gameVersion: selectedVersion,
      loader: selectedLoader,
    });
    const packFile = (filesResponse.data ?? []).find((file) => !file.isServerPack && file.fileName.toLowerCase().endsWith(".zip"));
    if (!packFile) throw new Error(`Não há arquivo do modpack compatível com Minecraft ${selectedVersion}.`);
    const archiveSha1 = packFile.hashes?.find((hash) => hash.algo === 1)?.value;
    if (!archiveSha1) throw new Error("O arquivo do modpack não possui SHA-1 para validação.");
    let archiveUrl = packFile.downloadUrl;
    if (!archiveUrl) {
      const resolved = await requestCurseForgeCatalog<{ data?: string }>(services, {
        action: "download",
        modId: pack.curseForgeId,
        fileId: packFile.id,
      }).catch(() => undefined);
      archiveUrl = resolved?.data;
    }
    if (!archiveUrl) {
      await downloadCurseForgeContentAutomatically({
        id,
        pageUrl: curseForgeDownloadPage(pack.slug, packFile.id, "modpack", pack.websiteUrl),
        filename: packFile.fileName,
        contentType: "modpack",
        sha1: archiveSha1,
        projectId: pack.curseForgeId,
        fileId: packFile.id,
      });
    }
    const manifest = await invoke<CurseForgeManifestSummary>("prepare_curseforge_modpack", {
      id,
      projectId: pack.curseForgeId,
      fileId: packFile.id,
      downloadUrl: archiveUrl ?? null,
      filename: packFile.fileName,
      sha1: archiveSha1,
    });
    const required = manifest.files.filter((file) => file.required);
    const resolvedFiles: CurseForgeResolvedFile[] = [];
    const resolvedCatalogFiles: CurseForgeFile[] = [];
    if (required.length > 0) {
      setNotice(`Resolvendo ${required.length} arquivo(s) obrigatório(s) de “${manifest.name}”…`);
      const files: CurseForgeFile[] = [];
      for (let index = 0; index < required.length; index += 400) {
        const response = await requestCurseForgeCatalog<{ data?: CurseForgeFile[] }>(services, {
          action: "resolveFiles",
          files: required.slice(index, index + 400).map((file) => ({ projectId: file.projectId, fileId: file.fileId })),
        });
        files.push(...(response.data ?? []));
      }
      if (files.length !== required.length) throw new Error("O CurseForge não retornou todos os arquivos obrigatórios do modpack.");
      resolvedCatalogFiles.push(...files);
      for (const file of files) {
        const projectId = file.modId ?? file.project?.id;
        const project = file.project;
        const sha1 = file.hashes?.find((hash) => hash.algo === 1)?.value;
        if (!projectId || !sha1 || !file.fileName.toLowerCase().endsWith(".jar")) {
          throw new Error(`O arquivo “${file.fileName || file.id}” não pode ser validado com segurança.`);
        }
        if (!file.downloadUrl) {
          if (!project?.slug) throw new Error(`O projeto ${projectId} não possui página oficial válida.`);
          await downloadCurseForgeContentAutomatically({
            id,
            pageUrl: curseForgeDownloadPage(project.slug, file.id, "mod", project.links?.websiteUrl),
            filename: file.fileName,
            contentType: "mod",
            sha1,
            projectId,
            fileId: file.id,
          });
        }
        resolvedFiles.push({
          projectId,
          fileId: file.id,
          fileName: file.fileName,
          downloadUrl: file.downloadUrl,
          sha1,
          fileSize: file.fileLength,
        });
      }
    }
    const result = await invoke<ModpackInstallSummary>("install_curseforge_modpack", {
      id,
      projectId: pack.curseForgeId,
      fileId: packFile.id,
      files: resolvedFiles,
      iconUrl: safeHttpsUrl(pack.iconUrl) || null,
    });
    for (const file of resolvedCatalogFiles) {
      const project = file.project;
      if (!project) continue;
      rememberContentArtworkData(id, file.fileName, "mod", {
        iconUrl: project.logo?.thumbnailUrl ?? project.logo?.url,
        projectId: String(project.id),
        source: "curseforge",
        title: project.name,
      });
    }
    return result;
  }

  async function installPackIntoInstance(pack: CatalogPack, id: string) {
    const result = pack.source === "modrinth"
      ? await invoke<ModpackInstallSummary>("install_modrinth_modpack", {
        id,
        projectId: pack.projectId,
        minecraftVersion: packVersions[pack.projectId] ?? preferredPackVersion(pack, minecraftVersion),
      })
      : await installCurseForgePack(pack, id);
    await invoke<Instance>("set_instance_presentation", {
      id,
      displayName: pack.title,
      iconUrl: safeHttpsUrl(pack.iconUrl) || null,
    }).catch(() => undefined);
    return result;
  }

  async function createAndInstallModrinth(pack: CatalogPack) {
    const selectedMinecraftVersion = packVersions[pack.projectId] ?? preferredPackVersion(pack, minecraftVersion);
    const id = safeInstanceId(`${pack.slug || pack.title}-${selectedMinecraftVersion}`);
    setBusy(true);
    setNotice(`Criando a instância de “${pack.title}” e baixando os arquivos verificados…`);
    try {
      const instance = await invoke<Instance>("create_instance", { id, unique: true });
      const result = await installPackIntoInstance(pack, instance.id);
      setTargetInstance(instance.id);
      setInstalledVersionId(result.minecraft.versionId);
      setInstalledMinecraftVersion(result.minecraftVersion);
      setMinecraftVersion(result.minecraftVersion);
      await completeInstanceInstall(instance.id, `“${result.name}” instalado! Sua instância está pronta para iniciar: ${result.minecraftVersion} com ${result.loader}.`);
    } catch (error) {
      setDownloadProgress(undefined);
      setNotice(`Não foi possível preparar “${pack.title}”: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }
  async function rehydrateContentArtwork(
    id: string,
    current: InstanceContent,
    seed: Record<string, ContentArtworkEntry> = {},
    stillCurrent: () => boolean = () => true,
  ) {
    let persisted: Record<string, ContentArtworkEntry> = {};
    try {
      persisted = JSON.parse(window.localStorage.getItem(`aurora.contentArtwork.${id}`) ?? "{}") as Record<string, ContentArtworkEntry>;
    } catch {
      persisted = {};
    }
    const known = activeInstanceRef.current.editingInstance === id ? contentArtwork : {};
    const actualKeys = new Set([
      ...current.mods.map((file) => contentArtworkKey("mod", file.name)),
      ...current.shaderpacks.map((file) => contentArtworkKey("shader", file.name)),
      ...current.resourcepacks.map((file) => contentArtworkKey("resourcepack", file.name)),
    ]);
    const mergedArtwork = Object.fromEntries(
      Object.entries({ ...known, ...seed, ...persisted }).filter(([key]) => actualKeys.has(key)),
    ) as Record<string, ContentArtworkEntry>;
    const resolvedGroups = await Promise.all(UPDATE_CONTENT_TYPES.map(async (type) => {
      try {
        return { type, items: await invoke<ResolvedContentArtwork[]>("resolve_content_artwork", { id, contentType: type }) };
      } catch {
        return { type, items: [] as ResolvedContentArtwork[] };
      }
    }));
    for (const group of resolvedGroups) {
      for (const item of group.items) {
        const key = contentArtworkKey(group.type, item.filename);
        if (!actualKeys.has(key)) continue;
        const registered = mergedArtwork[key];
        const source = registeredArtworkSource(registered);
        if (source === "curseforge") continue;
        mergedArtwork[key] = {
          iconUrl: registered?.iconUrl ?? item.iconUrl,
          projectId: registered?.projectId ?? item.projectId,
          source: source ?? "modrinth",
          title: registered?.title ?? item.title,
        };
      }
    }
    if (!stillCurrent()) return mergedArtwork;
    window.localStorage.setItem(`aurora.contentArtwork.${id}`, JSON.stringify(mergedArtwork));
    setContentArtwork(mergedArtwork);
    return mergedArtwork;
  }
  async function openEditor(id: string, initialView: EditorView = "installed", runId?: string) {
    if (recoveryBusyRef.current || voice?.isBlocking()) return;
    const editorRequest = { generation: editorOpenRequestRef.current.generation + 1, id };
    editorOpenRequestRef.current = editorRequest;
    const editorIsCurrent = () => editorOpenRequestRef.current.generation === editorRequest.generation
      && editorOpenRequestRef.current.id === id;
    invalidateContentUpdatePlan();
    setActiveSection("instances");
    activeInstanceRef.current = { ...activeInstanceRef.current, editingInstance: id };
    if (!editingInstance && document.activeElement instanceof HTMLElement) editorOpener.current = document.activeElement;
    setInstanceContent({ mods: [], shaderpacks: [], resourcepacks: [] });
    setEditingInstance(id);
    setRenameDraft(id);
    setEditorView(initialView);
    setDoctorRunId(runId);
    setDoctorRecoveryPointId(undefined);
    setSelectedContentNames([]);
    setInstalledContentFilter("");
    setContentQuery("");
    setContentResults([]);
    setContentUpdateFilter("all");
    setContentUpdateQuery("");
    setLastBackupAvailable(false);
    let savedArtwork: Record<string, ContentArtworkEntry> = {};
    try {
      const savedArtworkText = window.localStorage.getItem(`aurora.contentArtwork.${id}`);
      if (savedArtworkText) savedArtwork = JSON.parse(savedArtworkText) as Record<string, ContentArtworkEntry>;
    } catch {
      savedArtwork = {};
    }
    if (!editorIsCurrent()) return;
    setContentArtwork(savedArtwork);
    await selectInstance(id, editorIsCurrent);
    if (!editorIsCurrent()) return;
    try {
      const current = await invoke<InstanceContent>("list_instance_content", { id });
      if (!editorIsCurrent()) return;
      setInstanceContent(current);
      await rehydrateContentArtwork(id, current, savedArtwork, editorIsCurrent);
    } catch (error) {
      if (editorIsCurrent()) setNotice(`Não foi possível abrir o conteúdo da instância: ${String(error)}`);
    }
  }
  async function setContentEnabled(file: InstanceContentFile, enabled: boolean) {
    const id = editingInstance || targetInstance;
    if (!id) return;
    invalidateContentUpdatePlan();
    setBusy(true);
    try {
      await invoke("set_instance_content_enabled", {
        id,
        contentType,
        filename: file.name,
        enabled,
      });
      setInstanceContent(await invoke<InstanceContent>("list_instance_content", { id }));
      setNotice(`${file.name} foi ${enabled ? "ativado" : "desativado"}.`);
    } catch (error) {
      setNotice(`Não foi possível alterar “${file.name}”: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }
  async function setSelectedContentEnabled(enabled: boolean) {
    const id = editingInstance || targetInstance;
    const filesToChange = selectedContentFiles.filter(
      (file) => selectedContentNames.includes(file.name) && file.enabled !== enabled,
    );
    if (!id || filesToChange.length === 0) return;
    invalidateContentUpdatePlan();
    setBusy(true);
    try {
      const changed = await invoke<number>("set_instance_content_enabled_bulk", {
        id,
        contentType,
        filenames: filesToChange.map((file) => file.name),
        enabled,
      });
      setInstanceContent(await invoke<InstanceContent>("list_instance_content", { id }));
      setNotice(`${changed} item(ns) ${enabled ? "ativados" : "desativados"}.`);
    } catch (error) {
      setNotice(`Não foi possível atualizar a seleção: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }
  async function removeSelectedContent() {
    const id = editingInstance || targetInstance;
    if (!id || selectedContentNames.length === 0) return;
    const confirmed = window.confirm(
      `Desinstalar ${selectedContentNames.length} item(ns) da instância “${id}”? Esta ação remove os arquivos selecionados.`,
    );
    if (!confirmed) return;
    invalidateContentUpdatePlan();
    setBusy(true);
    try {
      const removed = await invoke<number>("remove_instance_content", {
        id,
        contentType,
        filenames: selectedContentNames,
      });
      setSelectedContentNames([]);
      setInstanceContent(await invoke<InstanceContent>("list_instance_content", { id }));
      setNotice(`${removed} item(ns) desinstalados.`);
    } catch (error) {
      setNotice(`Não foi possível desinstalar a seleção: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }
  async function openInstanceFolder(id: string) {
    try {
      await invoke("open_instance_folder", { id });
      setNotice(`Pasta de “${id}” aberta.`);
    } catch (error) {
      setNotice(`Não foi possível abrir a pasta: ${String(error)}`);
    }
  }
  function toggleContentSelection(filename: string) {
    setSelectedContentNames((current) => current.includes(filename)
      ? current.filter((name) => name !== filename)
      : [...current, filename]);
  }
  function selectContentType(nextType: ContentType) {
    if (recoveryBusyRef.current || voice?.isBlocking()) return;
    setContentType(nextType);
    setSelectedContentNames([]);
    setSelectedUpdateFilenames([]);
    setContentUpdateFilter("all");
    setContentUpdateQuery("");
    setInstalledContentFilter("");
    setContentQuery("");
    setContentCategoryFilter("");
    setContentResults([]);
    if (editorView === "discover") void loadContentCatalog({ type: nextType, query: "", category: "", sort: "popular" });
  }
  async function selectInstance(id: string, stillCurrent: () => boolean = () => true) {
    if (recoveryBusyRef.current || voice?.isBlocking()) return;
    activeInstanceRef.current = { ...activeInstanceRef.current, targetInstance: id };
    setTargetInstance(id);
    try {
      const saved = await invoke<InstanceLaunchProfile>("read_instance_launch_profile", { id });
      if (!stillCurrent()) return;
      if (saved.versionId) setInstalledVersionId(saved.versionId);
      if (saved.minecraftVersion) {
        setInstalledMinecraftVersion(saved.minecraftVersion);
        setMinecraftVersion(saved.minecraftVersion);
      }
    } catch (error) {
      if (stillCurrent()) setNotice(`Não foi possível ler a configuração de “${id}”: ${String(error)}`);
    }
  }
  function instanceLoader(): LoaderChoice | undefined {
    return supportedLoaderFromVersionId(installedVersionId);
  }
  async function readUpdaterContext(id: string) {
    const launchProfile = await invoke<InstanceLaunchProfile>("read_instance_launch_profile", { id });
    const gameVersion = launchProfile.minecraftVersion || installedMinecraftVersion || minecraftVersion;
    const loader = supportedLoaderFromVersionId(launchProfile.versionId || installedVersionId);
    if (!gameVersion || !loader) {
      throw new Error("Instale Fabric ou Forge nesta instância antes de verificar atualizações.");
    }
    return { gameVersion, loader: loader as LoaderChoice };
  }
  async function mergeCurseForgeUpdateItems(
    id: string,
    type: ContentType,
    items: ContentUpdateItem[],
    gameVersion: string,
    loader: LoaderChoice,
    installedProjectIds: ReadonlySet<number>,
    scanIsCurrent: () => boolean,
  ): Promise<{ items: ContentUpdateItem[]; warning?: string }> {
    // One page per category keeps a full scan + confirmation + refresh below the
    // Worker updates quota. Cached pages let subsequent explicit checks advance.
    const maxCandidatesPerCycle = 16;
    const candidates: Array<{ projectId: number; item: ContentUpdateItem }> = [];
    const seenProjects = new Set<number>();
    for (const item of items) {
      const projectId = Number(item.projectId);
      if (item.status !== "unmanaged"
        || !Number.isSafeInteger(projectId)
        || projectId <= 0
        || !item.currentSha1?.match(/^[a-f0-9]{40}$/i)
        || seenProjects.has(projectId)) continue;
      seenProjects.add(projectId);
      candidates.push({ projectId, item });
    }
    if (candidates.length === 0) return { items };
    if (!services) return { items, warning: "Entre na conta Aurora para verificar os itens reconhecidos pelo CurseForge." };
    const cachePrefix = `${id}\u0000${type}\u0000`;
    const scanKey = `${id}\u0000${type}`;
    const cacheKey = (projectId: number, sha1: string) => `${cachePrefix}${projectId}\u0000${sha1.toLowerCase()}`;
    const candidateKeys = new Set(candidates.map(({ projectId, item }) => cacheKey(projectId, item.currentSha1!)));
    for (const key of curseForgeUpdateCacheRef.current.keys()) {
      if (key.startsWith(cachePrefix) && !candidateKeys.has(key)) curseForgeUpdateCacheRef.current.delete(key);
    }
    if (curseForgeCompletedScansRef.current.has(scanKey)) {
      for (const key of candidateKeys) curseForgeUpdateCacheRef.current.delete(key);
      curseForgeCompletedScansRef.current.delete(scanKey);
    }
    const pendingCandidates = candidates.filter(({ projectId, item }) => !curseForgeUpdateCacheRef.current.has(cacheKey(projectId, item.currentSha1!)));
    const cycleCandidates = pendingCandidates.slice(0, maxCandidatesPerCycle);
    let requestFailure = "";
    for (let index = 0; index < cycleCandidates.length && scanIsCurrent(); index += 4) {
      const chunk = cycleCandidates.slice(index, index + 4);
      try {
        const response = await requestCurseForgeCatalog<CurseForgeUpdatesResponse>(services, {
          action: "updates",
          contentType: type,
          gameVersion,
          loader: type === "mod" ? loader : undefined,
          items: chunk.map(({ projectId, item }) => ({ projectId, currentSha1: item.currentSha1! })),
        });
        for (const entry of response.data) {
          if (typeof entry.currentSha1 === "string") {
            curseForgeUpdateCacheRef.current.set(cacheKey(entry.projectId, entry.currentSha1), entry);
          }
        }
      } catch (error) {
        requestFailure = updaterErrorMessage(error);
        break;
      }
    }
    try {
      const resolved = new Map<string, CurseForgeUpdatesResponse["data"][number]>();
      for (const { projectId, item } of candidates) {
        const entry = curseForgeUpdateCacheRef.current.get(cacheKey(projectId, item.currentSha1!));
        if (entry && typeof entry.currentSha1 === "string") resolved.set(`${entry.projectId}:${entry.currentSha1.toLowerCase()}`, entry);
      }
      const merged = items.map((item) => {
        const projectId = Number(item.projectId);
        const match = item.currentSha1 ? resolved.get(`${projectId}:${item.currentSha1.toLowerCase()}`) : undefined;
        if (!match) return item;
        const target = match.targetFile;
        const requiresOfficialPage = Boolean(target?.manualDownloadRequired || match.reason === "manualDownloadRequired");
        const missingDependencies = match.status === "updateAvailable"
          ? missingRequiredCurseForgeDependencies(target, installedProjectIds)
          : [];
        const dependencyBlocked = missingDependencies.length > 0;
        return {
          ...item,
          provider: "curseforge" as const,
          title: match.project.name || item.title,
          iconUrl: match.project.logo?.thumbnailUrl || match.project.logo?.url || item.iconUrl,
          targetVersionId: target ? String(target.id) : item.targetVersionId,
          targetVersionNumber: target?.displayName || target?.fileName || item.targetVersionNumber,
          targetFilename: target?.fileName || item.targetFilename,
          targetFile: target,
          status: requiresOfficialPage || dependencyBlocked ? "blocked" as const : match.status,
          reasonCode: dependencyBlocked ? "missingRequiredDependencies" : match.reason,
          reason: requiresOfficialPage
            ? "O CurseForge não disponibilizou uma URL direta; a troca automática fica bloqueada para preservar o arquivo atual."
            : dependencyBlocked
              ? `A nova versão exige ${missingDependencies.length === 1 ? "a dependência" : "as dependências"} CurseForge ${missingDependencies.join(", ")}. Instale ${missingDependencies.length === 1 ? "essa dependência" : "essas dependências"} antes de atualizar.`
            : match.message,
        };
      });
      const resolvedCount = candidates.filter(({ projectId, item }) => curseForgeUpdateCacheRef.current.has(cacheKey(projectId, item.currentSha1!))).length;
      const remaining = candidates.length - resolvedCount;
      if (!requestFailure && remaining === 0) curseForgeCompletedScansRef.current.add(scanKey);
      const warnings = [
        remaining > 0
          ? `${resolvedCount} de ${candidates.length} projetos CurseForge desta categoria foram verificados. Clique em “Verificar novamente” para continuar com os ${remaining} restantes.`
          : "",
        requestFailure
          ? `A consulta CurseForge foi interrompida; os resultados já recebidos foram preservados e os demais continuam como não identificados: ${requestFailure}`
          : "",
      ].filter(Boolean);
      return {
        items: merged,
        warning: warnings.join(" ") || undefined,
      };
    } catch (error) {
      return { items, warning: `CurseForge não pôde ser consultado: ${updaterErrorMessage(error)}` };
    }
  }
  async function scanAllContentUpdates(options: { id?: string; announce?: boolean; allowDuringApply?: boolean } = {}) {
    const id = options.id || editingInstance || targetInstance;
    if (!id) return;
    if (busy || (updatesApplying && !options.allowDuringApply)) {
      if (options.announce !== false) setNotice("Aguarde a operação atual terminar antes de verificar atualizações.");
      return;
    }
    const scanRequest = { generation: updateScanRequestRef.current.generation + 1, id };
    updateScanRequestRef.current = scanRequest;
    const scanIsCurrent = () => updateScanRequestRef.current.generation === scanRequest.generation
      && updateScanRequestRef.current.id === id;
    clearContentUpdatePlan();
    setUpdatesScanning(true);
    try {
      const { gameVersion, loader } = await readUpdaterContext(id);
      if (!scanIsCurrent()) return;
      const scannedAt = new Date().toISOString();
      let artworkForScan = contentArtwork;
      try {
        const persisted = JSON.parse(window.localStorage.getItem(`aurora.contentArtwork.${id}`) ?? "{}") as Record<string, ContentArtworkEntry>;
        artworkForScan = { ...contentArtwork, ...persisted };
      } catch {
        // O cache visual é opcional; o scan do backend continua sendo a fonte de verdade.
      }
      const scannedResults: Array<readonly [ContentType, ContentUpdateScanView]> = await Promise.all(UPDATE_CONTENT_TYPES.map(async (type): Promise<readonly [ContentType, ContentUpdateScanView]> => {
        try {
          const result = await invoke<ContentUpdateScanResult>("scan_instance_content_updates", {
            id,
            contentType: type,
            minecraftVersion: gameVersion,
            loader,
          });
          const identifiedItems = result.items.map((item) => {
            const artwork = artworkForScan[contentArtworkKey(type, item.filename)];
            const registeredSource = registeredArtworkSource(artwork);
            const keepCurseForgeOwnership = registeredSource === "curseforge" && item.status !== "auroraManaged";
            return {
              ...item,
              contentType: type,
              iconUrl: artwork?.iconUrl ?? item.iconUrl,
              provider: registeredSource ?? item.provider,
              projectId: artwork?.projectId ?? item.projectId,
              title: artwork?.title ?? item.title,
              ...(keepCurseForgeOwnership ? {
                status: "unmanaged" as const,
                targetVersionId: undefined,
                targetVersionNumber: undefined,
                targetFilename: undefined,
                reason: undefined,
              } : {}),
            };
          });
          return [type, { ...result, items: identifiedItems, scannedAt }];
        } catch (error) {
          return [type, {
            items: [],
            contentType: type,
            minecraftVersion: gameVersion,
            loader,
            scannedAt,
            error: updaterErrorMessage(error),
          }];
        }
      }));
      if (!scanIsCurrent()) return;
      const installedProjectIds = new Set<number>();
      for (const [, result] of scannedResults) {
        for (const item of result.items) {
          const projectId = Number(item.projectId);
          if (item.enabled && Number.isSafeInteger(projectId) && projectId > 0) installedProjectIds.add(projectId);
        }
      }
      const results: Array<readonly [ContentType, ContentUpdateScanView]> = await Promise.all(scannedResults.map(async ([type, result]) => {
        if (result.error) return [type, result] as const;
        const curseForge = await mergeCurseForgeUpdateItems(id, type, result.items, gameVersion, loader, installedProjectIds, scanIsCurrent);
        return [type, { ...result, items: curseForge.items, warning: curseForge.warning }] as const;
      }));
      if (!scanIsCurrent()) return;
      const nextScans = Object.fromEntries(results) as Record<ContentType, ContentUpdateScanView>;
      setContentUpdateScans(nextScans);
      setLastBackupAvailable(results.some(([, result]) => Boolean(result.rollbackAvailable ?? result.lastBackupAvailable)));

      let resolvedPack: ModpackUpdate | null = null;
      let packScanFailed = false;
      let packScanError = "";
      try {
        let pack = await invoke<ModpackUpdate | null>("scan_instance_modpack_update", { id });
        if (!scanIsCurrent()) return;
        if (pack) {
          const instance = instances.find((candidate) => candidate.id === id);
          pack = { ...pack, name: pack.name || instance?.displayName || id };
        }
        if (pack?.source === "curseforge" && services) {
          try {
            const projectId = Number(pack.projectId);
            const currentFileId = Number(pack.currentFileId);
            if (Number.isSafeInteger(projectId) && projectId > 0 && Number.isSafeInteger(currentFileId) && currentFileId > 0) {
              const response = await requestCurseForgeCatalog<CurseForgeUpdatesResponse>(services, {
                action: "updates",
                contentType: "modpack",
                gameVersion: pack.minecraftVersion || gameVersion,
                loader: (pack.loader === "forge" || pack.loader === "fabric") ? pack.loader : loader,
                items: [{ projectId, currentFileId }],
              });
              if (!scanIsCurrent()) return;
              const match = response.data.find((entry) => entry.projectId === projectId && entry.currentFileId === currentFileId);
              if (match) {
                const requiresOfficialPage = Boolean(match.targetFile?.manualDownloadRequired || match.reason === "manualDownloadRequired");
                const automaticApplyUnavailable = match.status === "updateAvailable" && !requiresOfficialPage;
                pack = {
                  ...pack,
                  provider: "curseforge",
                  name: match.project.name || pack.name,
                  targetVersionId: match.targetFile ? String(match.targetFile.id) : pack.targetVersionId,
                  targetVersionNumber: match.targetFile?.displayName || match.targetFile?.fileName || pack.targetVersionNumber,
                  targetFile: match.targetFile,
                  status: requiresOfficialPage || automaticApplyUnavailable ? "blocked" : match.status,
                  reasonCode: automaticApplyUnavailable ? "curseForgeModpackApplyUnavailable" : match.reason,
                  reason: requiresOfficialPage
                    ? "O CurseForge não disponibilizou uma URL direta; a atualização automática do modpack não pode substituir arquivos com segurança."
                    : automaticApplyUnavailable
                      ? `A versão ${match.targetFile?.displayName || match.targetFile?.fileName || "mais recente"} está disponível, mas este modpack CurseForge não possui um plano transacional aplicável. Nada foi alterado.`
                    : match.message,
                };
              }
            } else {
              pack = {
                ...pack,
                status: "blocked",
                reason: "O arquivo CurseForge instalado não foi registrado. Reinstale o modpack pelo Aurora para habilitar uma verificação segura.",
              };
            }
          } catch (error) {
            packScanError = `CurseForge não pôde ser consultado: ${updaterErrorMessage(error)}`;
          }
        }
        resolvedPack = pack;
      } catch (error) {
        packScanFailed = true;
        packScanError = updaterErrorMessage(error);
      }
      if (!scanIsCurrent()) return;
      setModpackUpdate(resolvedPack);
      setModpackScanError(packScanError);

      const failed = results.filter(([, result]) => Boolean(result.error)).length;
      const contentFound = results.reduce((total, [, result]) => total + result.items.filter((item) => item.status === "updateAvailable").length, 0);
      const packVisible = Boolean(resolvedPack && (resolvedPack.source || resolvedPack.projectId));
      const found = contentFound + (packVisible && resolvedPack?.status === "updateAvailable" ? 1 : 0);
      const attention = results.reduce((total, [, result]) => total + result.items.filter((item) => item.status === "blocked" || item.status === "unmanaged").length, 0)
        + (packVisible && (resolvedPack?.status === "blocked" || resolvedPack?.status === "unmanaged") ? 1 : 0);
      const failedSteps = failed + (packScanFailed ? 1 : 0);
      if (options.announce !== false) {
        setNotice(failedSteps
          ? `A verificação encontrou ${found} ${found === 1 ? "atualização" : "atualizações"}, mas ${failedSteps} ${failedSteps === 1 ? "etapa não pôde" : "etapas não puderam"} ser concluída${failedSteps === 1 ? "" : "s"}.`
          : found > 0
            ? `${found} ${found === 1 ? "atualização compatível encontrada" : "atualizações compatíveis encontradas"} em “${id}”.`
            : attention > 0
              ? `Nenhuma atualização automática foi encontrada em “${id}”; ${attention} ${attention === 1 ? "item precisa" : "itens precisam"} de atenção.`
              : `“${id}” está com o conteúdo atualizado.`);
      }
    } catch (error) {
      if (!scanIsCurrent()) return;
      const message = updaterErrorMessage(error);
      const scannedAt = new Date().toISOString();
      setContentUpdateScans(Object.fromEntries(UPDATE_CONTENT_TYPES.map((type) => [type, { items: [], contentType: type, scannedAt, error: message }])) as unknown as Record<ContentType, ContentUpdateScanView>);
      setModpackUpdate(null);
      setModpackScanError(message);
      if (options.announce !== false) setNotice(`Não foi possível verificar atualizações: ${message}`);
    } finally {
      if (scanIsCurrent()) setUpdatesScanning(false);
    }
  }
  function showContentUpdates() {
    setEditorView("updates");
    setSelectedUpdateFilenames([]);
    setContentUpdateFilter("all");
    if (Object.keys(contentUpdateScans).length === 0) void scanAllContentUpdates();
  }
  function activateEditorView(view: EditorView) {
    if (recoveryBusyRef.current || voice?.isBlocking()) return;
    if (view === "updates") showContentUpdates();
    else if (view === "discover") showContentCatalog();
    else setEditorView(view);
  }
  function handleEditorViewKeyDown(event: ReactKeyboardEvent<HTMLButtonElement>, view: EditorView) {
    if (recoveryBusyRef.current || voice?.isBlocking()) return;
    if (!["ArrowUp", "ArrowDown", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const currentIndex = EDITOR_VIEWS.indexOf(view);
    const nextIndex = event.key === "Home"
      ? 0
      : event.key === "End"
        ? EDITOR_VIEWS.length - 1
        : (currentIndex + (event.key === "ArrowDown" ? 1 : -1) + EDITOR_VIEWS.length) % EDITOR_VIEWS.length;
    const nextView = EDITOR_VIEWS[nextIndex];
    activateEditorView(nextView);
    window.requestAnimationFrame(() => document.getElementById(`editor-view-tab-${nextView}`)?.focus());
  }
  function toggleUpdateSelection(filename: string) {
    if (busy || updatesScanning || updatesApplying) return;
    const item = contentUpdateScans[contentType]?.items.find((candidate) => candidate.filename === filename);
    if (!item) return;
    if (selectedUpdateFilenames.includes(filename)) {
      setSelectedUpdateFilenames((current) => current.filter((name) => name !== filename));
      return;
    }
    const selectedItems = contentUpdateScans[contentType]?.items.filter((candidate) => selectedUpdateFilenames.includes(candidate.filename)) ?? [];
    const selectedProvider = selectedItems[0]?.provider ?? (selectedItems.length ? "modrinth" : undefined);
    const nextProvider = item.provider ?? "modrinth";
    if (selectedProvider && selectedProvider !== nextProvider) {
      setNotice("Atualize itens do Modrinth e do CurseForge em etapas separadas para manter o backup reversível.");
      return;
    }
    setSelectedUpdateFilenames((current) => [...current, filename]);
  }
  function selectAvailableUpdates(items: ContentUpdateItem[]) {
    if (busy || updatesScanning || updatesApplying) return;
    if (items.length === 0) {
      setSelectedUpdateFilenames([]);
      return;
    }
    const provider = items[0].provider ?? "modrinth";
    setSelectedUpdateFilenames(items.filter((item) => (item.provider ?? "modrinth") === provider).map((item) => item.filename));
  }
  function requestUpdateReview(request: UpdateReviewRequest) {
    if (busy || updatesScanning || updatesApplying) {
      setNotice("Aguarde a operação atual terminar antes de revisar atualizações.");
      return;
    }
    if (runningInstances.some((running) => running.instanceId === editingInstance)) {
      setNotice("Feche o Minecraft antes de alterar o conteúdo desta instância.");
      return;
    }
    setUpdateReviewError("");
    setUpdateReview(request);
  }
  async function refreshContentAfterUpdate(id: string) {
    const current = await invoke<InstanceContent>("list_instance_content", { id });
    setInstanceContent(current);
    await rehydrateContentArtwork(id, current, {}, () => activeInstanceRef.current.editingInstance === id);
    await selectInstance(id);
    await scanAllContentUpdates({ id, announce: false, allowDuringApply: true });
  }
  async function applyReviewedUpdate() {
    const request = updateReview;
    const id = editingInstance || targetInstance;
    if (!request || !id || updatesApplying) return;
    if (busy || updatesScanning) {
      setUpdateReviewError("Aguarde a operação atual terminar antes de continuar.");
      return;
    }
    if (runningInstances.some((running) => running.instanceId === id)) {
      setUpdateReviewError("Feche o Minecraft antes de continuar.");
      return;
    }
    setUpdatesApplying(true);
    setUpdateReviewError("");
    let backendCommitted = false;
    let committedMessage = "";
    try {
      if (request.kind === "content") {
        const { gameVersion, loader } = await readUpdaterContext(id);
        const updateType = (request.items[0]?.contentType ?? contentType) as ContentType;
        const providers = new Set(request.items.map((item) => item.provider ?? "modrinth"));
        if (providers.size !== 1) throw new Error("Atualize Modrinth e CurseForge em etapas separadas para manter um único backup reversível.");
        const provider = [...providers][0];
        let summary: ContentUpdateSummary;
        if (provider === "curseforge") {
          if (!services) throw new Error("Entre na conta Aurora para confirmar as versões do CurseForge.");
          if (request.items.length > 16) throw new Error("Revise no máximo 16 itens CurseForge por etapa.");
          const installedProjectIds = new Set<number>();
          for (const scan of Object.values(contentUpdateScans)) {
            for (const scannedItem of scan?.items ?? []) {
              const projectId = Number(scannedItem.projectId);
              if (scannedItem.enabled && Number.isSafeInteger(projectId) && projectId > 0) installedProjectIds.add(projectId);
            }
          }
          const refreshed = new Map<string, CurseForgeUpdatesResponse["data"][number]>();
          for (let index = 0; index < request.items.length; index += 4) {
            const chunk = request.items.slice(index, index + 4);
            const refreshItems = chunk.map((item) => {
              const projectId = Number(item.projectId);
              if (!Number.isSafeInteger(projectId) || projectId <= 0 || !item.currentSha1?.match(/^[a-f0-9]{40}$/i)) {
                throw new Error(`“${item.title || item.filename}” não possui uma identificação CurseForge verificável.`);
              }
              return { projectId, currentSha1: item.currentSha1 };
            });
            const response = await requestCurseForgeCatalog<CurseForgeUpdatesResponse>(services, {
              action: "updates",
              contentType: updateType,
              gameVersion,
              loader: updateType === "mod" ? loader : undefined,
              items: refreshItems,
            });
            for (const entry of response.data) {
              if (typeof entry.currentSha1 === "string") {
                refreshed.set(`${entry.projectId}:${entry.currentSha1.toLowerCase()}`, entry);
              }
            }
          }
          const updates = request.items.map((item) => {
            const projectId = Number(item.projectId);
            const match = item.currentSha1
              ? refreshed.get(`${projectId}:${item.currentSha1.toLowerCase()}`)
              : undefined;
            const reviewedTarget = item.targetFile;
            const target = match?.targetFile;
            const planChanged = match?.status !== "updateAvailable"
              || match.reason === "manualDownloadRequired"
              || !reviewedTarget
              || !target
              || target.id !== reviewedTarget.id
              || target.fileName !== reviewedTarget.fileName
              || target.sha1.toLowerCase() !== reviewedTarget.sha1.toLowerCase()
              || target.fileLength !== reviewedTarget.fileLength;
            if (planChanged) {
              throw new Error(`O catálogo mudou desde a revisão de “${item.title || item.filename}”. Verifique novamente e revise o novo plano antes de atualizar.`);
            }
            const missingDependencies = missingRequiredCurseForgeDependencies(target, installedProjectIds);
            if (missingDependencies.length > 0) {
              throw new Error(`“${item.title || item.filename}” exige ${missingDependencies.length === 1 ? "a dependência" : "as dependências"} CurseForge ${missingDependencies.join(", ")}. Instale ${missingDependencies.length === 1 ? "essa dependência" : "essas dependências"} antes de atualizar.`);
            }
            if (!item.currentSha1?.match(/^[a-f0-9]{40}$/i)
              || !target?.downloadUrl?.startsWith("https://")
              || !target.sha1.match(/^[a-f0-9]{40}$/i)
              || !target.fileLength
              || target.fileLength <= 0) {
              throw new Error(`“${item.title || item.filename}” não possui um download CurseForge verificável para atualização automática.`);
            }
            return {
              currentFilename: item.filename,
              currentSha1: item.currentSha1,
              targetFilename: target.fileName,
              downloadUrl: target.downloadUrl,
              sha1: target.sha1,
              size: target.fileLength,
              projectId: String(projectId),
              currentVersionId: item.currentVersionId ?? item.installedVersionId,
              currentVersionNumber: item.currentVersionNumber ?? item.installedVersionName,
              targetVersionId: String(target.id),
              targetVersionNumber: target.displayName || target.fileName,
            };
          });
          summary = await invoke<ContentUpdateSummary>("apply_remote_content_updates", {
            id,
            contentType: updateType,
            minecraftVersion: gameVersion,
            loader,
            updates,
          });
        } else {
          summary = await invoke<ContentUpdateSummary>("apply_instance_content_updates", {
            id,
            contentType: updateType,
            filenames: request.items.map((item) => item.filename),
            minecraftVersion: gameVersion,
            loader,
          });
        }
        backendCommitted = true;
        committedMessage = summary.message || `${summary.updatedFiles.length} arquivo(s) atualizado(s) com backup pronto para desfazer.`;
        const reviewedByFilename = new Map(request.items.map((item) => [item.filename, item]));
        const artworkMigrations = summary.updatedFiles.map((file): ContentArtworkMigration => {
          const reviewed = reviewedByFilename.get(file.previousFilename);
          return {
            contentType: updateType,
            fromFilename: file.previousFilename,
            toFilename: file.filename,
            fallback: file.projectId || reviewed?.projectId ? {
              iconUrl: reviewed?.iconUrl,
              projectId: file.projectId || reviewed?.projectId || "",
              source: reviewed?.provider ?? (/^\d+$/.test(file.projectId) ? "curseforge" : "modrinth"),
              title: reviewed?.title || reviewed?.filename || file.filename,
            } : undefined,
          };
        });
        migrateContentArtwork(id, artworkMigrations);
        lastArtworkMigrationRef.current = { id, migrations: artworkMigrations };
        setLastBackupAvailable(Boolean(summary.rollbackAvailable ?? summary.backupAvailable));
        setSelectedUpdateFilenames([]);
        setNotice(committedMessage);
      } else if (request.kind === "modpack") {
        if (request.modpack.source !== "modrinth") throw new Error("A atualização automática de modpacks CurseForge permanece bloqueada porque não há um plano transacional seguro.");
        const summary = await invoke<ModpackUpdateSummary>("apply_instance_modpack_update", { id });
        backendCommitted = true;
        committedMessage = summary.message || `O modpack foi atualizado para ${summary.versionNumber || "a versão compatível mais recente"}; ${summary.updatedFileCount} arquivo(s) foram verificados${summary.preservedExtras ? ` e ${summary.preservedExtras} extra(s) locais foram preservados` : ""}.`;
        setLastBackupAvailable(summary.rollbackAvailable);
        setNotice(committedMessage);
      } else {
        const summary = await invoke<ContentRollbackSummary>("rollback_last_content_update", { id });
        backendCommitted = true;
        committedMessage = `${summary.restoredFiles.length} arquivo(s) restaurado(s) a partir do último backup.`;
        const previousMigrations = lastArtworkMigrationRef.current?.id === id
          ? lastArtworkMigrationRef.current.migrations
          : [];
        const detailedMigrations = summary.restoredFiles.flatMap((file): ContentArtworkMigration[] => typeof file === "string"
          ? []
          : file.contentType === "modpack"
            ? []
            : [{
              contentType: UPDATE_CONTENT_TYPES.includes(file.contentType as ContentType)
                ? file.contentType as ContentType
                : previousMigrations.find((migration) => migration.toFilename === file.replacedFilename && migration.fromFilename === file.restoredFilename)?.contentType,
              fromFilename: file.replacedFilename,
              toFilename: file.restoredFilename,
            }]);
        const fallbackMigrations = previousMigrations.map((migration) => ({
            contentType: migration.contentType,
            fromFilename: migration.toFilename,
            toFilename: migration.fromFilename,
          }));
        migrateContentArtwork(id, detailedMigrations.length > 0 ? detailedMigrations : fallbackMigrations);
        lastArtworkMigrationRef.current = null;
        setLastBackupAvailable(false);
        setSelectedUpdateFilenames([]);
        setNotice(committedMessage);
      }
      setUpdateReview(undefined);
      await refreshContentAfterUpdate(id);
    } catch (error) {
      const message = updaterErrorMessage(error);
      if (backendCommitted) {
        invalidateContentUpdatePlan();
        setNotice(`${committedMessage} A alteração foi concluída, mas a tela não pôde ser atualizada: ${message}. Use “Verificar novamente”.`);
      } else {
        setUpdateReviewError(message);
        setNotice(`A alteração não foi aplicada: ${message}`);
      }
    } finally {
      setUpdatesApplying(false);
    }
  }
  async function loadContentCatalog(options: {
    source?: CatalogSource;
    type?: ContentType;
    query?: string;
    sort?: ContentSort;
    version?: string;
    loader?: "" | LoaderChoice;
    category?: string;
  } = {}) {
    const source = options.source ?? contentSource;
    const type = options.type ?? contentType;
    const query = options.query ?? contentQuery;
    const sort = options.sort ?? contentSort;
    const version = options.version ?? contentVersionFilter;
    const loader = options.loader ?? contentLoaderFilter;
    const category = options.category ?? contentCategoryFilter;
    void loadCatalogCategories(source, type);
    setBusy(true);
    try {
      let results: CatalogContent[];
      if (source === "modrinth") {
        const found = await invoke<ModrinthContent[]>("search_modrinth_content", {
          query: query.trim(),
          contentType: type,
          minecraftVersion: version || null,
          loader: type === "mod" ? loader || null : null,
          category: category || null,
          sort,
        });
        results = found.map((item) => ({ ...item, source: "modrinth" as const }));
      } else {
        if (!services) throw new Error("Entre na conta Aurora para consultar o CurseForge.");
        const response = await requestCurseForgeCatalog<CurseForgeSearchResponse>(services, {
          action: "search",
          contentType: type,
          query: query.trim() || undefined,
          gameVersion: version || undefined,
          loader: type === "mod" && loader ? loader : undefined,
          categoryId: category ? Number(category) : undefined,
          sort,
          pageSize: 24,
          index: 0,
        });
        results = (response.data ?? []).map((item) => ({
          source: "curseforge" as const,
          curseForgeId: item.id,
          projectId: String(item.id),
          slug: item.slug,
          title: item.name,
          description: item.summary ?? "",
          iconUrl: item.logo?.thumbnailUrl ?? item.logo?.url,
          versions: [...new Set((item.latestFilesIndexes ?? []).map((entry) => entry.gameVersion).filter((value): value is string => Boolean(value)))],
          loaders: [...new Set((item.latestFilesIndexes ?? []).map((entry) => entry.modLoader === 1 ? "forge" : entry.modLoader === 4 ? "fabric" : "").filter(Boolean))],
          downloads: item.downloadCount ?? 0,
          author: item.authors?.[0]?.name ?? "",
          dateModified: item.dateModified ?? "",
          websiteUrl: item.links?.websiteUrl,
          gallery: (item.screenshots ?? []).map((image) => image.url ?? image.thumbnailUrl ?? "").filter(Boolean),
        }));
      }
      setContentResults(results);
      setNotice(`${results.length} resultado(s) carregados do ${source === "modrinth" ? "Modrinth" : "CurseForge"}.`);
    } catch (error) {
      setContentResults([]);
      setNotice(`Não foi possível carregar o catálogo: ${errorMessage(error)}`);
    } finally {
      setBusy(false);
    }
  }
  async function searchContent(event: FormEvent) {
    event.preventDefault();
    await loadContentCatalog();
  }
  function showContentCatalog() {
    const version = installedMinecraftVersion || minecraftVersion;
    const loader = instanceLoader() ?? "";
    setEditorView("discover");
    setContentQuery("");
    setContentSort("popular");
    setContentVersionFilter(version);
    setContentLoaderFilter(loader);
    setContentCategoryFilter("");
    void loadContentCatalog({ query: "", sort: "popular", version, loader, category: "" });
  }
  function closeProjectDetail() {
    projectDetailRequestId.current += 1;
    setProjectDetail(undefined);
    setDetailLoading(false);
  }
  async function openContentDetail(item: CatalogContent) {
    const requestId = ++projectDetailRequestId.current;
    setDetailLoading(true);
    setProjectDetail({
      source: item.source,
      kind: "content",
      title: item.title,
      author: item.author,
      iconUrl: item.iconUrl,
      summary: item.description,
      body: item.source === "modrinth" ? normalizeProjectMarkdown(item.description) : item.description,
      bodyFormat: item.source === "modrinth" ? "markdown" : "html",
      gallery: item.gallery ?? [],
      websiteUrl: item.websiteUrl,
      content: item,
    });
    try {
      if (item.source === "modrinth") {
        const response = await fetch(`https://api.modrinth.com/v2/project/${encodeURIComponent(item.projectId)}`);
        if (!response.ok) throw new Error("A página do projeto não está disponível.");
        const project = await response.json() as {
          body?: string;
          icon_url?: string;
          source_url?: string;
          wiki_url?: string;
          discord_url?: string;
          gallery?: Array<{ url?: string; featured?: boolean }>;
        };
        setProjectDetail((current) => requestId === projectDetailRequestId.current && current?.content?.projectId === item.projectId ? {
          ...current,
          body: normalizeProjectMarkdown(project.body ?? item.description),
          iconUrl: project.icon_url ?? item.iconUrl,
          gallery: (project.gallery ?? []).sort((left, right) => Number(Boolean(right.featured)) - Number(Boolean(left.featured))).map((image) => image.url ?? "").filter(Boolean),
          websiteUrl: `https://modrinth.com/project/${item.slug || item.projectId}`,
        } : current);
      } else if (services && item.curseForgeId) {
        const description = await requestCurseForgeCatalog<{ data?: string }>(services, { action: "description", modId: item.curseForgeId });
        setProjectDetail((current) => requestId === projectDetailRequestId.current && current?.content?.projectId === item.projectId ? { ...current, body: description.data ?? item.description } : current);
      }
    } catch (error) {
      if (requestId === projectDetailRequestId.current) setNotice(`A descrição completa não pôde ser carregada: ${String(error)}`);
    } finally {
      if (requestId === projectDetailRequestId.current) setDetailLoading(false);
    }
  }
  async function openModpackDetail(pack: CatalogPack) {
    const requestId = ++projectDetailRequestId.current;
    setDetailLoading(true);
    setProjectDetail({
      source: pack.source,
      kind: "modpack",
      title: pack.title,
      author: pack.author,
      iconUrl: pack.iconUrl,
      summary: pack.description,
      body: pack.source === "modrinth" ? normalizeProjectMarkdown(pack.description) : pack.description,
      bodyFormat: pack.source === "modrinth" ? "markdown" : "html",
      gallery: pack.gallery ?? [],
      websiteUrl: pack.websiteUrl ?? `https://modrinth.com/modpack/${pack.slug || pack.projectId}`,
      pack,
    });
    try {
      if (pack.source === "modrinth") {
        const response = await fetch(`https://api.modrinth.com/v2/project/${encodeURIComponent(pack.projectId)}`);
        if (!response.ok) throw new Error("A página do modpack não está disponível.");
        const project = await response.json() as { body?: string; icon_url?: string; gallery?: Array<{ url?: string; featured?: boolean }> };
        setProjectDetail((current) => requestId === projectDetailRequestId.current && current?.pack?.projectId === pack.projectId ? { ...current, body: normalizeProjectMarkdown(project.body ?? pack.description), iconUrl: project.icon_url ?? pack.iconUrl, gallery: (project.gallery ?? []).sort((left, right) => Number(Boolean(right.featured)) - Number(Boolean(left.featured))).map((image) => image.url ?? "").filter(Boolean) } : current);
      } else if (services && pack.curseForgeId) {
        const description = await requestCurseForgeCatalog<{ data?: string }>(services, { action: "description", modId: pack.curseForgeId });
        setProjectDetail((current) => requestId === projectDetailRequestId.current && current?.pack?.projectId === pack.projectId ? { ...current, body: description.data ?? pack.description } : current);
      }
    } catch (error) {
      if (requestId === projectDetailRequestId.current) setNotice(`A descrição completa não pôde ser carregada: ${String(error)}`);
    } finally {
      if (requestId === projectDetailRequestId.current) setDetailLoading(false);
    }
  }
  function rememberContentArtworkData(
    id: string,
    filename: string,
    type: ContentType,
    artwork: ContentArtworkEntry,
  ) {
    setContentArtwork((current) => {
      let stored = current;
      if (activeInstanceRef.current.editingInstance !== id) {
        try {
          stored = JSON.parse(window.localStorage.getItem(`aurora.contentArtwork.${id}`) ?? "{}") as Record<string, ContentArtworkEntry>;
        } catch {
          stored = {};
        }
      }
      const nextArtwork = { ...stored, [contentArtworkKey(type, filename)]: artwork };
      window.localStorage.setItem(`aurora.contentArtwork.${id}`, JSON.stringify(nextArtwork));
      return activeInstanceRef.current.editingInstance === id ? nextArtwork : current;
    });
  }
  function migrateContentArtwork(id: string, migrations: ContentArtworkMigration[]) {
    if (migrations.length === 0) return;
    let stored = activeInstanceRef.current.editingInstance === id ? contentArtwork : {};
    try {
      const persisted = JSON.parse(window.localStorage.getItem(`aurora.contentArtwork.${id}`) ?? "{}") as Record<string, ContentArtworkEntry>;
      stored = { ...stored, ...persisted };
    } catch {
      if (activeInstanceRef.current.editingInstance !== id) stored = {};
    }
    const nextArtwork = { ...stored };
    for (const migration of migrations) {
      const inferredTypes = UPDATE_CONTENT_TYPES.filter((type) => Boolean(nextArtwork[contentArtworkKey(type, migration.fromFilename)]));
      const types = migration.contentType ? [migration.contentType] : inferredTypes.length === 1 ? inferredTypes : [];
      for (const type of types) {
        const previousKey = contentArtworkKey(type, migration.fromFilename);
        const nextKey = contentArtworkKey(type, migration.toFilename);
        const existing = nextArtwork[previousKey];
        const artwork = existing ?? migration.fallback;
        if (!artwork) continue;
        nextArtwork[nextKey] = {
          ...artwork,
          source: artwork.source ?? migration.fallback?.source,
        };
        if (nextKey !== previousKey) delete nextArtwork[previousKey];
      }
    }
    window.localStorage.setItem(`aurora.contentArtwork.${id}`, JSON.stringify(nextArtwork));
    if (activeInstanceRef.current.editingInstance === id) setContentArtwork(nextArtwork);
  }
  function rememberContentArtwork(id: string, filename: string, item: CatalogContent) {
    rememberContentArtworkData(id, filename, contentType, {
      iconUrl: item.iconUrl,
      projectId: item.projectId,
      source: item.source,
      title: item.title,
    });
  }
  async function installContent(item: CatalogContent) {
    const id = editingInstance || targetInstance;
    const gameVersion = installedMinecraftVersion || minecraftVersion;
    if (!id) {
      setNotice("Abra uma instância para instalar conteúdo.");
      return;
    }
    const loader = supportedLoaderFromVersionId(installedVersionId);
    if (contentType === "mod" && !loader) {
      setNotice("Instale Fabric ou Forge nessa instância antes de adicionar mods.");
      return;
    }
    invalidateContentUpdatePlan();
    setBusy(true);
    try {
      let filename: string;
      if (item.source === "modrinth") {
        filename = await invoke<string>("install_modrinth_content", { id, projectId: item.projectId, minecraftVersion: gameVersion, contentType, loader });
      } else {
        if (!services || !item.curseForgeId) throw new Error("O projeto CurseForge é inválido.");
        const files = await requestCurseForgeCatalog<{ data?: CurseForgeFile[] }>(services, {
          action: "files",
          modId: item.curseForgeId,
          gameVersion,
          loader: contentType === "mod" ? loader : undefined,
        });
        const expectedRootExtension = contentType === "mod" ? ".jar" : ".zip";
        const file = (files.data ?? []).find((entry) => entry.fileName.toLowerCase().endsWith(expectedRootExtension));
        if (!file) throw new Error(`Não há arquivo compatível com Minecraft ${gameVersion}${loader ? ` e ${loader}` : ""}.`);
        type InstallCandidate = { file: CurseForgeFile; project: CurseForgeProject; type: ContentType; root: boolean };
        const queue: InstallCandidate[] = [{
          file,
          type: contentType,
          root: true,
          project: {
            id: item.curseForgeId,
            name: item.title,
            slug: item.slug,
            classId: contentType === "mod" ? 6 : contentType === "shader" ? 6552 : 12,
            links: { websiteUrl: item.websiteUrl },
            logo: { thumbnailUrl: item.iconUrl },
          },
        }];
        const seenProjects = new Set<number>([item.curseForgeId]);
        let processed = 0;
        while (processed < queue.length) {
          const level = queue.slice(processed);
          processed = queue.length;
          const dependencyIds = [...new Set(level.flatMap((candidate) => (candidate.file.dependencies ?? [])
            .filter((dependency) => dependency.relationType === 3 && !seenProjects.has(dependency.modId))
            .map((dependency) => dependency.modId)))];
          if (queue.length + dependencyIds.length > 64) throw new Error("A árvore de dependências do projeto é grande demais para instalação automática.");
          for (const dependencyId of dependencyIds) seenProjects.add(dependencyId);
          for (let dependencyIndex = 0; dependencyIndex < dependencyIds.length; dependencyIndex += 32) {
            const requestedIds = dependencyIds.slice(dependencyIndex, dependencyIndex + 32);
            const response = await requestCurseForgeCatalog<{ data?: Array<{ project: CurseForgeProject; files: CurseForgeFile[] }> }>(services, {
              action: "resolveProjects",
              projectIds: requestedIds,
              gameVersion,
              loader,
            });
            const resolvedProjects = response.data ?? [];
            for (const dependencyId of requestedIds) {
              const resolved = resolvedProjects.find((entry) => entry.project.id === dependencyId);
              if (!resolved) throw new Error(`A dependência ${dependencyId} não está disponível.`);
              const dependencyType = curseForgeProjectContentType(resolved.project, "mod");
              const expectedExtension = dependencyType === "mod" ? ".jar" : ".zip";
              const dependencyFile = resolved.files.find((entry) => entry.fileName.toLowerCase().endsWith(expectedExtension));
              if (!dependencyFile) throw new Error(`“${resolved.project.name}” não possui arquivo compatível com esta instância.`);
              queue.push({ file: dependencyFile, project: resolved.project, type: dependencyType, root: false });
            }
          }
        }
        setNotice(`Baixando “${item.title}” e ${Math.max(0, queue.length - 1)} dependência(s) obrigatória(s)…`);
        let mainFilename = file.fileName;
        for (const candidate of queue) {
          const sha1 = candidate.file.hashes?.find((hash) => hash.algo === 1)?.value;
          if (!sha1) throw new Error(`“${candidate.file.fileName}” não possui SHA-1 para validação.`);
          let downloadUrl = candidate.file.downloadUrl;
          if (!downloadUrl) {
            const resolved = await requestCurseForgeCatalog<{ data?: string }>(services, {
              action: "download",
              modId: candidate.project.id,
              fileId: candidate.file.id,
            }).catch(() => undefined);
            downloadUrl = resolved?.data;
          }
          if (downloadUrl) {
            await invoke<string>("install_remote_content", {
              id,
              url: downloadUrl,
              filename: candidate.file.fileName,
              contentType: candidate.type,
              sha1,
            });
          } else {
            await downloadCurseForgeContentAutomatically({
              id,
              pageUrl: curseForgeDownloadPage(candidate.project.slug, candidate.file.id, candidate.type, candidate.project.links?.websiteUrl),
              filename: candidate.file.fileName,
              contentType: candidate.type,
              sha1,
              projectId: candidate.project.id,
              fileId: candidate.file.id,
            });
          }
          rememberContentArtworkData(id, candidate.file.fileName, candidate.type, {
            iconUrl: candidate.project.logo?.thumbnailUrl ?? candidate.project.logo?.url,
            projectId: String(candidate.project.id),
            source: "curseforge",
            title: candidate.project.name,
          });
          if (candidate.root) mainFilename = candidate.file.fileName;
        }
        filename = mainFilename;
      }
      setInstanceContent(await invoke<InstanceContent>("list_instance_content", { id }));
      rememberContentArtwork(id, filename, item);
      setEditorView("installed");
      closeProjectDetail();
      setNotice(`“${filename}” foi instalado em “${id}”.`);
    } catch (error) {
      setDownloadProgress(undefined);
      setNotice(`Não foi possível instalar “${item.title}”: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }
  async function launchMinecraft(instanceId = targetInstance) {
    if (!profile || !capeWardrobe.ready || wardrobeLoading || wardrobeReadyOwner !== profile.uid || !isCurrentSkinOwner(profile.uid)) {
      setNotice("Aguarde a recuperação da aparência antes de iniciar Minecraft.");
      return;
    }
    if (!instanceId) {
      setNotice("Escolha uma instância que já tenha Minecraft instalado.");
      return;
    }
    if (runningInstances.some((running) => running.instanceId === instanceId)) {
      setNotice(`“${instanceId}” já está rodando.`);
      return;
    }
    setBusy(true);
    setNotice(`Montando “${instanceId}” e iniciando Minecraft…`);
    let runId: string | undefined;
    setDoctorFailure((current) => current?.instanceId === instanceId ? undefined : current);
    try {
      const run = await invoke<DoctorRun>("begin_instance_launch", { id: instanceId });
      runId = run.runId;
      const launchProfile = await invoke<InstanceLaunchProfile>("read_instance_launch_profile", { id: instanceId });
      if (!launchProfile.versionId || !launchProfile.minecraftVersion) {
        throw new Error("instale Fabric ou Forge nesta instância antes de iniciar");
      }
      let versionId = launchProfile.versionId;
      const gameVersion = launchProfile.minecraftVersion;
      if (fabricLoaderNeedsUpgrade(versionId, gameVersion)) {
        setNotice(`Atualizando o Fabric Loader de “${instanceId}” para uma versão compatível…`);
        const repaired = await invoke<InstallSummary>("install_fabric", {
          id: instanceId,
          minecraftVersion: gameVersion,
        });
        versionId = repaired.versionId;
        await refresh();
      }
      setTargetInstance(instanceId);
      setInstalledVersionId(versionId);
      setInstalledMinecraftVersion(gameVersion);
      setMinecraftVersion(gameVersion);
      let executable = javaPath.trim();
      const requiredJava = await invoke<number>("required_java_for_instance", {
        id: instanceId,
        minecraftVersion: gameVersion,
      });
      if (executable) {
        try {
          const detectedVersion = await invoke<string>("verify_java", { executable });
          if (javaMajor(detectedVersion) !== requiredJava) executable = "";
        } catch {
          executable = "";
        }
      }
      if (!executable) {
        const runtimes = await invoke<JavaRuntime[]>("discover_java");
        const selected = runtimes.find((runtime) => javaMajor(runtime.version) === requiredJava)
          ?? await invoke<JavaRuntime>("install_java_major", { major: requiredJava });
        executable = selected.executable;
        setJavaRuntimes(runtimes.some((runtime) => runtime.executable === selected.executable) ? runtimes : [...runtimes, selected]);
        setJavaPath(executable);
        window.localStorage.setItem("aurora.javaExecutable", executable);
      }
      if (!isCurrentSkinOwner(profile.uid)) {
        await invoke("cancel_instance_launch", { id: instanceId, runId }).catch(() => undefined);
        return;
      }
      const result = await invoke<LaunchSummary>("launch_instance", {
        id: instanceId,
        runId,
        versionId,
        minecraftVersion: gameVersion,
        javaExecutable: executable,
        nickname: profile?.username ?? "",
        skinUrl: profile?.skinUrl ?? null,
        skinFile: localSkinPath || null,
        capeFile: capeWardrobe.active?.path || null,
        shareCape: capeWardrobe.share,
        capeUrl: capeWardrobe.active?.url || null,
        skinModel: localSkinPath ? equippedSkin?.skinModel ?? profile?.skinModel ?? null : profile?.skinModel ?? null,
      });
      setRunningInstances((current) => [
        ...current.filter((running) => running.instanceId !== instanceId),
        { instanceId, processId: result.processId },
      ]);
      setNotice(
        `“${instanceId}” está rodando (processo ${result.processId}) com o perfil ${result.versionId}${result.coreInstalled ? " e Aurora Core" : ""}${result.skinsInstalled ? " + Skins" : ""}.`,
      );
    } catch (error) {
      if (runId) {
        await invoke("fail_instance_launch", { id: instanceId, runId, failure: String(error) }).catch(() => undefined);
        setDoctorFailure({ instanceId, runId });
      }
      setNotice(`Não foi possível iniciar Minecraft: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }
  async function selectOnlineSkin(skin: { name: string; skinUrl: string; model: "classic" | "slim"; contentHash: string }) {
    if (!profile || busy || wardrobeLoading) return;
    const ownerId = profile.uid;
    setBusy(true);
    setNotice(`Baixando a skin de “${skin.name}”…`);
    try {
      const loaded = await invoke<AppearanceImage>("load_appearance_url", { url: skin.skinUrl, kind: "skin" });
      const image = await appearanceBlob(loaded.dataBase64);
      if (await skinSha256(image) !== skin.contentHash) throw new Error("A skin mudou. Pesquise novamente para obter o PNG atual.");
      if (!isCurrentSkinOwner(ownerId)) return;
      const publicCatalogUrl = publicCatalogSource(skin.skinUrl, skin.contentHash);
      const local = await saveLocalSkin({ ownerId, name: skin.name, skinModel: skin.model, sourceUrl: loaded.url, publicCatalogUrl, image });
      if (!isCurrentSkinOwner(ownerId)) return;
      selectedSkinIdRef.current = local.id;
      setSelectedLocalSkinId(local.id);
      await refreshLocalSkinLibrary(ownerId, local.id);
      if (!isCurrentSkinOwner(ownerId)) return;
      setSkinModel(skin.model);
      setFavoriteName(skin.name);
      setNotice(`Skin “${skin.name}” pronta. Clique em Aplicar skin para usá-la. Não precisa abrir o Minecraft agora.`);
    } catch (error) {
      if (isCurrentSkinOwner(ownerId)) setNotice(`Não foi possível baixar essa skin: ${errorMessage(error)}`);
    } finally {
      setBusy(false);
    }
  }
  async function saveAppearance(event: FormEvent) {
    event.preventDefault();
    if (!profile || !services || wardrobeLoading) return;
    const ownerId = profile.uid;
    setBusy(true);
    let equippedLocally = false;
    try {
      let selected: LocalSkin | undefined = localSkins.find((skin) => skin.id === selectedLocalSkinId);
      if (selected) selected = await cacheSkin(selected);
      else {
        const loaded = await invoke<AppearanceImage>("load_appearance_url", { url: skinUrl, kind: "skin" });
        const image = await appearanceBlob(loaded.dataBase64);
        if (!isCurrentSkinOwner(ownerId)) return;
        selected = await saveLocalSkin({ ownerId, name: favoriteName.trim() || "Minha skin", skinModel, sourceUrl: loaded.url, image });
      }
      if (!selected?.image) throw new Error("Esta skin não está mais na biblioteca. Selecione o PNG novamente.");
      if (!isCurrentSkinOwner(ownerId)) return;
      const file = new File([selected.image], "skin.png", { type: "image/png" });
      await validateAppearanceFile("skin", file);
      const png = await fileAsDataUrl(file);
      const equippedLocalPath = await invoke<string>("save_local_appearance", { userId: ownerId, kind: "skin", dataBase64: png });
      equippedLocally = true;
      if (!isCurrentSkinOwner(ownerId)) return;
      rememberEquippedSkin(ownerId, selected, skinModel, png, equippedLocalPath);
      if (selected.skinModel !== skinModel) await saveLocalSkin({ ...selected, skinModel });
      await refreshLocalSkinLibrary(ownerId, selected.id);
      if (!isCurrentSkinOwner(ownerId)) return;
      let equippedSkinUrl = profile.skinUrl ?? "";
      let equippedAvatarUrl = profile.avatarUrl ?? "";
      try {
        // Let an already dispatched legacy migration finish before the explicit
        // equip writes the new profile, so older metadata cannot win the race.
        await appearanceMigrationPending.current;
        if (!isCurrentSkinOwner(ownerId)) return;
        let onlineSkinUrl: string | undefined;
        try {
          onlineSkinUrl = await uploadAppearanceImage("skin", file);
        } catch {
          if (!isCurrentSkinOwner(ownerId)) return;
          onlineSkinUrl = await verifyPublicCatalogSkin(selected.publicCatalogUrl, file);
        }
        if (!isCurrentSkinOwner(ownerId)) return;
        if (!onlineSkinUrl) throw new Error("Sincronização indisponível.");
        await updateLocalSkinSource(ownerId, selected.id, onlineSkinUrl);
        equippedSkinUrl = onlineSkinUrl;
        equippedAvatarUrl = onlineSkinUrl;
      } catch (error) {
        if (isCurrentSkinOwner(ownerId)) setNotice("Skin equipada neste computador. Clique em Aplicar skin para tentar a sincronização online novamente.");
        return;
      }
      if (!isCurrentSkinOwner(ownerId)) return;
      const next = await saveAuroraAppearance(services, profile, {
        avatarUrl: equippedAvatarUrl || null,
        skinUrl: equippedSkinUrl || null,
        skinModel,
      });
      if (isCurrentSkinOwner(ownerId)) {
        setProfileState(current => current?.uid === next.uid ? { ...next, capeUrl: current.capeUrl } : current);
        await refreshLocalSkinLibrary(ownerId, selected.id);
        if (!isCurrentSkinOwner(ownerId)) return;
        setNotice("Skin equipada, guardada neste computador e sincronizada online.");
      }
    } catch (error) {
      if (isCurrentSkinOwner(ownerId)) setNotice(equippedLocally
        ? `A skin foi equipada neste computador, mas o perfil online não pôde ser atualizado: ${String(error)}`
        : `Não foi possível equipar a skin: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }
  async function uploadAppearanceImage(kind: "skin" | "cape", file: File) {
    if (!profile || !services) throw new Error("Entre na conta Aurora para enviar a imagem.");
    try {
      return await uploadAuroraAppearanceImage(services, kind, file);
    } catch (supabaseError) {
      try {
        return await uploadFirebaseAppearanceImage(services, profile, kind, file);
      } catch (firebaseError) {
        throw new Error(
          `Supabase: ${String(supabaseError)} Firebase Storage: ${String(firebaseError)}`,
        );
      }
    }
  }
  async function importSkinFile(file?: File) {
    if (!profile || !services || !file) return;
    setBusy(true);
    try {
      await validateAppearanceFile("skin", file);
      const local = await saveLocalSkin({
        ownerId: profile.uid,
        name: importSkinName.trim() || file.name.replace(/\.png$/i, "") || "Minha skin",
        skinModel,
        image: file,
      });
      if (!isCurrentSkinOwner(profile.uid)) return;
      await refreshLocalSkinLibrary(profile.uid, local.id);
      if (!isCurrentSkinOwner(profile.uid)) return;
      setWardrobeView("library");
      setImportSkinName("");
      setFavoriteName(local.name);
      setNotice("Skin adicionada à biblioteca local. Clique em Aplicar skin para aplicá-la.");
    } catch (error) {
      if (isCurrentSkinOwner(profile.uid)) setNotice(`Não foi possível adicionar a imagem: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }
  async function importLinkedSkin() {
    if (!profile || !importSkinUrl.trim() || busy || wardrobeLoading) return;
    setBusy(true);
    try {
      if (isRetiredSkinSource(importSkinUrl.trim(), workerUrl)) throw new Error("Esta skin foi retirada do catálogo.");
      const loaded = await invoke<AppearanceImage>("load_appearance_url", { url: importSkinUrl.trim(), kind: "skin" });
      const image = await appearanceBlob(loaded.dataBase64);
      if (!isCurrentSkinOwner(profile.uid)) return;
      const local = await saveLocalSkin({
        ownerId: profile.uid,
        name: importSkinName.trim() || "Minha skin",
        skinModel,
        sourceUrl: loaded.url,
        image,
      });
      await refreshLocalSkinLibrary(profile.uid, local.id);
      if (!isCurrentSkinOwner(profile.uid)) return;
      setFavoriteName(local.name);
      setImportSkinName("");
      setImportSkinUrl("");
      setWardrobeView("library");
      setNotice("Skin adicionada à biblioteca. Clique em Aplicar skin para usá-la no jogo.");
    } catch (error) {
      if (isCurrentSkinOwner(profile.uid)) setNotice(`Não foi possível guardar a skin: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }
  async function useFavoriteSkin(skin: LocalSkinView) {
    if (wardrobeLoading || busy) return;
    setSelectedLocalSkinId(skin.id);
    setSkinUrl(skin.previewUrl);
    setSkinModel(skin.skinModel);
    setFavoriteName(skin.name);
    setNotice(`Skin “${skin.name}” selecionada. Clique em Aplicar skin para aplicá-la.`);
    if (!skin.image) {
      try {
        if (await cacheSkin(skin)) await refreshLocalSkinLibrary(skin.ownerId);
      } catch {
        if (isCurrentSkinOwner(skin.ownerId) && selectedSkinIdRef.current === skin.id) setNotice("O link antigo desta skin não está disponível. Importe o PNG novamente; as outras skins foram preservadas.");
      }
    }
  }
  function showEquippedSkin() {
    setWardrobeView("equipped");
    if (!equippedSkin) return;
    setSelectedLocalSkinId(equippedSkin.id);
    setSkinUrl(equippedSkin.previewUrl);
    setSkinModel(equippedSkin.skinModel);
    setFavoriteName(equippedSkin.name);
  }
  async function removeFavoriteSkin(id: string) {
    if (!profile) return;
    if (id === equippedSkin?.id) {
      setNotice("Equipe outra skin antes de remover a que está em uso.");
      return;
    }
    setBusy(true);
    try {
      await deleteLocalSkin(profile.uid, id);
      if (!isCurrentSkinOwner(profile.uid)) return;
      if (selectedLocalSkinId === id) {
        setSelectedLocalSkinId(equippedSkin?.id ?? "");
        setSkinUrl(equippedSkin?.previewUrl ?? "");
        setSkinModel(equippedSkin?.skinModel ?? profile.skinModel);
      }
      await refreshLocalSkinLibrary(profile.uid);
      if (!isCurrentSkinOwner(profile.uid)) return;
      setNotice("Skin removida deste computador.");
    } catch (error) {
      if (isCurrentSkinOwner(profile.uid)) setNotice(`Não foi possível remover a skin: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }
  async function renameEditingInstance(event: FormEvent) {
    event.preventDefault();
    const oldId = editingInstance;
    const newId = renameDraft.trim();
    if (!oldId || !newId || oldId === newId) return;
    editorOpenRequestRef.current = { generation: editorOpenRequestRef.current.generation + 1, id: "" };
    invalidateContentUpdatePlan();
    setBusy(true);
    try {
      const renamed = await invoke<Instance>("rename_instance", { id: oldId, newId });
      const oldArtworkKey = `aurora.contentArtwork.${oldId}`;
      const storedArtwork = window.localStorage.getItem(oldArtworkKey);
      if (storedArtwork) {
        window.localStorage.setItem(`aurora.contentArtwork.${renamed.id}`, storedArtwork);
        window.localStorage.removeItem(oldArtworkKey);
      }
      editorOpenRequestRef.current = { generation: editorOpenRequestRef.current.generation + 1, id: renamed.id };
      activeInstanceRef.current = { editingInstance: renamed.id, targetInstance: renamed.id };
      setEditingInstance(renamed.id);
      setRenameDraft(renamed.id);
      setTargetInstance(renamed.id);
      await refresh();
      await selectInstance(renamed.id);
      setNotice(`Instância renomeada para “${renamed.id}”.`);
    } catch (error) {
      setNotice(`Não foi possível renomear a instância: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }
  async function deleteInstance(id: string) {
    setDeleteCandidate(undefined);
    setBusy(true);
    try {
      await invoke("delete_instance", { id });
      if (targetInstance === id) {
        setTargetInstance("");
        setInstalledVersionId("");
        setInstalledMinecraftVersion("");
      }
      if (editingInstance === id) closeInstanceEditor();
      await refresh();
      setNotice(`Instância “${id}” excluída.`);
    } catch (error) {
      setNotice(`Não foi possível excluir “${id}”: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }
  async function verifyJava(event: FormEvent) {
    event.preventDefault();
    if (!javaPath.trim()) return;
    setBusy(true);
    try {
      setNotice(
        `Java isolado validado: ${await invoke<string>("verify_java", { executable: javaPath.trim() })}`,
      );
    } catch (error) {
      setNotice(String(error));
    } finally {
      setBusy(false);
    }
  }
  async function discoverJava() {
    setBusy(true);
    try {
      const runtimes = await invoke<JavaRuntime[]>("discover_java");
      setJavaRuntimes(runtimes);
      if (runtimes.length === 0) {
        setNotice(
          "Nenhum Java foi encontrado nos locais usuais. Informe o executável manualmente.",
        );
        return;
      }
      const selected = chooseJavaRuntime(runtimes, installedMinecraftVersion || minecraftVersion);
      setJavaPath(selected.executable);
      window.localStorage.setItem("aurora.javaExecutable", selected.executable);
      setNotice(
        `${runtimes.length} instalação(ões) encontrada(s). Selecionado: ${selected.version}`,
      );
    } catch (error) {
      setNotice(String(error));
    } finally {
      setBusy(false);
    }
  }
  function selectJava(runtime: JavaRuntime) {
    setJavaPath(runtime.executable);
    window.localStorage.setItem("aurora.javaExecutable", runtime.executable);
    setNotice(`${runtime.version} selecionado para o Aurora.`);
  }
  async function installJava(major: number) {
    setBusy(true);
    setInstallingJavaMajor(major);
    setNotice(`Baixando e verificando Java ${major}…`);
    try {
      const runtime = await invoke<JavaRuntime>("install_java_major", { major });
      const runtimes = await invoke<JavaRuntime[]>("discover_java");
      setJavaRuntimes(runtimes.some((item) => item.executable === runtime.executable) ? runtimes : [...runtimes, runtime]);
      selectJava(runtime);
      setJavaInstallOpen(false);
      setNotice(`Java ${major} instalado e pronto para usar.`);
    } catch (error) {
      setNotice(`Não foi possível instalar Java ${major}: ${String(error)}`);
    } finally {
      setInstallingJavaMajor(undefined);
      setBusy(false);
    }
  }
  const connectAppearanceReady = !!profile && capeWardrobe.ready && !wardrobeLoading
    && wardrobeReadyOwner === profile.uid && isCurrentSkinOwner(profile.uid) && !signingOut
    && connectAccountBarrier === "idle";
  const connectLaunchContext = useMemo(() => {
    if (!connectAppearanceReady || !profile || !targetInstance) return undefined;
    return {
      sourceInstanceId: targetInstance,
      javaExecutable: javaPath.trim(),
      nickname: profile.username,
      appearance: {
        skinUrl: profile.skinUrl ?? null,
        skinFile: localSkinPath || null,
        capeFile: capeWardrobe.active?.path || null,
        shareCape: capeWardrobe.share,
        capeUrl: capeWardrobe.active?.url || null,
        skinModel: localSkinPath ? equippedSkin?.skinModel ?? profile.skinModel ?? null : profile.skinModel ?? null,
      },
      contextKey: crypto.randomUUID(),
    };
  }, [connectAppearanceReady, profile?.uid, profile?.username, profile?.skinUrl, capeWardrobe.active, capeWardrobe.share,
    profile?.skinModel, targetInstance, javaPath, localSkinPath, equippedSkin?.skinModel]);
  if (connectAccountBarrier === "blocked") return <StartupScreen
    error="A sessão mudou, mas o encerramento do Connect ainda não foi confirmado. Tente concluir o encerramento antes de continuar."
    onRetry={() => { void accountTransition.current?.retry(); }} />;
  if (connectAccountBarrier === "closing" || sessionStartup.phase === "loading") return <StartupScreen />;
  if (sessionStartup.phase === "error") return <StartupScreen error={sessionStartup.error}
    onRetry={() => { setSessionStartup({ phase: "loading" }); setSessionRetry((value) => value + 1); }}
    onSignOut={() => { void signOutWithConnectCleanup(); }}
  />;
  if (sessionStartup.phase === "ready" && (!profile
    || profile.uid !== sessionStartup.profile.uid
    || profile.uid !== services.auth.currentUser?.uid)) return <StartupScreen />;
  if (sessionStartup.phase !== "ready" || !profile)
    return <AuthScreen services={services} independent={!services.authScope.persistent} authenticate={async (mode, nickname, password) => {
      const controller = sessionController.current;
      if (!controller) throw new Error("A sessão ainda está sendo preparada. Tente novamente.");
      await controller.authenticate(async () => {
        if (mode === "login") { await loginAuroraUser(services, nickname, password); return; }
        const credential = await registerAuroraUser(services, nickname, password);
        setRecoverySettingsUid(credential.user.uid);
      });
    }} />;
  const filteredInstances = instances.filter((instance) =>
    `${instance.displayName ?? ""} ${instance.id}`.toLocaleLowerCase().includes(instanceFilter.trim().toLocaleLowerCase()),
  );
  const sectionTitles: Record<LauncherSection, string> = {
    instances: "Instâncias",
    discover: "Descobrir",
    appearance: "Guarda-roupa",
    java: "Java & engine",
    connect: "Aurora Connect · experimental",
    social: "Amigos e comunidade",
  };
  const sectionIcons: Record<LauncherSection, string> = { instances: "◈", discover: "⌕", appearance: "✦", java: "▣", connect: "⇄", social: "◉" };
  const selectedContentFiles = contentType === "mod"
    ? instanceContent.mods
    : contentType === "shader"
      ? instanceContent.shaderpacks
      : instanceContent.resourcepacks;
  const filteredSkins = localSkins.filter((skin) => skin.name.toLocaleLowerCase().includes(favoriteSearch.trim().toLocaleLowerCase()));
  const visibleInstalledContent = selectedContentFiles.filter((file) =>
    file.name.toLocaleLowerCase().includes(installedContentFilter.trim().toLocaleLowerCase()),
  );
  const selectedInstalledCount = selectedContentFiles.filter((file) => selectedContentNames.includes(file.name)).length;
  const selectedVisibleCount = visibleInstalledContent.filter((file) => selectedContentNames.includes(file.name)).length;
  const editingInstanceSummary = instances.find((instance) => instance.id === editingInstance);
  const editingInstanceIsRunning = runningInstances.some((running) => running.instanceId === editingInstance);
  const currentUpdateScan = contentUpdateScans[contentType];
  const updateCountFor = (type: ContentType) => contentUpdateScans[type]?.items.filter((item) => item.status === "updateAvailable").length ?? 0;
  const visibleModpackUpdate = Boolean(modpackUpdate && (modpackUpdate.source || modpackUpdate.projectId));
  const totalAvailableUpdates = UPDATE_CONTENT_TYPES.reduce((total, type) => total + updateCountFor(type), 0)
    + (visibleModpackUpdate && modpackUpdate?.status === "updateAvailable" ? 1 : 0);
  const totalUpdateAttention = UPDATE_CONTENT_TYPES.reduce((total, type) => total + (contentUpdateScans[type]?.items.filter((item) => item.status === "blocked" || item.status === "unmanaged").length ?? 0), 0)
    + (visibleModpackUpdate && (modpackUpdate?.status === "blocked" || modpackUpdate?.status === "unmanaged") ? 1 : 0);
  const updateScanIssueCount = UPDATE_CONTENT_TYPES.filter((type) => Boolean(contentUpdateScans[type]?.error)).length + (modpackScanError ? 1 : 0);
  const updateTabBadge = updatesScanning ? "…" : totalAvailableUpdates > 0 ? String(totalAvailableUpdates) : updateScanIssueCount > 0 || totalUpdateAttention > 0 ? "!" : "";
  const updateTabLabel = updatesScanning
    ? "Atualizações: verificando conteúdo"
    : totalAvailableUpdates > 0
      ? `Atualizações: ${totalAvailableUpdates} ${totalAvailableUpdates === 1 ? "disponível" : "disponíveis"}`
      : updateScanIssueCount > 0
        ? `Atualizações: ${updateScanIssueCount} etapa${updateScanIssueCount === 1 ? "" : "s"} com erro`
        : totalUpdateAttention > 0
          ? `Atualizações: ${totalUpdateAttention} item${totalUpdateAttention === 1 ? "" : "s"} precisa${totalUpdateAttention === 1 ? "" : "m"} de atenção`
          : "Atualizações";
  const modpackCategoryKey = catalogCategoryKey(catalogSource, "modpack");
  const contentCategoryKey = catalogCategoryKey(contentSource, contentType);
  const modpackCategoryGroups = groupedCatalogCategories(catalogCategories[modpackCategoryKey] ?? []);
  const contentCategoryGroups = groupedCatalogCategories(catalogCategories[contentCategoryKey] ?? []);
  return (
    <>
    <div className={themes.theme.id === DEFAULT_THEME.id ? "launcher-theme-root" : "launcher-theme-root aurora-themed"}
      data-sidebar={themes.theme.layout.sidebar} data-status-side={themes.theme.layout.statusSide}
      data-custom-cursor={themes.theme.cursor.style !== "system" ? "true" : "false"}
      style={{ ...themeVariables(themes.theme), cursor: themeCursorValue(themes.theme.cursor) }}>
    {themes.theme.id !== DEFAULT_THEME.id && <ThemeScene theme={themes.theme} onError={themes.mediaFailed} />}
    {themes.theme.id !== DEFAULT_THEME.id && <ThemeCursor theme={themes.theme} />}
    <main className="shell app-shell">
      <aside className="app-sidebar">
        <div className="brand-lockup">
          <div className="aurora-mark"><ThemeLogo theme={themes.theme} onError={themes.mediaFailed} /></div>
          <div>
            <strong>Aurora</strong>
            <span>Smart Launcher</span>
          </div>
        </div>
        <div className="sidebar-user" title={`Conta Aurora: ${profile.username}`}>
          <SkinHeadAvatar source={equippedSkin?.previewUrl || profile.skinUrl || profile.avatarUrl} />
          <span><strong>{profile.username}</strong><small>{services.authScope.persistent ? "Conta Aurora" : "Sessão independente"}</small></span>
        </div>
        <nav aria-label="Seções do launcher" className="app-nav">
          {themes.theme.layout.navOrder.filter(section => section === "connect" ? connectAvailable && (!socialFeatureBuild || activeSection === "connect" || manualConnectRequested) : section === "social" ? socialFeatureBuild : true).map((section) => (
            <button
              aria-current={activeSection === section ? "page" : undefined}
              className={activeSection === section ? "nav-item active" : "nav-item"}
              key={section}
              onClick={() => {
                setThemesOpen(false);
                setActiveSection(section);
                if (section === "social" && friendInbox && friendInbox.count > 0) setFriendInboxReveal(value => value + 1);
                if (section === "discover" && !catalogLoading && modrinthPacks.length === 0) void loadModrinthPage(0);
              }}
              type="button"
            >
              <span aria-hidden="true"><ThemeNavIcon theme={themes.theme} section={section} fallback={sectionIcons[section]} onError={themes.mediaFailed} /></span>{sectionTitles[section]}
              {section === "social" && friendInbox && friendInbox.count > 0 && <span className="social-request-count" aria-label={`${friendInbox.count} ${friendInbox.count === 1 ? "pedido de amizade recebido" : "pedidos de amizade recebidos"}`}>{friendInbox.count}</span>}
            </button>
          ))}
        </nav>
        <div className="sidebar-bottom">
          <SupportContextButton className="secondary compact sidebar-action" label="Copiar log ou crash" />
          <button className="text-button sidebar-action" disabled={signingOut} aria-haspopup="dialog" onClick={() => setRecoverySettingsUid(profile.uid)}>Recuperação da conta</button>
          {!signingOut && recoverySettingsUid === profile.uid && <RecoveryEmailDialog key={profile.uid} services={services} uid={profile.uid} close={() => setRecoverySettingsUid(undefined)} />}
          <button className="text-button sidebar-action" disabled={signingOut} onClick={() => void signOutWithConnectCleanup()} type="button">{signingOut ? "Encerrando conexão…" : "Sair da conta"}</button>
          <ExitLauncherButton disabled={signingOut || updatesApplying || recoveryBusy} />
        </div>
      </aside>
      <ThemeWorkspace theme={themes.theme} editing={themesOpen} className={`workspace active-${activeSection}`}
        header={<header className="workspace-header">
          <div>
            <h1>{themesOpen ? "Personalização" : sectionTitles[activeSection]}</h1>
          </div>
          <button className="secondary compact" aria-pressed={themesOpen} type="button" onClick={() => setThemesOpen((open) => !open)}>{themesOpen ? "Voltar ao launcher" : "Personalização"}</button>
        </header>}
        decorations={<ThemeDecorations theme={themes.theme} onError={themes.mediaFailed} />}
        footer={<footer>
          <span>{status?.ready ? "Engine pronta" : "Carregando engine"}</span>
          <code>{status?.dataDirectory ?? "Localizando dados…"}</code>
        </footer>}>
        {themesOpen && <ThemeManager library={themes.library} instances={instances.map((item) => ({ id: item.id, name: item.displayName || item.id }))}
          selectedInstanceId={targetInstance} busy={themes.busy} error={themes.error} onSave={themes.save} onPreview={themes.preview}
          onDelete={themes.remove} onReset={themes.reset} onAutomatic={themes.automatic} onUnbind={themes.unbind} />}
        {doctorFailure && <aside className="doctor-failure-banner" aria-label="Diagnóstico de inicialização" role="status">
          <span>Houve uma falha ao iniciar ou executar “{doctorFailure.instanceId}”. O Aurora Doctor pode analisar os dados disponíveis.</span>
          <button className="secondary compact" disabled={busy || updatesApplying} onClick={() => void openEditor(doctorFailure.instanceId, "doctor", doctorFailure.runId)} type="button">{doctorFailure.runId ? "Analisar esta execução" : "Analisar instância"}</button>
          <button className="secondary compact" aria-label="Dispensar aviso de diagnóstico" onClick={() => setDoctorFailure(undefined)} type="button">×</button>
        </aside>}
        <section className="workspace-grid" style={themesOpen ? { display: "none" } : undefined}>
          <div className="workspace-main">
        {socialService && activeSection === "social" && !signingOut && <div className="section-social"><SocialBuildBanner /><SocialView key={profile.uid} service={socialService} accountId={profile.uid} nickname={profile.username}
          inbox={friendInbox} revealInboxRequest={friendInboxReveal} instanceNames={Object.fromEntries(instances.map(instance => [instance.id, instance.displayName || instance.id]))}
          connection={socialConnection} discord={discord} publicInvite={publicInvite}
          onOpenConnectedGame={connectAvailable ? target => {
            setSocialPlay(connectLaunchContext ? socialPlayRequest(target, connectLaunchContext.contextKey) : undefined);
            setGameRevealRequest(value => value + 1); setActiveSection("connect");
          } : undefined}
          onOpenManualConnect={connectAvailable && connectAccountBarrier === "idle" && !signingOut ? () => { setManualConnectRequested(true); setActiveSection("connect"); } : undefined}
          onPrepareDedicatedServer={connectAvailable && socialConnection?.state.maxSessionSeconds === 14400 ? () => { setDedicatedRevealRequest(value => value + 1); setActiveSection("connect"); } : undefined}
          currentModpack={runningInstances.length === 1 ? instances.find(item => item.id === runningInstances[0].instanceId)?.displayName || runningInstances[0].instanceId : undefined} /></div>}
        {connectAvailable && (connectVisited || activeSection === "connect") && <div className="section-connect" hidden={activeSection !== "connect"} inert={signingOut || activeSection !== "connect"}>
          {socialFeatureBuild && <button className="secondary compact" type="button" onClick={() => setActiveSection("social")}>Voltar para amigos e comunidade</button>}
          <ConnectView manualControls={!socialFeatureBuild || manualConnectRequested} launchContext={connectLaunchContext} onInitialReadReady={setConnectViewReady} onChooseSource={() => setActiveSection("instances")} gameRevealRequest={gameRevealRequest} gameRevealReady={!signingOut && activeSection === "connect"} socialPlay={socialPlay} /><NativeVoicePanel disabled={signingOut} revealRequest={dedicatedRevealRequest} revealReady={connectViewReady && activeSection === "connect"} /></div>}
        <article className="panel instances section-instances">
          <div className="panel-heading">
            <div>
              <h2>Meus modpacks</h2>
            </div>
            <div className="panel-heading-actions">
              <span className="count">{instances.length}</span>
              <button className="secondary compact" onClick={() => setShowCreateModal(true)} type="button">+ Nova</button>
            </div>
          </div>
          {instances.length > 0 && (
            <label className="instance-filter">
              <span>Filtrar instâncias</span>
              <input value={instanceFilter} onChange={(event) => setInstanceFilter(event.target.value)} placeholder="Buscar por nome" />
            </label>
          )}
          {instances.length === 0 ? (
            <div className="empty">
              <strong>Nenhuma instância ainda.</strong>
              <span>Crie uma base para receber Minecraft, loader e mods.</span>
            </div>
          ) : filteredInstances.length === 0 ? (
            <div className="empty compact-empty">
              <strong>Nenhuma instância encontrada.</strong>
              <span>Limpe o filtro ou crie uma nova instância.</span>
            </div>
          ) : (
            <div className="instance-list instance-grid">
              {filteredInstances.map((instance) => {
                const isRunning = runningInstances.some((running) => running.instanceId === instance.id);
                return (
                <article className={`instance${targetInstance === instance.id ? " selected" : ""}${isRunning ? " running" : ""}`} key={instance.id}>
                  <button aria-pressed={targetInstance === instance.id} className="instance-select text-button" onClick={() => void selectInstance(instance.id)} type="button">
                    {instance.iconUrl ? <img className="instance-artwork" alt="" src={instance.iconUrl} /> : <span aria-hidden="true" className="orb">✦</span>}
                    <span className="instance-copy">
                      <strong>{instance.displayName || instance.id}</strong>
                      <span>{isRunning ? "Rodando agora" : instance.hasModsDirectory
                        ? instance.hasInstalledVersion ? "Minecraft instalado" : "Aguardando instalação"
                        : "Verificação necessária"}</span>
                    </span>
                  </button>
                  <button className="instance-delete text-button" aria-label={`Excluir ${instance.displayName || instance.id}`} title="Excluir instância" disabled={busy || isRunning} onClick={() => setDeleteCandidate(instance)} type="button">
                    <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round"><path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13M10 10v7m4-7v7" /></svg>
                  </button>
                  <div className="instance-actions">
                    <button className="compact" disabled={busy || !capeWardrobe.ready || wardrobeLoading || wardrobeReadyOwner !== profile.uid || !instance.hasInstalledVersion || isRunning}
                      title={wardrobeLoading || wardrobeReadyOwner !== profile.uid ? "Recuperando sua skin…" : undefined}
                      onClick={() => void launchMinecraft(instance.id)} type="button">{isRunning ? "● Rodando" : "▶ Iniciar"}</button>
                    <button aria-haspopup="dialog" className="secondary compact" onClick={() => void openEditor(instance.id)} type="button">Gerenciar</button>
                    <button aria-haspopup="dialog" className="secondary compact" aria-label={`Mapa de teclas de ${instance.displayName || instance.id}`} onClick={() => void openEditor(instance.id, "keymap")} type="button">Teclas</button>
                  </div>
                </article>
              );})}
            </div>
          )}
          {deleteCandidate && <DeleteInstanceDialog instance={deleteCandidate} onCancel={() => setDeleteCandidate(undefined)}
            onConfirm={() => void deleteInstance(deleteCandidate.id)} disabled={busy || runningInstances.some(game => game.instanceId === deleteCandidate.id)} />}
          {showCreateModal && (
            <section aria-label="Criar uma instância" className="create-modal" role="dialog" aria-modal="true">
              <div className="modal-heading">
                <div><p className="eyebrow">NOVA INSTÂNCIA</p><h2>Prepare seu próximo mundo</h2></div>
                <button aria-label="Fechar criação de instância" className="secondary compact" onClick={() => setShowCreateModal(false)} type="button">Fechar</button>
              </div>
          <form onSubmit={createInstance} className="inline-form">
            <input
              value={instanceName}
              onChange={(event) => setInstanceName(event.target.value)}
              placeholder="ex.: chronicles-1.20.1"
              maxLength={64}
            />
            <button disabled={busy} type="submit">
              Criar instância
            </button>
          </form>
          <form onSubmit={installMinecraft} className="inline-form">
            <select value={targetInstance} onChange={(event) => void selectInstance(event.target.value)} aria-label="Instância para instalar">
              <option value="">Escolha a instância</option>
              {instances.map((instance) => <option key={instance.id} value={instance.id}>{instance.id}</option>)}
            </select>
            <select value={minecraftVersion} onChange={(event) => {
              setMinecraftVersion(event.target.value);
              if (event.target.value === "1.12.2") setLoaderChoice("forge");
            }} aria-label="Versão Minecraft">
              {INSTALLABLE_VERSIONS.map((version) => <option key={version} value={version}>{version}</option>)}
            </select>
            <select value={loaderChoice} onChange={(event) => setLoaderChoice(event.target.value as LoaderChoice)} aria-label="Tipo da instância">
              <option value="fabric" disabled={minecraftVersion === "1.12.2"}>Fabric</option>
              <option value="forge">Forge</option>
            </select>
            <button disabled={busy || !targetInstance || (loaderChoice === "forge" && !DEFAULT_FORGE_VERSION[minecraftVersion])} type="submit">
              Instalar {loaderChoice === "fabric" ? "Fabric" : "Forge"}
            </button>
          </form>
          {!integratedTarget(minecraftVersion, loaderChoice) && (
            <p className="muted">Esta combinação usa os mods compatíveis do catálogo. O menu Aurora e as skins do launcher estão disponíveis nas combinações com Aurora Core e Skins.</p>
          )}
          <p className="muted version-note">Esta versão mostra apenas releases estáveis. Snapshots, betas e NeoForge serão liberados quando a engine oferecer instalação verificável.</p>
            </section>
          )}
        </article>
        <article className="panel catalog-panel section-discover">
          <div className="panel-heading">
            <div>
              <h2>Modpacks</h2>
            </div>
          </div>
          <div aria-label="Fonte do catálogo" className="catalog-tabs" role="group">
            {(["modrinth", "curseforge"] as const).map((source) => <button key={source}
              aria-pressed={catalogSource === source} className={catalogSource === source ? "selected compact" : "secondary compact"}
              onClick={() => {
                if (source === catalogSource) return;
                setCatalogSource(source);
                setModpackCategoryFilter("");
                setModrinthPacks([]);
                setModrinthTotalHits(0);
                setModrinthPage(0);
                void loadModrinthPage(0, modrinthPageSize, source, "");
              }} type="button">{source === "modrinth" ? "Modrinth" : "CurseForge"}</button>)}
          </div>
          <form onSubmit={searchModrinth} className="catalog-search-bar">
            <input aria-label="Buscar modpacks" value={modpackQuery} onChange={(event) => setModpackQuery(event.target.value)} placeholder={`Buscar modpack no ${catalogSource === "modrinth" ? "Modrinth" : "CurseForge"}`} />
            <label>Mostrar<select value={modrinthPageSize} onChange={(event) => { const size = Number(event.target.value); setModrinthPageSize(size); void loadModrinthPage(0, size); }}><option value={10}>10</option><option value={20}>20</option><option value={30}>30</option><option value={50}>50</option></select></label>
            <button disabled={busy} type="submit">Buscar</button>
          </form>
          <div className="content-filter-bar modpack-filter-bar">
            <label>Versão<select value={modpackVersionFilter} onChange={(event) => { setModpackVersionFilter(event.target.value); if (!event.target.value) setModpackLoaderFilter(""); }}><option value="">Todas</option>{[...new Set([minecraftVersion, ...INSTALLABLE_VERSIONS].filter(Boolean))].map((version) => <option key={version} value={version}>{version}</option>)}</select></label>
            <label>Loader<select disabled={!modpackVersionFilter} value={modpackLoaderFilter} onChange={(event) => setModpackLoaderFilter(event.target.value as "" | LoaderChoice)}><option value="">Forge e Fabric</option><option value="fabric">Fabric</option><option value="forge">Forge</option></select></label>
            <label title={catalogCategoryErrors[modpackCategoryKey]}>Categoria<select value={modpackCategoryFilter} onChange={(event) => setModpackCategoryFilter(event.target.value)}><option value="">Todas</option>{!Object.hasOwn(catalogCategories, modpackCategoryKey) && !catalogCategoryErrors[modpackCategoryKey] && <option disabled value="__loading">Carregando categorias…</option>}{catalogCategoryErrors[modpackCategoryKey] && <option disabled value="__error">Categorias indisponíveis</option>}{modpackCategoryGroups.map(({ group, options }) => <optgroup key={group} label={group}>{options.map((category) => <option key={category.value} value={category.value}>{catalogCategoryLabel(category.name)}</option>)}</optgroup>)}</select></label>
            <label>Ordenar<select value={modpackSort} onChange={(event) => setModpackSort(event.target.value as ContentSort)}><option value="popular">Mais baixados</option><option value="relevance">Relevância</option><option value="updated">Atualizados</option></select></label>
            <button className="secondary compact" disabled={busy} onClick={() => void loadModrinthPage(0)} type="button">Aplicar filtros</button>
          </div>
          <div className="catalog-browser" aria-busy={catalogLoading}>
            {modrinthPacks.length > 0 ? (
              <ul className="pack-compact-list" key={`${catalogSource}-${modrinthPage}`} aria-label="Resultados de modpacks">
                {modrinthPacks.map((pack) => <li key={pack.projectId}>
                  <button aria-haspopup="dialog" className="pack-row" disabled={catalogLoading} onClick={() => void openModpackDetail(pack)} type="button">
                    {pack.iconUrl ? <img className="pack-icon" src={pack.iconUrl} alt="" decoding="async" loading="lazy" /> : <span aria-hidden="true" className="orb">◈</span>}
                    <span className="pack-row-copy"><strong>{pack.title}</strong>
                      <small>{[pack.author, pack.loaders.join(" · "), `${pack.downloads.toLocaleString("pt-BR")} downloads`].filter(Boolean).join(" · ")}</small>
                    </span>
                  </button>
                </li>)}
              </ul>
            ) : (
              <div className="editor-empty discover-empty" role="status">
                <strong>{catalogLoading ? "Carregando catálogo…" : catalogError || "Nenhum modpack compatível"}</strong>
                {!catalogLoading && !catalogError && <p>Altere a busca ou os filtros.</p>}
              </div>
            )}
          </div>
          <nav className="catalog-pagination" aria-label="Paginação de modpacks">
            <button className="secondary compact" disabled={catalogLoading || modrinthPage === 0} onClick={() => void loadModrinthPage(modrinthPage - 1)} type="button">← Anterior</button>
            <span role="status">{catalogLoading ? "Buscando…" : modrinthTotalHits ? `Página ${modrinthPage + 1} de ${Math.ceil(modrinthTotalHits / modrinthPageSize)}` : "Sem resultados"}</span>
            <button className="secondary compact" disabled={catalogLoading || (modrinthPage + 1) * modrinthPageSize >= modrinthTotalHits} onClick={() => void loadModrinthPage(modrinthPage + 1)} type="button">Próxima →</button>
          </nav>
        </article>
        {editingInstance && (
            <section aria-label={`Editor da instância ${editingInstance}`} aria-modal="true" className="editor-panel theme-protected" role="dialog" ref={editorPanelRef}
              onKeyDown={event => {
                if (event.key !== "Tab" || event.defaultPrevented || projectDetail || updateReview || !event.currentTarget.contains(event.target as Node)) return;
                const focusable = [...event.currentTarget.querySelectorAll<HTMLElement>('button:not(:disabled), summary, a[href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex="0"]')]
                  .filter(el => el.tabIndex >= 0 && !el.closest("[inert]") && el.checkVisibility());
                const first = focusable[0], last = focusable[focusable.length - 1];
                if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
                if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
              }}>
              <div className="editor-heading">
                <div className="editor-title">
                  {editingInstanceSummary?.iconUrl ? <img className="editor-instance-artwork" alt="" src={editingInstanceSummary.iconUrl} /> : <span className="editor-instance-icon">◈</span>}
                  <div>
                    <p className="eyebrow">INSTÂNCIA</p>
                    <h2 title={editingInstanceSummary?.displayName || editingInstance}>{editingInstanceSummary?.displayName || editingInstance}</h2>
                  </div>
                </div>
                <div className="editor-heading-actions">
                  <details className="editor-options" key={editingInstance}>
                    <summary>Opções</summary>
                    <div className="editor-options-menu">
                      <button className="secondary compact" disabled={busy || updatesApplying || recoveryBusy} onClick={() => void openInstanceFolder(editingInstance)} type="button">Abrir pasta da instância</button>
                      <form className="editor-name-form" onSubmit={renameEditingInstance}>
                        <label>Identificador da pasta<input disabled={busy || updatesScanning || updatesApplying || recoveryBusy || editingInstanceIsRunning} maxLength={64} onChange={(event) => setRenameDraft(event.target.value)} value={renameDraft} /></label>
                        <button className="secondary compact" disabled={busy || updatesScanning || updatesApplying || recoveryBusy || !renameDraft.trim() || renameDraft.trim() === editingInstance || editingInstanceIsRunning} type="submit">Renomear pasta</button>
                      </form>
                    </div>
                  </details>
                  <button aria-label="Fechar editor" className="editor-close" disabled={updatesApplying || recoveryBusy} onClick={closeInstanceEditor} type="button">×</button>
                </div>
              </div>
              <div className="instance-editor-layout">
                <div className="editor-nav" role="tablist" aria-label="Seções da instância" aria-orientation="vertical" inert={recoveryBusy}>
                  {EDITOR_SECTIONS.map(section => <div className="editor-nav-group" key={section.label} role="presentation">
                    <p aria-hidden="true">{section.label}</p>
                    {section.views.map(([view, label]) => <button key={view} aria-controls="editor-view-panel" aria-selected={editorView === view}
                      aria-label={view === "updates" ? updateTabLabel : undefined} id={`editor-view-tab-${view}`}
                      onClick={() => activateEditorView(view)} onKeyDown={event => handleEditorViewKeyDown(event, view)}
                      role="tab" tabIndex={editorView === view ? 0 : -1} type="button">
                      {label}{view === "updates" && updateTabBadge && <span aria-hidden="true" className="editor-update-count">{updateTabBadge}</span>}
                    </button>)}
                  </div>)}
                </div>
                <div aria-labelledby={`editor-view-tab-${editorView}`} className="editor-body" id="editor-view-panel" role="tabpanel" tabIndex={0} key={editingInstance + "/" + editorView}>
                  {(editorView === "installed" || editorView === "discover" || editorView === "updates") && <>
                    <h3 className="editor-content-heading">{editorView === "installed" ? "Conteúdo instalado" : editorView === "discover" ? "Adicionar conteúdo" : "Atualizações de conteúdo"}</h3>
                    <div className="content-tabs" role="group" aria-label="Tipo de conteúdo" inert={recoveryBusy}>
                      <button aria-label={`Mods: ${instanceContent.mods.length} instalados, ${updateCountFor("mod")} atualizações`} aria-pressed={contentType === "mod"} className={contentType === "mod" ? "selected compact" : "secondary compact"} onClick={() => selectContentType("mod")} type="button">Mods <span>{instanceContent.mods.length}</span>{updateCountFor("mod") > 0 && <em>{updateCountFor("mod")} ↑</em>}</button>
                      <button aria-label={`Shaders: ${instanceContent.shaderpacks.length} instalados, ${updateCountFor("shader")} atualizações`} aria-pressed={contentType === "shader"} className={contentType === "shader" ? "selected compact" : "secondary compact"} onClick={() => selectContentType("shader")} type="button">Shaders <span>{instanceContent.shaderpacks.length}</span>{updateCountFor("shader") > 0 && <em>{updateCountFor("shader")} ↑</em>}</button>
                      <button aria-label={`Pacotes de recursos: ${instanceContent.resourcepacks.length} instalados, ${updateCountFor("resourcepack")} atualizações`} aria-pressed={contentType === "resourcepack"} className={contentType === "resourcepack" ? "selected compact" : "secondary compact"} onClick={() => selectContentType("resourcepack")} type="button">Pacotes de recursos <span>{instanceContent.resourcepacks.length}</span>{updateCountFor("resourcepack") > 0 && <em>{updateCountFor("resourcepack")} ↑</em>}</button>
                    </div>
                  </>}
                {editorView === "logs" ? <InstanceLogsPanel instanceId={editingInstance} /> : editorView === "worlds" ? (
                  <WorldBackupsPanel instanceId={editingInstance} running={editingInstanceIsRunning} />
                ) : editorView === "keymap" ? (
                  <Suspense fallback={<p role="status">Carregando mapa de teclas…</p>}>
                    <KeymapPanel key={editingInstance} instanceId={editingInstance} running={runningInstances.some(game => game.instanceId === editingInstance)} />
                  </Suspense>
                ) : editorView === "memory" ? (
                  <LaunchSettingsPanel key={editingInstance} instanceId={editingInstance}
                    busy={busy || recoveryBusy || !!voice?.isBlocking()}
                    running={runningInstances.some(running => running.instanceId === editingInstance)} />
                ) : editorView === "doctor" ? (
                  <DoctorView
                    busy={busy || updatesApplying}
                    instanceId={editingInstance}
                    javaExecutable={javaPath}
                    key={`${editingInstance}:${doctorRunId ?? "current"}`}
                    onSelectRun={setDoctorRunId}
                    onRecovery={(pointId) => { setDoctorRecoveryPointId(pointId); activateEditorView("recovery"); }}
                    onDiagnosticLab={() => activateEditorView("diagnostic-lab")}
                    runId={doctorRunId}
                  />
                ) : editorView === "recovery" ? (
                  <RecoveryView
                    busy={launcherBusy || updatesScanning || updatesApplying}
                    instanceId={editingInstance}
                    key={editingInstance}
                    onChanged={refreshAfterRecovery}
                    onOperationChange={setRecoveryOperation}
                    running={editingInstanceIsRunning}
                    initialPointId={doctorRecoveryPointId}
                    onInitialPointConsumed={() => setDoctorRecoveryPointId(undefined)}
                  />
                ) : editorView === "diagnostic-lab" ? (
                  <DiagnosticLabView
                    busy={launcherBusy || updatesScanning || updatesApplying}
                    instanceId={editingInstance}
                    key={editingInstance}
                    javaExecutable={javaPath}
                    runningIds={runningInstances.map((running) => running.instanceId)}
                    onChanged={refreshAfterRecovery}
                    onOperationChange={setRecoveryOperation}
                    onLaunch={launchMinecraft}
                    onRecovery={(id, pointId) => { void openEditor(id, "recovery").then(() => { if (activeInstanceRef.current.editingInstance === id) setDoctorRecoveryPointId(pointId); }); }}
                  />
                ) : editorView === "installed" ? (
                  <>
                    <div className="editor-toolbar">
                      <label className="editor-filter">
                        <span aria-hidden="true">⌕</span>
                        <input aria-label="Buscar conteúdo instalado" onChange={(event) => setInstalledContentFilter(event.target.value)} placeholder="Buscar nos instalados" value={installedContentFilter} />
                      </label>
                      <div className="editor-bulk-actions">
                        <button className="secondary compact" disabled={busy || updatesScanning || updatesApplying || visibleInstalledContent.length === 0} onClick={() => setSelectedContentNames(selectedVisibleCount === visibleInstalledContent.length ? selectedContentNames.filter((name) => !visibleInstalledContent.some((file) => file.name === name)) : [...new Set([...selectedContentNames, ...visibleInstalledContent.map((file) => file.name)])])} type="button">{selectedVisibleCount === visibleInstalledContent.length && visibleInstalledContent.length > 0 ? "Limpar visíveis" : "Selecionar visíveis"}</button>
                        {selectedInstalledCount > 0 && <>
                        <button className="secondary compact" disabled={busy || updatesScanning || updatesApplying || selectedInstalledCount === 0} onClick={() => void setSelectedContentEnabled(true)} type="button">Ativar selecionados</button>
                        <button className="secondary compact" disabled={busy || updatesScanning || updatesApplying || selectedInstalledCount === 0} onClick={() => void setSelectedContentEnabled(false)} type="button">Desativar selecionados</button>
                        <button className="danger compact" disabled={busy || updatesScanning || updatesApplying || selectedInstalledCount === 0} onClick={() => void removeSelectedContent()} type="button">Desinstalar ({selectedInstalledCount})</button>
                        </>}
                      </div>
                    </div>
                    <div className="installed-content content-table">
                      {selectedContentFiles.length === 0 ? (
                        <div className="editor-empty"><span>✦</span><strong>Nada instalado aqui</strong><p>Use “Adicionar” para encontrar conteúdo no Modrinth ou CurseForge.</p><button onClick={showContentCatalog} type="button">Encontrar conteúdo</button></div>
                      ) : visibleInstalledContent.length === 0 ? (
                        <div className="editor-empty compact-empty"><strong>Nenhum resultado</strong><p>Tente buscar por outra parte do nome do arquivo.</p></div>
                      ) : visibleInstalledContent.map((file) => (
                        <div className={selectedContentNames.includes(file.name) ? "content-row selected" : "content-row"} key={file.name}>
                          <input aria-label={`Selecionar ${file.name}`} checked={selectedContentNames.includes(file.name)} className="content-select" disabled={busy || updatesScanning || updatesApplying} onChange={() => toggleContentSelection(file.name)} onClick={(event) => event.stopPropagation()} type="checkbox" />
                          <ContentArtwork compact iconUrl={contentArtwork[contentArtworkKey(contentType, file.name)]?.iconUrl} title={contentArtwork[contentArtworkKey(contentType, file.name)]?.title ?? contentDisplayName(file.name)} type={contentType} />
                          <button aria-pressed={selectedContentNames.includes(file.name)} className="content-file-details content-file-select" onClick={() => toggleContentSelection(file.name)} title={file.name} type="button">
                            <strong>{contentArtwork[contentArtworkKey(contentType, file.name)]?.title ?? contentDisplayName(file.name)}</strong>
                            <small>{file.name}</small>
                          </button>
                          <span className={`content-status ${file.enabled ? "enabled" : ""}`}>{file.enabled ? "Ativo" : "Pausado"}</span>
                          <label className="content-toggle" onClick={(event) => event.stopPropagation()}>
                            <input aria-label={`${file.enabled ? "Desativar" : "Ativar"} ${file.name}`} checked={file.enabled} disabled={busy || updatesScanning || updatesApplying} onChange={(event) => void setContentEnabled(file, event.target.checked)} type="checkbox" />
                            <span aria-hidden="true" />
                          </label>
                        </div>
                      ))}
                    </div>
                  </>
                ) : editorView === "updates" ? (
                  <ContentUpdatesView
                    applying={updatesApplying}
                    busy={busy}
                    contentType={contentType}
                    filter={contentUpdateFilter}
                    lastBackupAvailable={lastBackupAvailable}
                    modpackScanError={modpackScanError}
                    modpackUpdate={modpackUpdate}
                    onCheckAgain={() => void scanAllContentUpdates()}
                    onFilterChange={setContentUpdateFilter}
                    onQueryChange={setContentUpdateQuery}
                    onReviewItems={(items) => requestUpdateReview({ kind: "content", items })}
                    onReviewModpack={(modpack) => requestUpdateReview({ kind: "modpack", modpack })}
                    onReviewRollback={() => requestUpdateReview({ kind: "rollback" })}
                    onSelectAvailable={selectAvailableUpdates}
                    onToggleSelection={toggleUpdateSelection}
                    running={editingInstanceIsRunning}
                    query={contentUpdateQuery}
                    scan={currentUpdateScan}
                    scanIssueCount={updateScanIssueCount}
                    scanning={updatesScanning}
                    selectedFilenames={selectedUpdateFilenames}
                    totalAvailable={totalAvailableUpdates}
                    totalAttention={totalUpdateAttention}
                  />
                ) : (
                  <div className="editor-discover">
                    <div className="content-source-tabs" role="tablist" aria-label="Fonte dos downloads">
                      <button aria-selected={contentSource === "modrinth"} className={contentSource === "modrinth" ? "active" : ""} onClick={() => { setContentSource("modrinth"); setContentCategoryFilter(""); void loadContentCatalog({ source: "modrinth", query: "", category: "" }); }} role="tab" type="button">Modrinth</button>
                      <button aria-selected={contentSource === "curseforge"} className={contentSource === "curseforge" ? "active" : ""} onClick={() => { setContentSource("curseforge"); setContentCategoryFilter(""); void loadContentCatalog({ source: "curseforge", query: "", category: "" }); }} role="tab" type="button">CurseForge</button>
                    </div>
                    <form onSubmit={searchContent} className="editor-search-form">
                      <span aria-hidden="true">⌕</span>
                      <input autoFocus value={contentQuery} onChange={(event) => setContentQuery(event.target.value)} placeholder={contentType === "mod" ? "Pesquisar mods" : contentType === "shader" ? "Pesquisar shaders" : "Pesquisar pacotes de recursos"} />
                      <button disabled={busy} type="submit">Pesquisar</button>
                    </form>
                    <div className="content-filter-bar">
                      <label>Versão<select value={contentVersionFilter} onChange={(event) => { setContentVersionFilter(event.target.value); if (!event.target.value) setContentLoaderFilter(""); }}><option value="">Todas</option>{[...new Set([installedMinecraftVersion, minecraftVersion, ...INSTALLABLE_VERSIONS].filter(Boolean))].map((version) => <option key={version} value={version}>{version}</option>)}</select></label>
                      {contentType === "mod" && <label>Loader<select disabled={!contentVersionFilter} value={contentLoaderFilter} onChange={(event) => setContentLoaderFilter(event.target.value as "" | LoaderChoice)}><option value="">Todos</option><option value="fabric">Fabric</option><option value="forge">Forge</option></select></label>}
                      <label title={catalogCategoryErrors[contentCategoryKey]}>Categoria<select value={contentCategoryFilter} onChange={(event) => setContentCategoryFilter(event.target.value)}><option value="">Todas</option>{!Object.hasOwn(catalogCategories, contentCategoryKey) && !catalogCategoryErrors[contentCategoryKey] && <option disabled value="__loading">Carregando categorias…</option>}{catalogCategoryErrors[contentCategoryKey] && <option disabled value="__error">Categorias indisponíveis</option>}{contentCategoryGroups.map(({ group, options }) => <optgroup key={group} label={group}>{options.map((category) => <option key={category.value} value={category.value}>{catalogCategoryLabel(category.name)}</option>)}</optgroup>)}</select></label>
                      <label>Ordenar<select value={contentSort} onChange={(event) => setContentSort(event.target.value as ContentSort)}><option value="popular">Mais baixados</option><option value="relevance">Relevância</option><option value="updated">Atualizados</option></select></label>
                      <button className="secondary compact" disabled={busy} onClick={() => void loadContentCatalog()} type="button">Aplicar filtros</button>
                    </div>
                    {contentResults.length > 0 ? (
                      <div className="editor-catalog-results">
                        {contentResults.map((item) => (
                          <button className="editor-catalog-card" key={`${item.source}-${item.projectId}`} onClick={() => void openContentDetail(item)} type="button">
                            <ContentArtwork iconUrl={item.iconUrl} title={item.title} type={contentType} />
                            <div>
                              <strong>{item.title}<em>{item.source === "modrinth" ? "Modrinth" : "CurseForge"}</em></strong>
                              <p>{item.description || "Sem descrição"}</p>
                              <small>{item.downloads.toLocaleString("pt-BR")} downloads{item.author ? ` · por ${item.author}` : ""}</small>
                            </div>
                            <span className="catalog-card-open">Ver página →</span>
                          </button>
                        ))}
                      </div>
                    ) : (
                      <div className="editor-empty discover-empty"><span>{busy ? "…" : "⌕"}</span><strong>{busy ? "Carregando catálogo" : "Nenhum resultado compatível"}</strong><p>Os mais baixados aparecem automaticamente; use os filtros para refinar.</p></div>
                    )}
                  </div>
                )}
                </div>
              </div>
            </section>
          )}
          {updateReview && (
            <UpdateReviewDialog
              busy={updatesApplying}
              error={updateReviewError}
              instanceName={editingInstanceSummary?.displayName || editingInstance}
              loader={currentUpdateScan?.loader || instanceLoader()}
              minecraftVersion={currentUpdateScan?.minecraftVersion || installedMinecraftVersion || minecraftVersion}
              onCancel={() => { setUpdateReview(undefined); setUpdateReviewError(""); }}
              onConfirm={() => void applyReviewedUpdate()}
              request={updateReview}
            />
          )}
          {projectDetail && (
            <ProjectDialog title={projectDetail.title} onClose={closeProjectDetail}>
              <header className="project-detail-header">
                <div className="project-detail-identity">
                  {projectDetail.iconUrl ? <img alt="" src={projectDetail.iconUrl} /> : <span>◈</span>}
                  <div><p className="eyebrow">{projectDetail.source === "modrinth" ? "MODRINTH" : "CURSEFORGE"} · {projectDetail.kind === "modpack" ? "MODPACK" : contentType === "mod" ? "MOD" : contentType === "shader" ? "SHADER" : "PACOTE DE RECURSOS"}</p><h2>{projectDetail.title}</h2>{projectDetail.author && <small>por {projectDetail.author}</small>}</div>
                </div>
                <button aria-label="Fechar página do projeto" className="editor-close" onClick={closeProjectDetail} type="button">×</button>
              </header>
              <div className="project-detail-scroll">
                {projectDetail.gallery[0] && <img className="project-hero" src={projectDetail.gallery[0]} alt={`Destaque de ${projectDetail.title}`} />}
                <Suspense fallback={<p role="status">Carregando descrição…</p>}>
                  <ProjectDescription summary={projectDetail.summary} body={projectDetail.body} bodyFormat={projectDetail.bodyFormat} loading={detailLoading} />
                </Suspense>
              </div>
              <footer className={projectDetail.kind === "modpack" ? "project-detail-actions modpack-install-actions" : "project-detail-actions"}>
                {projectDetail.websiteUrl && <a href={projectDetail.websiteUrl} rel="noreferrer" target="_blank">Abrir site oficial</a>}
                {projectDetail.kind === "content" && <small>Instalação compatível com Minecraft {installedMinecraftVersion || minecraftVersion}{contentType === "mod" && instanceLoader() ? ` · ${instanceLoader()}` : ""}</small>}
                {projectDetail.kind === "modpack" && projectDetail.pack && <label className="detail-version-select">Minecraft<select disabled={busy} value={packVersions[projectDetail.pack.projectId] ?? preferredPackVersion(projectDetail.pack, minecraftVersion)} onChange={(event) => setPackVersions((current) => ({ ...current, [projectDetail.pack!.projectId]: event.target.value }))}>{(projectDetail.pack.versions.length ? projectDetail.pack.versions : [minecraftVersion]).map((version) => <option key={version} value={version}>{version}</option>)}</select></label>}
                <span />
                {projectDetail.kind === "content" && projectDetail.content && <button disabled={busy} onClick={() => { closeProjectDetail(); void installContent(projectDetail.content!); }} type="button">{contentType === "mod" ? "Instalar com dependências" : "Instalar na instância"}</button>}
                {projectDetail.kind === "modpack" && projectDetail.pack && <button disabled={busy} onClick={() => {
                  closeProjectDetail();
                  void createAndInstallModrinth(projectDetail.pack!);
                }} type="button">Criar e instalar</button>}
              </footer>
            </ProjectDialog>
          )}
          </div>
        <aside className="side-stack">
          <article className="panel section-java">
            {activeSection === "java" && <div hidden={javaInstallOpen}><GlobalMemoryPanel busy={busy || recoveryBusy || !!voice?.isBlocking()} /></div>}
            <div className="java-manager-heading">
              <div><h2>{javaInstallOpen ? "Instalar Java" : "Versões do Java"}</h2><p className="muted">{javaInstallOpen ? "Escolha a versão que deseja instalar." : "Seleção automática ao iniciar o jogo."}</p></div>
              <div className="java-manager-actions">
                {!javaInstallOpen && <button className="secondary compact" disabled={busy} onClick={() => void discoverJava()} type="button">↻ Verificar novamente</button>}
                <button className={javaInstallOpen ? "secondary compact" : "compact"} aria-expanded={javaInstallOpen} aria-controls={javaInstallOpen ? "java-install-picker" : undefined}
                  disabled={busy} onClick={() => setJavaInstallOpen(current => !current)} type="button">{javaInstallOpen ? "Voltar para versões" : "+ Instalar Java"}</button>
              </div>
            </div>
            {javaInstallOpen && (
              <section id="java-install-picker" className="java-install-picker" aria-label="Versões de Java disponíveis para instalação">
                <div className="java-install-copy"><strong>Instalar um runtime gerenciado</strong><small>O Aurora baixa o Eclipse Temurin, verifica a integridade e mantém cada versão isolada.</small></div>
                <div className="java-recommended-options" aria-label="Versões recomendadas">
                  {RECOMMENDED_JAVA_MAJORS.filter((major) => availableJavaMajors.includes(major)).map((major) => {
                    const installed = javaRuntimes.some((runtime) => javaMajor(runtime.version) === major);
                    return <button aria-pressed={javaInstallMajor === major} className={javaInstallMajor === major ? "java-version-choice selected" : "java-version-choice"} disabled={busy} key={major} onClick={() => setJavaInstallMajor(major)} type="button"><strong>Java {major}</strong><small>{installed ? "Instalado" : javaVersionDescription(major)}</small></button>;
                  })}
                </div>
                <div className="java-install-controls">
                  <label><span>Versão</span><select disabled={busy} value={javaInstallMajor} onChange={(event) => setJavaInstallMajor(Number(event.target.value))}>{availableJavaMajors.map((major) => <option key={major} value={major}>Java {major}{javaLtsMajors.includes(major) ? " · LTS" : major === 16 ? " · recomendado para este modpack" : ""}</option>)}</select></label>
                  <button disabled={busy} onClick={() => void installJava(javaInstallMajor)} type="button">{installingJavaMajor === javaInstallMajor ? `Instalando Java ${javaInstallMajor}…` : `Instalar Java ${javaInstallMajor}`}</button>
                </div>
              </section>
            )}
            {!javaInstallOpen && <>
            <div className="java-runtime-list">
              {javaRuntimes.length === 0 ? <div className="editor-empty compact-empty"><strong>Nenhum Java encontrado</strong><p>Use “Instalar Java” para baixar uma versão gerenciada pelo Aurora.</p></div> : javaRuntimes.map((runtime) => {
                const selected = runtime.executable === javaPath;
                const major = javaMajor(runtime.version);
                return <button aria-pressed={selected} className={selected ? "java-runtime selected" : "java-runtime"} key={runtime.executable} onClick={() => selectJava(runtime)} type="button"><span className="java-version-mark">{major || "J"}</span><span><strong>{major ? `Java ${major}` : "Java detectado"}</strong><small>{runtime.version}</small><code title={runtime.executable}>{runtime.executable}</code></span><em>{selected ? "Em uso" : "Usar"}</em></button>;
              })}
            </div>
            <details className="java-manual"><summary>Usar um executável manualmente</summary><form onSubmit={verifyJava} className="java-form"><input value={javaPath} onChange={(event) => { setJavaPath(event.target.value); window.localStorage.setItem("aurora.javaExecutable", event.target.value); }} placeholder="C:\\Java\\bin\\java.exe" /><button className="secondary" disabled={busy} type="submit">Validar Java</button></form></details>
            </>}
          </article>
          {activeSection === "appearance" && <ThemeSurface as="article" surfaceId="wardrobe" className="panel section-appearance wardrobe-panel">
            <div className="wardrobe-heading">
              <nav className="wardrobe-tabs" aria-label="Guarda-roupa">
                <button className={wardrobeView === "explore" ? "compact" : "secondary compact"} aria-pressed={wardrobeView === "explore"} onClick={() => setWardrobeView("explore")} type="button">Explorar</button>
                <button className={wardrobeView === "library" ? "compact" : "secondary compact"} aria-pressed={wardrobeView === "library"} onClick={() => { setWardrobeView("library"); setFavoriteSearch(""); }} type="button">Minhas skins <span>({localSkins.length})</span></button>
                <button className={wardrobeView === "equipped" ? "compact" : "secondary compact"} aria-pressed={wardrobeView === "equipped"} onClick={showEquippedSkin} type="button">Equipada</button>
                <button className={wardrobeView === "cape" ? "compact" : "secondary compact"} aria-pressed={wardrobeView === "cape"} onClick={() => setWardrobeView("cape")} type="button">Capa</button>
                <button className={wardrobeView === "import" ? "compact" : "secondary compact"} aria-pressed={wardrobeView === "import"} onClick={() => setWardrobeView("import")} type="button">Importar</button>
              </nav>
            </div>
            <div className="wardrobe-layout">
              <div className="wardrobe-controls">
                {wardrobeLoading && <p className="muted" role="status">Recuperando sua biblioteca e a skin equipada…</p>}
                {wardrobeView === "cape" && <CapePanel wardrobe={capeWardrobe} />}
                {wardrobeView === "explore" && (
                  <OnlineSkinSearch disabled={busy || wardrobeLoading} workerUrl={workerUrl} onSelect={(skin) => void selectOnlineSkin(skin)} />
                )}
                {wardrobeView === "equipped" && <section className="equipped-skin-panel" aria-label="Skin equipada no jogo">
                  <h2>Skin equipada</h2>
                  {equippedSkin ? <>
                    <SkinThumbnail src={equippedSkin.previewUrl} model={equippedSkin.skinModel} name={equippedSkin.name} className="equipped-skin-thumbnail" />
                    <h3>{equippedSkin.name}</h3>
                    <p className="muted">Modelo {equippedSkin.skinModel === "slim" ? "Slim" : "Clássico"} · Salva neste computador</p>
                  </> : <>
                    <p className="muted">{wardrobeLoading ? "Carregando a skin equipada…" : profile.skinUrl ? "Não foi possível recuperar o PNG da skin equipada. Tente novamente ou importe o arquivo salvo." : "Escolha uma skin e clique em Aplicar skin para usá-la no jogo."}</p>
                    {!wardrobeLoading && (profile.skinUrl || window.localStorage.getItem(`aurora.localSkinPath.${profile.uid}`)) && <button className="secondary compact" disabled={busy} onClick={() => setWardrobeRetry((value) => value + 1)} type="button">Tentar recuperar novamente</button>}
                  </>}
                </section>}
                {wardrobeView === "import" && <section className="skin-import" aria-label="Importar skin">
                  <h2>Importar skin</h2>
                  <div className="import-tabs">
                    <button aria-pressed={skinAddMode === "file"} className={skinAddMode === "file" ? "compact" : "secondary compact"} onClick={() => setSkinAddMode("file")} type="button">Arquivo PNG</button>
                    <button aria-pressed={skinAddMode === "url"} className={skinAddMode === "url" ? "compact" : "secondary compact"} onClick={() => setSkinAddMode("url")} type="button">URL da imagem</button>
                  </div>
                  <label>Nome da skin (opcional)<input value={importSkinName} disabled={busy || wardrobeLoading} onChange={(event) => setImportSkinName(event.target.value)} maxLength={32} placeholder="Minha skin" /></label>
                  {skinAddMode === "file" ? <label className="file-input">Escolher arquivo PNG<input accept="image/png" disabled={busy || wardrobeLoading} onChange={(event) => void importSkinFile(event.target.files?.[0])} type="file" /></label> : <>
                    <label>Link do PNG<input type="url" value={importSkinUrl} disabled={busy || wardrobeLoading} onChange={(event) => setImportSkinUrl(event.target.value)} placeholder="https://…" /></label>
                    <button className="secondary compact" disabled={busy || wardrobeLoading || !importSkinUrl.trim()} onClick={() => void importLinkedSkin()} type="button">Adicionar à biblioteca</button>
                  </>}
                </section>}
                {wardrobeView === "library" && <section className="wardrobe-saved" aria-label="Biblioteca de skins">
                  <h2>Minhas skins</h2>
                  <input aria-label="Pesquisar nas suas skins" value={favoriteSearch} onChange={(event) => setFavoriteSearch(event.target.value)} placeholder="Pesquisar nas suas skins" />
                  <div className="skin-library wardrobe-library">
                    {filteredSkins.map((skin) => (
                      <div className={selectedLocalSkinId === skin.id ? "skin-card active" : "skin-card"} key={skin.id}>
                        <button className="secondary wardrobe-select" aria-pressed={selectedLocalSkinId === skin.id} disabled={busy || wardrobeLoading} onClick={() => void useFavoriteSkin(skin)} type="button">
                          <SkinThumbnail src={skin.previewUrl} name={skin.name} model={skin.skinModel} /><strong>{skin.name}</strong><small>{skin.skinModel === "slim" ? "Slim" : "Clássico"}{equippedSkin?.id === skin.id ? " · Equipada" : ""}</small>
                        </button>
                        <button className="secondary compact wardrobe-remove" aria-label={`Remover ${skin.name}`} disabled={busy || wardrobeLoading || equippedSkin?.id === skin.id} onClick={() => void removeFavoriteSkin(skin.id)} type="button">Remover</button>
                      </div>
                    ))}
                    {filteredSkins.length === 0 && !wardrobeLoading && <p className="muted empty-library">{localSkins.length ? "Nenhuma skin com esse nome." : "Sua biblioteca está vazia. Explore o catálogo ou importe um PNG."}</p>}
                  </div>
                </section>}
              </div>
              {wardrobeView !== "cape" && <div className="skin-preview-stage">
                <h2>Prévia</h2>
                <SkinPreview skinModel={skinModel} skinUrl={skinUrl} />
                {skinUrl && <strong>{favoriteName || localSkins.find(skin => skin.id === selectedLocalSkinId)?.name || profile.username}</strong>}
                <label className="model-row">Modelo<select value={skinModel} disabled={busy || wardrobeLoading || wardrobeView === "equipped"} onChange={(event) => setSkinModel(event.target.value as "classic" | "slim")}><option value="classic">Clássico</option><option value="slim">Slim</option></select></label>
                <button className="compact theme-surface-action" disabled={busy || wardrobeLoading || !skinUrl.trim()} onClick={() => void saveAppearance({ preventDefault() {} } as FormEvent)} type="button">Aplicar skin</button>
              </div>}
            </div>
          </ThemeSurface>}
          {coreConnection && (
            <article className="panel status">
              <p className="eyebrow">AURORA CORE</p>
              <p>{coreConnection.kind === "connected" ? `Conectado: ${coreConnection.loader} ${coreConnection.minecraft_version}` : "Aurora Core desconectado."}</p>
            </article>
          )}
        </aside>
        </section>
      </ThemeWorkspace>
      {downloadProgress && (
        <aside className="download-toast" aria-live="polite">
          <header className="download-toast-header">
            <span className="download-pulse" aria-hidden="true" />
            <strong title={downloadProgress.label}>{downloadProgress.label}</strong>
            <span className="download-total-badge">
              {Math.round(downloadProgress.totalPercent ?? downloadProgress.percent)}% total
            </span>
          </header>
          <div className="download-toast-meta">
            <span>
              {downloadProgress.activeDownloads > 1
                ? `${downloadProgress.activeDownloads} downloads simultâneos`
                : downloadProgress.totalFiles > 0
                  ? `${downloadProgress.completedFiles}/${downloadProgress.totalFiles} arquivos`
                  : "Preparando download"}
            </span>
            {downloadProgress.bytesPerSecond > 0 && (
              <span>{formatBytes(downloadProgress.bytesPerSecond)}/s</span>
            )}
          </div>
          <section className="download-progress-group" aria-label="Progresso total">
            <div className="download-progress-label">
              <span>{downloadProgress.operationId ? "Progresso da cópia" : "Instalação completa"}</span>
              <b>{Math.round(downloadProgress.totalPercent ?? downloadProgress.percent)}%</b>
            </div>
            <progress className="total-progress" max={100} value={downloadProgress.totalPercent ?? downloadProgress.percent} />
          </section>
          {downloadProgress.totalFiles > 0 && (
            <section className="download-progress-group" aria-label="Progresso do arquivo atual">
              <div className="download-progress-label">
                <span>Arquivo atual</span>
                <b>{Math.round(downloadProgress.itemPercent ?? 0)}%</b>
              </div>
              <progress className="item-progress" max={100} value={downloadProgress.itemPercent ?? 0} />
              {(downloadProgress.itemDownloadedBytes > 0 || downloadProgress.itemTotalBytes) && (
                <small>
                  {formatBytes(downloadProgress.itemDownloadedBytes)}
                  {downloadProgress.itemTotalBytes ? ` de ${formatBytes(downloadProgress.itemTotalBytes)}` : ""}
                </small>
              )}
            </section>
          )}
        </aside>
      )}
      {notice && (
        <aside
          className="notice-toast"
          onMouseEnter={() => setNoticeHovered(true)}
          onMouseLeave={() => setNoticeHovered(false)}
          aria-live="polite"
        >
          <span>{notice}</span>
          {/erro|falh|inválid|não foi possível|incompatível|indisponível/i.test(notice) && <SupportContextButton key={notice} label="Copiar erro" source="Launcher" log={notice} />}
          <button aria-label="Fechar mensagem" onClick={() => setNotice("")} type="button">×</button>
        </aside>
      )}
    </main>
    </div>
    </>
  );
}

export default function App() {
  return <VoiceAnnouncementProvider><LauncherStartup /></VoiceAnnouncementProvider>;
}
