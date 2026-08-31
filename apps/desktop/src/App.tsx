import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { register } from "@tauri-apps/plugin-global-shortcut";
import { FormEvent, useEffect, useRef, useState } from "react";
import DOMPurify from "dompurify";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { IdleAnimation, SkinViewer } from "skinview3d";
import {
  bundledFirebaseConfig,
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
import { askAuroraAssistant, requestCurseForgeCatalog, uploadAuroraAppearanceImage } from "./lib/edge";
import { deleteLocalSkin, listLocalSkins, saveLocalSkin, type LocalSkin } from "./lib/localSkins";

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
type InstanceLog = { filename: string; lines: string[] };
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
type EditorView = "installed" | "discover";
type ContentSort = "popular" | "relevance" | "updated";
type CatalogContent = ModrinthContent & {
  source: CatalogSource;
  curseForgeId?: number;
  websiteUrl?: string;
  gallery?: string[];
};
type CurseForgeProject = {
  id: number;
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
  fileName: string;
  displayName?: string;
  downloadUrl?: string;
  hashes?: Array<{ algo: number; value: string }>;
};
type CurseForgeSearchResponse = { data?: CurseForgeProject[]; pagination?: { totalCount?: number } };
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
  pack?: ModrinthPack;
};
type ContentArtworkEntry = { iconUrl?: string; projectId: string; title: string };
type ResolvedContentArtwork = ContentArtworkEntry & { filename: string };
type LoaderChoice = "fabric" | "forge";
type LauncherSection = "instances" | "discover" | "appearance" | "java";
type CatalogSource = "modrinth" | "curseforge";
type LaunchSummary = {
  processId: number;
  versionId: string;
  coreInstalled: boolean;
  companionInstalled: boolean;
};
type DownloadProgress = {
  label: string;
  percent: number;
  totalPercent: number;
  itemPercent: number;
  itemDownloadedBytes: number;
  itemTotalBytes?: number;
  downloadedBytes: number;
  totalBytes?: number;
  completedFiles: number;
  totalFiles: number;
  activeDownloads: number;
  bytesPerSecond: number;
};
type ManualDownloadResult = {
  instanceId: string;
  filename: string;
  status: "downloading" | "completed" | "failed";
  error?: string;
};
type SpeechBoundary = { offsetMs: number; durationMs: number; text: string };
type SpeechResult = { audioBase64: string; mimeType: string; boundaries: SpeechBoundary[] };
type IpcEvent =
  | { kind: "Connected"; loader: string; minecraftVersion: string }
  | { kind: "OverlayRequested" }
  | { kind: "AssistantRequest"; requestId: string; message: string; screenshotBase64?: string }
  | { kind: "AssistantListenRequested"; requestId: string }
  | { kind: "Telemetry"; fps: number; mspt: number; usedMemoryMb: number; dimension?: string }
  | { kind: "Disconnected" };
type IpcSessionEvent = { processId: number; event: IpcEvent };
type AuthMode = "login" | "register";
type SkinAddMode = "file" | "url" | "username";
type LocalSkinView = LocalSkin & { previewUrl: string };
type OverlayMessage = { role: "user" | "assistant"; text: string };
type SpeechRecognitionLike = {
  lang: string;
  interimResults: boolean;
  onresult: ((event: { results: ArrayLike<ArrayLike<{ transcript: string }>> }) => void) | null;
  onerror: (() => void) | null;
  onend: (() => void) | null;
  start: () => void;
  stop: () => void;
};
type SpeechWindow = Window & {
  SpeechRecognition?: new () => SpeechRecognitionLike;
  webkitSpeechRecognition?: new () => SpeechRecognitionLike;
};
const COMPANION_SUPPORTED = new Set(["1.12.2", "1.16.5", "1.19.2", "1.20.1", "1.21.1"]);
const DEFAULT_FORGE_VERSION: Record<string, string> = {
  "1.12.2": "14.23.5.2860",
  "1.16.5": "36.2.42",
  "1.19.2": "43.4.16",
  "1.20.1": "47.4.10",
  "1.21.1": "52.0.57",
};

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

function contentArtworkKey(contentType: ContentType, filename: string) {
  return `${contentType}:${filename}`;
}

function curseForgeDownloadPage(item: CatalogContent, fileId: number, contentType: ContentType) {
  if (item.websiteUrl) {
    try {
      const url = new URL(item.websiteUrl);
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
  const category = contentType === "mod" ? "mc-mods" : contentType === "shader" ? "shaders" : "texture-packs";
  return `https://www.curseforge.com/minecraft/${category}/${encodeURIComponent(item.slug)}/download/${fileId}`;
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
      context.imageSmoothingEnabled = false;
      context.clearRect(0, 0, canvas.width, canvas.height);
      context.drawImage(image, 8, 8, 8, 8, 0, 0, canvas.width, canvas.height);
      context.drawImage(image, 40, 8, 8, 8, 0, 0, canvas.width, canvas.height);
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

function requiredJavaMajor(minecraftVersion: string) {
  if (minecraftVersion === "1.12.2" || minecraftVersion === "1.16.5") return 8;
  if (minecraftVersion === "1.21.1") return 21;
  return 17;
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

function preferredPackVersion(pack: ModrinthPack, fallback: string) {
  return pack.versions.includes(fallback) ? fallback : pack.versions[0] ?? fallback;
}

async function playAuroraSpeech(
  text: string,
  onCaption?: (caption: string) => void,
) {
  const speech = await invoke<SpeechResult>("synthesize_speech", { text });
  if (speech.mimeType !== "audio/mpeg" || !speech.audioBase64) {
    throw new Error("O Edge TTS retornou um áudio incompatível.");
  }
  let binary: string;
  try {
    binary = window.atob(speech.audioBase64);
  } catch {
    throw new Error("O áudio do Edge TTS chegou corrompido.");
  }
  if (binary.length < 128) {
    throw new Error("O Edge TTS retornou um áudio vazio.");
  }
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
  const objectUrl = URL.createObjectURL(new Blob([bytes], { type: "audio/mpeg" }));
  const audio = new Audio();
  audio.preload = "auto";
  audio.src = objectUrl;
  let released = false;
  const release = () => {
    if (released) return;
    released = true;
    URL.revokeObjectURL(objectUrl);
  };
  audio.addEventListener("ended", release, { once: true });
  audio.addEventListener("error", release, { once: true });
  audio.addEventListener("pause", release, { once: true });
  if (onCaption) {
    audio.addEventListener("timeupdate", () => {
      const elapsed = audio.currentTime * 1_000;
      const boundary = speech.boundaries.find(({ offsetMs, durationMs }) =>
        elapsed >= offsetMs && elapsed <= offsetMs + Math.max(durationMs, 800));
      onCaption(boundary?.text ?? "");
    });
    audio.addEventListener("ended", () => onCaption(""));
  }
  await new Promise<void>((resolve, reject) => {
    const ready = () => {
      cleanup();
      resolve();
    };
    const failed = () => {
      cleanup();
      const mediaError = audio.error;
      reject(new Error(mediaError?.message || `O WebView2 não reconheceu o áudio do Edge TTS (código ${mediaError?.code ?? 0}).`));
    };
    const cleanup = () => {
      audio.removeEventListener("canplay", ready);
      audio.removeEventListener("error", failed);
    };
    audio.addEventListener("canplay", ready, { once: true });
    audio.addEventListener("error", failed, { once: true });
    audio.load();
  }).catch((error) => {
    release();
    throw error;
  });
  await audio.play().catch((error) => {
    release();
    throw new Error(`Não foi possível iniciar a voz do Aurora: ${error instanceof Error ? error.message : String(error)}`);
  });
  return audio;
}

function AssistantPanel({ services, username, onClose }: { services: FirebaseServices; username: string; onClose: () => void }) {
  const [draft, setDraft] = useState("");
  const [messages, setMessages] = useState<OverlayMessage[]>([]);
  const [notice, setNotice] = useState("Pronto para ajudar");
  const [busy, setBusy] = useState(false);
  const [listening, setListening] = useState(false);
  const [muted, setMuted] = useState(false);
  const recognitionRef = useRef<SpeechRecognitionLike | null>(null);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const messageEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    messageEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, busy]);
  useEffect(() => () => {
    recognitionRef.current?.stop();
    audioRef.current?.pause();
  }, []);

  async function speak(text: string) {
    if (muted) return;
    audioRef.current?.pause();
    try {
      audioRef.current = await playAuroraSpeech(text);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "Não foi possível reproduzir a voz do Aurora");
    }
  }
  function toggleListening() {
    if (muted) {
      setNotice("Ative o áudio para usar o microfone");
      return;
    }
    if (listening) {
      recognitionRef.current?.stop();
      setListening(false);
      setNotice("Microfone pausado");
      return;
    }
    const SpeechRecognition = (window as SpeechWindow).SpeechRecognition ?? (window as SpeechWindow).webkitSpeechRecognition;
    if (!SpeechRecognition) {
      setNotice("O microfone por voz não está disponível neste computador");
      return;
    }
    const recognition = new SpeechRecognition();
    recognition.lang = "pt-BR";
    recognition.interimResults = false;
    recognition.onresult = (event) => {
      setDraft(event.results[0]?.[0]?.transcript ?? "");
      setNotice("Mensagem transcrita");
    };
    recognition.onerror = () => {
      setListening(false);
      setNotice("Não consegui compreender o áudio");
    };
    recognition.onend = () => setListening(false);
    recognitionRef.current = recognition;
    recognition.start();
    setListening(true);
    setNotice("Ouvindo você…");
  }
  function toggleMuted() {
    const next = !muted;
    setMuted(next);
    if (next) {
      recognitionRef.current?.stop();
      audioRef.current?.pause();
      setListening(false);
      setNotice("Áudio mutado");
    } else {
      setNotice("Áudio ativado");
    }
  }
  async function send(event: FormEvent) {
    event.preventDefault();
    if (!draft.trim() || busy) return;
    const question = draft.trim();
    setDraft("");
    setMessages((current) => [...current, { role: "user", text: question }]);
    setBusy(true);
    setNotice("Pensando…");
    try {
      const answer = await askAuroraAssistant(services, question);
      setMessages((current) => [...current, { role: "assistant", text: answer }]);
      setNotice("Resposta pronta");
      void speak(answer);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "O Assistente Aurora não respondeu");
    } finally {
      setBusy(false);
    }
  }
  return (
    <aside aria-label="Assistente Aurora" className="assistant-panel">
      <header className="assistant-heading">
        <div className="assistant-identity"><span className="assistant-avatar">✦</span><span><strong>Aurora</strong><small>{busy ? "Pensando…" : listening ? "Ouvindo…" : "Online"}</small></span></div>
        <div className="assistant-header-actions">
          <button aria-label={muted ? "Ativar áudio" : "Mutar áudio"} className={muted ? "assistant-icon muted" : "assistant-icon"} onClick={toggleMuted} title={muted ? "Ativar áudio" : "Mutar áudio"} type="button">{muted ? "🔇" : "🔊"}</button>
          <button aria-label="Fechar Assistente" className="assistant-icon" onClick={onClose} type="button">×</button>
        </div>
      </header>
      <div className="assistant-messages">
        {messages.length === 0 && (
          <div className="assistant-welcome">
            <span className="assistant-welcome-mark">✦</span>
            <h2>Olá, {username}.</h2>
            <p>Posso ajudar com Minecraft, mods, desempenho ou erros da sua instância.</p>
            <div className="assistant-suggestions">
              <button onClick={() => setDraft("Como melhorar o FPS da minha instância?")} type="button">Melhorar meu FPS</button>
              <button onClick={() => setDraft("Analise um erro de inicialização para mim")} type="button">Entender um erro</button>
            </div>
          </div>
        )}
        {messages.map((message, index) => (
          <div className={`assistant-message ${message.role}`} key={`${message.role}-${index}`}>
            {message.role === "assistant" && <span className="message-avatar">✦</span>}
            <p>{message.text}</p>
          </div>
        ))}
        {busy && <div className="assistant-message assistant"><span className="message-avatar">✦</span><p className="typing-dots"><i /><i /><i /></p></div>}
        <div ref={messageEndRef} />
      </div>
      {listening && <div className="mic-live" aria-live="polite"><span className="mic-waves" aria-hidden="true">{[0, 1, 2, 3, 4, 5, 6].map((bar) => <i key={bar} />)}</span><strong>Ouvindo…</strong></div>}
      <form className="assistant-composer" onSubmit={send}>
        <button aria-label={listening ? "Parar microfone" : "Usar microfone"} className={listening ? "mic-button listening" : "mic-button"} disabled={muted} onClick={toggleListening} title="Microfone" type="button">●</button>
        <textarea aria-label="Mensagem para o Assistente" value={draft} onChange={(event) => setDraft(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); event.currentTarget.form?.requestSubmit(); } }} placeholder="Mensagem para o Aurora…" maxLength={2_000} rows={1} />
        <button aria-label="Enviar mensagem" className="send-button" disabled={busy || !draft.trim()} type="submit">↑</button>
      </form>
      <footer className="assistant-footer"><span>{notice}</span><span>Enter para enviar · Shift + Enter para quebrar linha</span></footer>
    </aside>
  );
}

function AuthScreen({
  services,
  onAuthenticated,
}: {
  services: FirebaseServices;
  onAuthenticated: (profile: AuroraUserProfile) => void;
}) {
  const [mode, setMode] = useState<AuthMode>("login");
  const [nickname, setNickname] = useState("");
  const [password, setPassword] = useState("");
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!MINECRAFT_NICK_PATTERN.test(nickname)) {
      setMessage("Use um nick Minecraft válido: 3–16 letras, números ou _. Sem espaços, acentos ou símbolos.");
      return;
    }
    setBusy(true);
    setMessage("");
    try {
      const credential =
        mode === "login"
          ? await loginAuroraUser(services, nickname, password)
          : await registerAuroraUser(services, nickname, password);
      onAuthenticated(await loadAuroraProfile(services, credential.user));
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
          {mode === "login" ? "Bem-vindo de volta." : "Crie sua constelação."}
        </h1>
        <p className="lede">
          Entre somente com seu nick e senha. Seu perfil Aurora acompanha suas
          instâncias.
        </p>
        <div className="auth-tabs">
          <button
            className={mode === "login" ? "selected" : "secondary"}
            onClick={() => setMode("login")}
            type="button"
          >
            Entrar
          </button>
          <button
            className={mode === "register" ? "selected" : "secondary"}
            onClick={() => setMode("register")}
            type="button"
          >
            Criar conta
          </button>
        </div>
        <form onSubmit={submit} className="auth-form">
          <label>
            Nick
            <input
              value={nickname}
              onChange={(event) => {
                const next = sanitizeMinecraftNickname(event.target.value);
                setNickname(next);
                if (next !== event.target.value) {
                  setMessage("O nick aceita somente letras, números e _. Espaços e símbolos foram removidos.");
                }
              }}
              placeholder="Aurora_Player"
              autoComplete="username"
              minLength={3}
              maxLength={16}
              pattern="[A-Za-z0-9_]{3,16}"
              title="3–16 letras, números ou _. Sem espaços e símbolos."
              required
            />
            <small className="field-hint">Esse será exatamente o nome usado dentro do Minecraft.</small>
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
          {message && <p className="form-error">{message}</p>}
          <button disabled={busy} type="submit">
            {busy
              ? "Aguarde…"
              : mode === "login"
                ? "Entrar no Aurora"
                : "Criar conta Aurora"}
          </button>
        </form>
      </section>
    </main>
  );
}

function SkinPreviewCanvas({ skinModel, skinUrl }: { skinModel: "classic" | "slim"; skinUrl: string }) {
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
    void viewer.loadSkin(skinUrl, { model: skinModel === "classic" ? "default" : "slim" })
      .catch(() => setPreviewError(true));
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

function LauncherApp() {
  const [services, setServices] = useState<FirebaseServices>(() =>
    createFirebaseServices(bundledFirebaseConfig),
  );
  const [profile, setProfile] = useState<AuroraUserProfile>();
  const [status, setStatus] = useState<EngineStatus>();
  const [instances, setInstances] = useState<Instance[]>([]);
  const [instanceName, setInstanceName] = useState("");
  const [targetInstance, setTargetInstance] = useState("");
  const [minecraftVersion, setMinecraftVersion] = useState("1.20.1");
  const [loaderChoice, setLoaderChoice] = useState<LoaderChoice>("fabric");
  const [installedVersionId, setInstalledVersionId] = useState("");
  const [installedMinecraftVersion, setInstalledMinecraftVersion] = useState("");
  const [modpackQuery, setModpackQuery] = useState("");
  const [modrinthPacks, setModrinthPacks] = useState<ModrinthPack[]>([]);
  const [modrinthTotalHits, setModrinthTotalHits] = useState(0);
  const [modrinthPage, setModrinthPage] = useState(0);
  const [modrinthPageSize, setModrinthPageSize] = useState(10);
  const [selectedModrinthPack, setSelectedModrinthPack] = useState<ModrinthPack>();
  const [packVersions, setPackVersions] = useState<Record<string, string>>({});
  const [editingInstance, setEditingInstance] = useState("");
  const [contentType, setContentType] = useState<ContentType>("mod");
  const [editorView, setEditorView] = useState<EditorView>("installed");
  const [installedContentFilter, setInstalledContentFilter] = useState("");
  const [contentArtwork, setContentArtwork] = useState<Record<string, ContentArtworkEntry>>({});
  const [contentQuery, setContentQuery] = useState("");
  const [contentResults, setContentResults] = useState<CatalogContent[]>([]);
  const [contentSource, setContentSource] = useState<CatalogSource>("modrinth");
  const [contentSort, setContentSort] = useState<ContentSort>("popular");
  const [contentVersionFilter, setContentVersionFilter] = useState("");
  const [contentLoaderFilter, setContentLoaderFilter] = useState<"" | LoaderChoice>("");
  const [projectDetail, setProjectDetail] = useState<ProjectDetail>();
  const [detailLoading, setDetailLoading] = useState(false);
  const [instanceContent, setInstanceContent] = useState<InstanceContent>({ mods: [], shaderpacks: [], resourcepacks: [] });
  const [selectedContentNames, setSelectedContentNames] = useState<string[]>([]);
  const [javaPath, setJavaPath] = useState("");
  const [javaRuntimes, setJavaRuntimes] = useState<JavaRuntime[]>([]);
  const [javaInstallOpen, setJavaInstallOpen] = useState(false);
  const [installingJavaMajor, setInstallingJavaMajor] = useState<number>();
  const [notice, setNotice] = useState("Preparando o núcleo Aurora…");
  const [noticeHovered, setNoticeHovered] = useState(false);
  const [downloadProgress, setDownloadProgress] = useState<DownloadProgress>();
  const [telemetry, setTelemetry] = useState<IpcEvent>();
  const [skinUrl, setSkinUrl] = useState("");
  const [capeUrl, setCapeUrl] = useState("");
  const [skinModel, setSkinModel] = useState<"classic" | "slim">("classic");
  const [favoriteName, setFavoriteName] = useState("");
  const [favoriteSearch, setFavoriteSearch] = useState("");
  const [skinAddMode, setSkinAddMode] = useState<SkinAddMode>("file");
  const [skinUsername, setSkinUsername] = useState("");
  const [localSkins, setLocalSkins] = useState<LocalSkinView[]>([]);
  const [selectedLocalSkinId, setSelectedLocalSkinId] = useState("");
  const [localSkinPath, setLocalSkinPath] = useState("");
  const localPreviewUrls = useRef<string[]>([]);
  const [instanceFilter, setInstanceFilter] = useState("");
  const [activeSection, setActiveSection] = useState<LauncherSection>("instances");
  const [catalogSource, setCatalogSource] = useState<CatalogSource>("modrinth");
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [assistantOpen, setAssistantOpen] = useState(false);
  const [instanceLog, setInstanceLog] = useState<{ id: string; log: InstanceLog }>();
  const [runningInstances, setRunningInstances] = useState<RunningInstance[]>([]);
  const [renameDraft, setRenameDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const avatarMigrationRef = useRef("");
  const localAvatarMigrationRef = useRef("");
  const activeInstanceRef = useRef({ editingInstance, targetInstance });
  useEffect(() => {
    activeInstanceRef.current = { editingInstance, targetInstance };
  }, [editingInstance, targetInstance]);
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
  async function refreshLocalSkinLibrary(ownerId: string) {
    localPreviewUrls.current.forEach((url) => URL.revokeObjectURL(url));
    localPreviewUrls.current = [];
    const records = await listLocalSkins(ownerId);
    const views = records.map((skin) => {
      const previewUrl = skin.image ? URL.createObjectURL(skin.image) : skin.sourceUrl ?? "";
      if (skin.image) localPreviewUrls.current.push(previewUrl);
      return { ...skin, previewUrl };
    });
    setLocalSkins(views);
    return views;
  }
  useEffect(() => {
    invoke<FirebasePublicConfig | null>("load_firebase_config")
      .then((config) => {
        if (config) setServices(createFirebaseServices(config));
      })
      .catch((error: unknown) => setNotice(String(error)));
  }, []);
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
  }, []);
  useEffect(() => {
    let disposed = false;
    let stopListening: (() => void) | undefined;
    void listen<DownloadProgress>("aurora-download-progress", (event) => {
      if (disposed) return;
      setDownloadProgress(event.payload);
      if ((event.payload.totalPercent ?? event.payload.percent) >= 100) {
        window.setTimeout(() => setDownloadProgress(undefined), 1800);
      }
    }).then((unlisten) => {
      if (disposed) unlisten();
      else stopListening = unlisten;
    });
    return () => { disposed = true; stopListening?.(); };
  }, []);
  useEffect(() => {
    let disposed = false;
    let stopListening: (() => void) | undefined;
    void listen<ManualDownloadResult>("aurora-manual-download", (event) => {
      if (disposed) return;
      const result = event.payload;
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
      if (activeInstanceRef.current.editingInstance === result.instanceId) {
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
    return () => { disposed = true; stopListening?.(); };
  }, []);
  useEffect(() => {
    if (!notice || noticeHovered || busy) return;
    const timer = window.setTimeout(() => setNotice(""), 6500);
    return () => window.clearTimeout(timer);
  }, [notice, noticeHovered, busy]);
  useEffect(() => {
    register("Ctrl+Alt+/", (event) => {
      if (event.state === "Pressed") {
        void invoke<boolean>("toggle_ipc_assistant")
          .then((deliveredToGame) => {
            if (!deliveredToGame) setAssistantOpen((current) => !current);
          })
          .catch(() => setAssistantOpen((current) => !current));
      }
    }).catch(() => setNotice("Atalho AltGr + / indisponível neste computador."));
  }, []);
  useEffect(() => {
    if (!services) return;
    return observeAuroraSession(services, (user) => {
      if (!user) {
        setProfile(undefined);
        return;
      }
      loadAuroraProfile(services, user)
        .then(setProfile)
        .catch((error: unknown) => setNotice(String(error)));
    });
  }, [services]);
  useEffect(() => {
    if (!profile || !services) return;
    void syncAuroraPublicProfile(services, profile).catch(() => undefined);
    refresh()
      .then(() => {
        setNotice("Núcleo pronto. Crie uma instância para começar.");
      })
      .catch((error: unknown) =>
        setNotice(`Não foi possível iniciar o Aurora: ${String(error)}`),
      );
  }, [profile]);
  useEffect(() => {
    if (!profile) return;
    if (!selectedLocalSkinId) setSkinUrl(profile.skinUrl ?? "");
    setCapeUrl(profile.capeUrl ?? "");
    setSkinModel(profile.skinModel);
    setLocalSkinPath(window.localStorage.getItem(`aurora.localSkinPath.${profile.uid}`) ?? "");
  }, [profile]);
  useEffect(() => {
    if (!profile?.skinUrl || profile.avatarUrl === profile.skinUrl || !services) return;
    const migrationKey = `${profile.uid}:${profile.skinUrl}`;
    if (avatarMigrationRef.current === migrationKey) return;
    avatarMigrationRef.current = migrationKey;
    void (async () => {
      try {
        const next = await saveAuroraAppearance(services, profile, {
          avatarUrl: profile.skinUrl,
          skinUrl: profile.skinUrl,
          capeUrl: profile.capeUrl,
          skinModel: profile.skinModel,
        });
        setProfile(next);
      } catch {
        // O perfil continua utilizável; uma nova tentativa ocorre quando a skin for equipada novamente.
      }
    })();
  }, [profile?.uid, profile?.skinUrl, profile?.avatarUrl, services]);
  useEffect(() => {
    if (!profile || profile.avatarUrl || profile.skinUrl || !localSkinPath || !services) return;
    const migrationKey = `${profile.uid}:${localSkinPath}`;
    if (localAvatarMigrationRef.current === migrationKey) return;
    localAvatarMigrationRef.current = migrationKey;
    void (async () => {
      try {
        const loaded = await invoke<AppearanceImage>("load_local_appearance", { userId: profile.uid, kind: "skin" });
        const blob = await (await fetch(loaded.dataBase64)).blob();
        const skinUrl = await uploadAppearanceImage("skin", new File([blob], "skin.png", { type: "image/png" }));
        const next = await saveAuroraAppearance(services, profile, {
          avatarUrl: skinUrl,
          skinUrl,
          capeUrl: profile.capeUrl,
          skinModel: profile.skinModel,
        });
        setProfile(next);
      } catch {
        // A skin continua equipada localmente; o usuário pode sincronizá-la novamente no guarda-roupa.
      }
    })();
  }, [profile?.uid, profile?.avatarUrl, profile?.skinUrl, localSkinPath, services]);
  useEffect(() => {
    if (!profile || !services) return;
    let cancelled = false;
    void (async () => {
      const stored = await listLocalSkins(profile.uid);
      const knownUrls = new Set(stored.map((skin) => skin.sourceUrl).filter(Boolean));
      for (const favorite of profile.skinFavorites) {
        if (!knownUrls.has(favorite.skinUrl)) {
          await saveLocalSkin({
            id: favorite.id,
            ownerId: profile.uid,
            name: favorite.name,
            skinModel: favorite.skinModel,
            sourceUrl: favorite.skinUrl,
          });
          knownUrls.add(favorite.skinUrl);
        }
      }
      if (profile.skinUrl && !knownUrls.has(profile.skinUrl)) {
        await saveLocalSkin({ ownerId: profile.uid, name: "Skin equipada", skinModel: profile.skinModel, sourceUrl: profile.skinUrl });
      }
      if (profile.skinFavorites.length > 0) {
        const next = await clearRemoteSkinLibrary(services, profile);
        if (!cancelled) setProfile(next);
      }
      if (!cancelled) {
        const views = await refreshLocalSkinLibrary(profile.uid);
        const equipped = views.find((skin) => skin.sourceUrl === profile.skinUrl);
        if (equipped) setSelectedLocalSkinId(equipped.id);
      }
    })().catch((error: unknown) => setNotice(`Não foi possível carregar a biblioteca local: ${String(error)}`));
    return () => { cancelled = true; };
  }, [profile?.uid, services]);
  useEffect(() => () => localPreviewUrls.current.forEach((url) => URL.revokeObjectURL(url)), []);
  useEffect(() => {
    async function answerInGame(processId: number, event: Extract<IpcEvent, { kind: "AssistantRequest" }>) {
      if (!services) {
        await invoke("send_ipc_assistant_response", {
          processId,
          requestId: event.requestId,
          text: null,
          error: "Entre na sua conta Aurora no launcher para usar o Assistente.",
        }).catch(() => undefined);
        return;
      }
      try {
        const answer = await askAuroraAssistant(services, event.message, {
          mode: "inGame",
          screenshotBase64: event.screenshotBase64,
        });
        await invoke("send_ipc_assistant_response", {
          processId,
          requestId: event.requestId,
          text: answer,
          error: null,
        });
        let lastCaption = "";
        void playAuroraSpeech(answer, (caption) => {
          if (caption === lastCaption) return;
          lastCaption = caption;
          void invoke("send_ipc_caption", { processId, requestId: event.requestId, caption }).catch(() => undefined);
        }).catch(() => undefined);
      } catch (error) {
        await invoke("send_ipc_assistant_response", {
          processId,
          requestId: event.requestId,
          text: null,
          error: error instanceof Error ? error.message : "O Assistente Aurora não respondeu.",
        }).catch(() => undefined);
      }
    }
    function listenInGame(processId: number, event: Extract<IpcEvent, { kind: "AssistantListenRequested" }>) {
      const SpeechRecognition = (window as SpeechWindow).SpeechRecognition ?? (window as SpeechWindow).webkitSpeechRecognition;
      if (!SpeechRecognition) {
        void invoke("send_ipc_transcript", {
          processId,
          requestId: event.requestId,
          text: null,
          error: "O reconhecimento de voz não está disponível neste computador.",
        });
        return;
      }
      const recognition = new SpeechRecognition();
      let completed = false;
      const respond = (text: string | null, error: string | null) => {
        if (completed) return;
        completed = true;
        void invoke("send_ipc_transcript", { processId, requestId: event.requestId, text, error }).catch(() => undefined);
      };
      recognition.lang = "pt-BR";
      recognition.interimResults = false;
      recognition.onresult = (result) => {
        const transcript = result.results[0]?.[0]?.transcript?.trim() ?? "";
        respond(transcript || null, transcript ? null : "Não consegui compreender o áudio.");
      };
      recognition.onerror = () => respond(null, "Não consegui acessar ou compreender o microfone.");
      recognition.onend = () => respond(null, "Nenhuma fala foi detectada.");
      recognition.start();
    }
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
          for (const { processId, event } of sessionEvents) {
            if (event.kind === "AssistantRequest") void answerInGame(processId, event);
            if (event.kind === "AssistantListenRequested") listenInGame(processId, event);
          }
          const last = sessionEvents.at(-1)?.event;
          if (last) setTelemetry(last);
        })
        .catch(() => undefined);
    }, 1_000);
    return () => window.clearInterval(timer);
  }, [services]);
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
      await refresh();
      setNotice(`${loadingLabel} pronto em “${targetInstance}”: ${result.libraryCount} bibliotecas e ${result.assetCount} assets verificados.`);
    } catch (error) {
      setDownloadProgress(undefined);
      setNotice(`Não foi possível instalar Minecraft: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }
  async function searchModrinth(event: FormEvent) {
    event.preventDefault();
    await loadModrinthPage(0);
  }
  async function loadModrinthPage(page: number, requestedPageSize = modrinthPageSize) {
    setBusy(true);
    try {
      const result = await invoke<ModrinthSearchPage>("search_modrinth_modpacks", {
        query: modpackQuery.trim(),
        offset: page * requestedPageSize,
        limit: requestedPageSize,
      });
      setModrinthPacks(result.items);
      setModrinthTotalHits(result.totalHits);
      setModrinthPage(page);
      setSelectedModrinthPack(result.items[0]);
      setPackVersions((current) => {
        const next = { ...current };
        for (const pack of result.items) {
          if (!next[pack.projectId]) next[pack.projectId] = preferredPackVersion(pack, minecraftVersion);
        }
        return next;
      });
      setNotice(`${result.totalHits.toLocaleString("pt-BR")} modpack(s) encontrado(s) no Modrinth.`);
    } catch (error) {
      setNotice(`Não foi possível consultar Modrinth: ${String(error)}`);
    } finally { setBusy(false); }
  }
  async function installModrinth(pack: ModrinthPack) {
    if (!targetInstance) {
      setNotice("Escolha uma instância antes de instalar o modpack.");
      return;
    }
    setBusy(true);
    setNotice(`Preparando “${pack.title}”. O Aurora vai baixar e verificar todos os arquivos do modpack…`);
    try {
      const result = await invoke<ModpackInstallSummary>("install_modrinth_modpack", {
        id: targetInstance,
        projectId: pack.projectId,
        minecraftVersion: packVersions[pack.projectId] ?? preferredPackVersion(pack, minecraftVersion),
      });
      await invoke<Instance>("set_instance_presentation", {
        id: targetInstance,
        displayName: pack.title,
        iconUrl: safeHttpsUrl(pack.iconUrl) || null,
      }).catch(() => undefined);
      setInstalledVersionId(result.minecraft.versionId);
      setInstalledMinecraftVersion(result.minecraftVersion);
      setNotice(
        `“${result.name}” instalado em “${targetInstance}” (${result.minecraftVersion}, ${result.loader}): ${result.downloadedFiles} arquivos verificados e ${result.overrideFiles} arquivos extras aplicados.`,
      );
      await refresh();
    } catch (error) {
      setDownloadProgress(undefined);
      setNotice(`Não foi possível instalar “${pack.title}”: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }
  async function createAndInstallModrinth(pack: ModrinthPack) {
    const selectedMinecraftVersion = packVersions[pack.projectId] ?? preferredPackVersion(pack, minecraftVersion);
    const preferred = safeInstanceId(`${pack.slug || pack.title}-${selectedMinecraftVersion}`);
    let id = preferred;
    let counter = 2;
    while (instances.some((instance) => instance.id === id)) {
      id = `${preferred.slice(0, 58)}-${counter++}`;
    }
    setBusy(true);
    setNotice(`Criando a instância de “${pack.title}” e baixando os arquivos verificados…`);
    try {
      const instance = await invoke<Instance>("create_instance", { id });
      const result = await invoke<ModpackInstallSummary>("install_modrinth_modpack", {
        id: instance.id,
        projectId: pack.projectId,
        minecraftVersion: selectedMinecraftVersion,
      });
      await invoke<Instance>("set_instance_presentation", {
        id: instance.id,
        displayName: pack.title,
        iconUrl: safeHttpsUrl(pack.iconUrl) || null,
      }).catch(() => undefined);
      setTargetInstance(instance.id);
      setInstalledVersionId(result.minecraft.versionId);
      setInstalledMinecraftVersion(result.minecraftVersion);
      setMinecraftVersion(result.minecraftVersion);
      await refresh();
      setNotice(`“${result.name}” está pronto para iniciar: ${result.minecraftVersion} com ${result.loader}.`);
    } catch (error) {
      setDownloadProgress(undefined);
      setNotice(`Não foi possível preparar “${pack.title}”: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }
  async function openEditor(id: string) {
    setActiveSection("instances");
    setEditingInstance(id);
    setRenameDraft(id);
    setEditorView("installed");
    setSelectedContentNames([]);
    setInstalledContentFilter("");
    setContentQuery("");
    setContentResults([]);
    let savedArtwork: Record<string, ContentArtworkEntry> = {};
    try {
      const savedArtworkText = window.localStorage.getItem(`aurora.contentArtwork.${id}`);
      if (savedArtworkText) savedArtwork = JSON.parse(savedArtworkText) as Record<string, ContentArtworkEntry>;
    } catch {
      savedArtwork = {};
    }
    setContentArtwork(savedArtwork);
    await selectInstance(id);
    try {
      const current = await invoke<InstanceContent>("list_instance_content", { id });
      setInstanceContent(current);
      const resolvedGroups = await Promise.all((["mod", "shader", "resourcepack"] as ContentType[]).map(async (type) => {
        try {
          return { type, items: await invoke<ResolvedContentArtwork[]>("resolve_content_artwork", { id, contentType: type }) };
        } catch {
          return { type, items: [] as ResolvedContentArtwork[] };
        }
      }));
      const mergedArtwork = { ...savedArtwork };
      for (const group of resolvedGroups) {
        for (const item of group.items) {
          mergedArtwork[contentArtworkKey(group.type, item.filename)] = { projectId: item.projectId, title: item.title, iconUrl: item.iconUrl };
        }
      }
      setContentArtwork(mergedArtwork);
      window.localStorage.setItem(`aurora.contentArtwork.${id}`, JSON.stringify(mergedArtwork));
    } catch (error) {
      setNotice(`Não foi possível abrir o conteúdo da instância: ${String(error)}`);
    }
  }
  async function openInstanceLog(id: string) {
    setActiveSection("instances");
    await selectInstance(id);
    setBusy(true);
    try {
      const log = await invoke<InstanceLog>("read_instance_log", { id });
      setInstanceLog({ id, log });
      setNotice(log.lines.length ? `Exibindo ${log.filename}.` : "Ainda não há logs nesta instância.");
    } catch (error) {
      setNotice(`Não foi possível abrir o log: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }
  async function setContentEnabled(file: InstanceContentFile, enabled: boolean) {
    const id = editingInstance || targetInstance;
    if (!id) return;
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
    setContentType(nextType);
    setSelectedContentNames([]);
    setInstalledContentFilter("");
    setContentQuery("");
    setContentResults([]);
    if (editorView === "discover") void loadContentCatalog({ type: nextType, query: "", sort: "popular" });
  }
  async function selectInstance(id: string) {
    setTargetInstance(id);
    try {
      const saved = await invoke<InstanceLaunchProfile>("read_instance_launch_profile", { id });
      if (saved.versionId) setInstalledVersionId(saved.versionId);
      if (saved.minecraftVersion) {
        setInstalledMinecraftVersion(saved.minecraftVersion);
        setMinecraftVersion(saved.minecraftVersion);
      }
    } catch (error) {
      setNotice(`Não foi possível ler a configuração de “${id}”: ${String(error)}`);
    }
  }
  function instanceLoader(): LoaderChoice | undefined {
    const versionId = installedVersionId.toLowerCase();
    if (versionId.includes("fabric")) return "fabric";
    if (versionId.includes("forge")) return "forge";
    return undefined;
  }
  async function loadContentCatalog(options: {
    source?: CatalogSource;
    type?: ContentType;
    query?: string;
    sort?: ContentSort;
    version?: string;
    loader?: "" | LoaderChoice;
  } = {}) {
    const source = options.source ?? contentSource;
    const type = options.type ?? contentType;
    const query = options.query ?? contentQuery;
    const sort = options.sort ?? contentSort;
    const version = options.version ?? (contentVersionFilter || installedMinecraftVersion || minecraftVersion);
    const loader = options.loader ?? (contentLoaderFilter || instanceLoader() || "");
    setBusy(true);
    try {
      let results: CatalogContent[];
      if (source === "modrinth") {
        const found = await invoke<ModrinthContent[]>("search_modrinth_content", {
          query: query.trim(),
          contentType: type,
          minecraftVersion: version || null,
          loader: type === "mod" ? loader || null : null,
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
      setNotice(`Não foi possível carregar o catálogo: ${String(error)}`);
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
    void loadContentCatalog({ query: "", sort: "popular", version, loader });
  }
  async function openContentDetail(item: CatalogContent) {
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
        setProjectDetail((current) => current?.content?.projectId === item.projectId ? {
          ...current,
          body: normalizeProjectMarkdown(project.body ?? item.description),
          iconUrl: project.icon_url ?? item.iconUrl,
          gallery: (project.gallery ?? []).sort((left, right) => Number(Boolean(right.featured)) - Number(Boolean(left.featured))).map((image) => image.url ?? "").filter(Boolean),
          websiteUrl: `https://modrinth.com/project/${item.slug || item.projectId}`,
        } : current);
      } else if (services && item.curseForgeId) {
        const description = await requestCurseForgeCatalog<{ data?: string }>(services, { action: "description", modId: item.curseForgeId });
        setProjectDetail((current) => current?.content?.projectId === item.projectId ? { ...current, body: description.data ?? item.description } : current);
      }
    } catch (error) {
      setNotice(`A descrição completa não pôde ser carregada: ${String(error)}`);
    } finally {
      setDetailLoading(false);
    }
  }
  async function openModpackDetail(pack: ModrinthPack) {
    setDetailLoading(true);
    setProjectDetail({ source: "modrinth", kind: "modpack", title: pack.title, author: pack.author, iconUrl: pack.iconUrl, summary: pack.description, body: normalizeProjectMarkdown(pack.description), bodyFormat: "markdown", gallery: [], websiteUrl: `https://modrinth.com/modpack/${pack.slug || pack.projectId}`, pack });
    try {
      const response = await fetch(`https://api.modrinth.com/v2/project/${encodeURIComponent(pack.projectId)}`);
      if (!response.ok) throw new Error("A página do modpack não está disponível.");
      const project = await response.json() as { body?: string; icon_url?: string; gallery?: Array<{ url?: string; featured?: boolean }> };
      setProjectDetail((current) => current?.pack?.projectId === pack.projectId ? { ...current, body: normalizeProjectMarkdown(project.body ?? pack.description), iconUrl: project.icon_url ?? pack.iconUrl, gallery: (project.gallery ?? []).sort((left, right) => Number(Boolean(right.featured)) - Number(Boolean(left.featured))).map((image) => image.url ?? "").filter(Boolean) } : current);
    } catch (error) {
      setNotice(`A descrição completa não pôde ser carregada: ${String(error)}`);
    } finally {
      setDetailLoading(false);
    }
  }
  function rememberContentArtwork(id: string, filename: string, item: CatalogContent) {
    const nextArtwork = {
      ...contentArtwork,
      [contentArtworkKey(contentType, filename)]: { iconUrl: item.iconUrl, projectId: item.projectId, title: item.title },
    };
    setContentArtwork(nextArtwork);
    window.localStorage.setItem(`aurora.contentArtwork.${id}`, JSON.stringify(nextArtwork));
  }
  async function installContent(item: CatalogContent) {
    const id = editingInstance || targetInstance;
    const gameVersion = installedMinecraftVersion || minecraftVersion;
    if (!id) {
      setNotice("Abra uma instância para instalar conteúdo.");
      return;
    }
    const loader = installedVersionId.toLowerCase().includes("fabric")
      ? "fabric"
      : installedVersionId.toLowerCase().includes("forge")
        ? "forge"
        : undefined;
    if (contentType === "mod" && !loader) {
      setNotice("Instale Fabric ou Forge nessa instância antes de adicionar mods.");
      return;
    }
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
        const file = files.data?.[0];
        if (!file) throw new Error(`Não há arquivo compatível com Minecraft ${gameVersion}${loader ? ` e ${loader}` : ""}.`);
        let downloadUrl = file.downloadUrl;
        if (!downloadUrl) {
          const resolved = await requestCurseForgeCatalog<{ data?: string }>(services, { action: "download", modId: item.curseForgeId, fileId: file.id });
          downloadUrl = resolved.data;
        }
        const sha1 = file.hashes?.find((hash) => hash.algo === 1)?.value;
        if (!downloadUrl) {
          if (!sha1) throw new Error("O autor exige download pela página e o CurseForge não forneceu o SHA-1 necessário para uma instalação segura.");
          await invoke("open_manual_content_download", {
            id,
            pageUrl: curseForgeDownloadPage(item, file.id, contentType),
            filename: file.fileName,
            contentType,
            sha1,
          });
          rememberContentArtwork(id, file.fileName, item);
          setProjectDetail(undefined);
          setNotice(`A página oficial foi aberta dentro do Aurora. Aguarde os 5 segundos; “${file.fileName}” será validado e colocado na instância automaticamente.`);
          return;
        }
        filename = await invoke<string>("install_remote_content", {
          id,
          url: downloadUrl,
          filename: file.fileName,
          contentType,
          sha1: sha1 ?? null,
        });
      }
      setInstanceContent(await invoke<InstanceContent>("list_instance_content", { id }));
      rememberContentArtwork(id, filename, item);
      setEditorView("installed");
      setProjectDetail(undefined);
      setNotice(`“${filename}” foi instalado em “${id}”.`);
    } catch (error) {
      setDownloadProgress(undefined);
      setNotice(`Não foi possível instalar “${item.title}”: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }
  async function launchMinecraft(instanceId = targetInstance) {
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
    try {
      const launchProfile = await invoke<InstanceLaunchProfile>("read_instance_launch_profile", { id: instanceId });
      if (!launchProfile.versionId || !launchProfile.minecraftVersion) {
        throw new Error("instale Fabric ou Forge nesta instância antes de iniciar");
      }
      const versionId = launchProfile.versionId;
      const gameVersion = launchProfile.minecraftVersion;
      setTargetInstance(instanceId);
      setInstalledVersionId(versionId);
      setInstalledMinecraftVersion(gameVersion);
      setMinecraftVersion(gameVersion);
      let executable = javaPath.trim();
      const requiredJava = requiredJavaMajor(gameVersion);
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
          ?? await invoke<JavaRuntime>("ensure_java", { minecraftVersion: gameVersion });
        executable = selected.executable;
        setJavaRuntimes(runtimes.some((runtime) => runtime.executable === selected.executable) ? runtimes : [...runtimes, selected]);
        setJavaPath(executable);
        window.localStorage.setItem("aurora.javaExecutable", executable);
      }
      const result = await invoke<LaunchSummary>("launch_instance", {
        id: instanceId,
        versionId,
        minecraftVersion: gameVersion,
        javaExecutable: executable,
        nickname: profile?.username ?? "",
        skinUrl: profile?.skinUrl ?? null,
        skinFile: localSkinPath || null,
        capeUrl: profile?.capeUrl ?? null,
        skinModel: profile?.skinModel ?? null,
      });
      setRunningInstances((current) => [
        ...current.filter((running) => running.instanceId !== instanceId),
        { instanceId, processId: result.processId },
      ]);
      setNotice(
        `“${instanceId}” está rodando (processo ${result.processId}) com o perfil ${result.versionId}${result.coreInstalled ? " e Aurora Core" : ""}${result.companionInstalled ? " + Companion" : ""}.`,
      );
    } catch (error) {
      setNotice(`Não foi possível iniciar Minecraft: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }
  async function saveAppearance(event: FormEvent) {
    event.preventDefault();
    if (!profile || !services) return;
    setBusy(true);
    let equippedLocally = false;
    try {
      const selected = localSkins.find((skin) => skin.id === selectedLocalSkinId);
      let equippedSkinUrl = selected?.sourceUrl ?? skinUrl;
      let equippedAvatarUrl = equippedSkinUrl || profile.avatarUrl || "";
      let equippedLocalPath = "";
      let onlineWarning = "";
      if (selected?.image) {
        const file = new File([selected.image], `${selected.name || "skin"}.png`, { type: "image/png" });
        equippedLocalPath = await invoke<string>("save_local_appearance", {
          userId: profile.uid,
          kind: "skin",
          dataBase64: await fileAsDataUrl(file),
        });
        equippedLocally = true;
        setLocalSkinPath(equippedLocalPath);
        window.localStorage.setItem(`aurora.localSkinPath.${profile.uid}`, equippedLocalPath);
        setSkinUrl(selected.previewUrl);
        try {
          const onlineSkinUrl = await uploadAppearanceImage("skin", file);
          equippedSkinUrl = onlineSkinUrl;
          equippedAvatarUrl = onlineSkinUrl;
        } catch (error) {
          equippedSkinUrl = profile.skinUrl ?? "";
          equippedAvatarUrl = profile.avatarUrl ?? "";
          onlineWarning = error instanceof Error ? error.message : String(error);
        }
      } else if (equippedSkinUrl) {
        const loaded = await invoke<AppearanceImage>("load_appearance_url", { url: equippedSkinUrl, kind: "skin" });
        equippedSkinUrl = loaded.url;
        equippedAvatarUrl = loaded.url;
      }
      const equippedCapeUrl = capeUrl
        ? await invoke<string>("validate_appearance_url", { url: capeUrl, kind: "cape" })
        : "";
      const next = await saveAuroraAppearance(services, profile, {
        avatarUrl: equippedAvatarUrl || null,
        skinUrl: equippedSkinUrl || null,
        capeUrl: equippedCapeUrl || null,
        skinModel,
      });
      setProfile(next);
      if (equippedLocalPath) {
        setSkinUrl(selected?.previewUrl ?? skinUrl);
        setNotice(onlineWarning
          ? `Skin equipada localmente, mas a sincronização online falhou: ${onlineWarning}`
          : "Skin equipada localmente e sincronizada no Supabase.");
      } else {
        setLocalSkinPath("");
        window.localStorage.removeItem(`aurora.localSkinPath.${profile.uid}`);
        setSkinUrl(next.skinUrl ?? "");
        setNotice("Skin por URL equipada no perfil Aurora.");
      }
    } catch (error) {
      setNotice(equippedLocally
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
  async function uploadAppearance(kind: "skin" | "cape", file?: File) {
    if (!profile || !services || !file) return;
    setBusy(true);
    try {
      await validateAppearanceFile(kind, file);
      if (kind === "skin") {
        const local = await saveLocalSkin({
          ownerId: profile.uid,
          name: favoriteName.trim() || file.name.replace(/\.png$/i, "") || "Minha skin",
          skinModel,
          image: file,
        });
        const views = await refreshLocalSkinLibrary(profile.uid);
        const selected = views.find((skin) => skin.id === local.id);
        setSelectedLocalSkinId(local.id);
        setSkinUrl(selected?.previewUrl ?? "");
        setFavoriteName("");
        setNotice("Skin adicionada à biblioteca local. Clique em Equipar no jogo para aplicá-la.");
      } else {
        const url = await uploadAppearanceImage(kind, file);
        setCapeUrl(url);
        setNotice("Capa enviada. Clique em Equipar para aplicá-la.");
      }
    } catch (error) {
      setNotice(`Não foi possível adicionar a imagem: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }
  async function favoriteCurrentSkin() {
    if (!profile) return;
    if (selectedLocalSkinId) {
      setNotice("Essa skin já está na biblioteca local.");
      return;
    }
    setBusy(true);
    try {
      const validatedSkinUrl = await invoke<string>("validate_appearance_url", { url: skinUrl, kind: "skin" });
      const local = await saveLocalSkin({
        ownerId: profile.uid,
        name: favoriteName.trim() || "Minha skin",
        skinModel,
        sourceUrl: validatedSkinUrl,
      });
      await refreshLocalSkinLibrary(profile.uid);
      setSelectedLocalSkinId(local.id);
      setFavoriteName("");
      setNotice("Skin guardada somente neste computador.");
    } catch (error) {
      setNotice(`Não foi possível guardar a skin: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }
  function useFavoriteSkin(skin: LocalSkinView) {
    setSelectedLocalSkinId(skin.id);
    setSkinUrl(skin.previewUrl);
    setSkinModel(skin.skinModel);
    setFavoriteName(skin.name);
    setNotice(`Skin “${skin.name}” selecionada. Clique em Equipar no jogo para aplicá-la.`);
  }
  async function removeFavoriteSkin(id: string) {
    if (!profile) return;
    setBusy(true);
    try {
      await deleteLocalSkin(profile.uid, id);
      if (selectedLocalSkinId === id) {
        setSelectedLocalSkinId("");
        setSkinUrl(profile.skinUrl ?? "");
        setSkinModel(profile.skinModel);
      }
      await refreshLocalSkinLibrary(profile.uid);
      setNotice("Skin removida deste computador.");
    } catch (error) {
      setNotice(`Não foi possível remover a skin: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  }
  function selectSkinFromUsername() {
    const username = skinUsername.trim();
    if (!MINECRAFT_NICK_PATTERN.test(username)) {
      setNotice("Digite um nick Minecraft válido para procurar uma skin.");
      return;
    }
    setSkinUrl(`https://minotar.net/skin/${username}`);
    setSelectedLocalSkinId("");
    setFavoriteName(username);
    setSkinUsername("");
    setNotice(`Skin pública de ${username} selecionada. Guarde-a localmente ou clique em Equipar.`);
  }
  async function renameEditingInstance(event: FormEvent) {
    event.preventDefault();
    const oldId = editingInstance;
    const newId = renameDraft.trim();
    if (!oldId || !newId || oldId === newId) return;
    setBusy(true);
    try {
      const renamed = await invoke<Instance>("rename_instance", { id: oldId, newId });
      const oldArtworkKey = `aurora.contentArtwork.${oldId}`;
      const storedArtwork = window.localStorage.getItem(oldArtworkKey);
      if (storedArtwork) {
        window.localStorage.setItem(`aurora.contentArtwork.${renamed.id}`, storedArtwork);
        window.localStorage.removeItem(oldArtworkKey);
      }
      setEditingInstance(renamed.id);
      setRenameDraft(renamed.id);
      setTargetInstance(renamed.id);
      setInstanceLog((current) => current?.id === oldId ? { ...current, id: renamed.id } : current);
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
    if (!window.confirm(`Excluir a instância “${id}” e todos os arquivos dela? Esta ação não pode ser desfeita.`)) return;
    setBusy(true);
    try {
      await invoke("delete_instance", { id });
      if (targetInstance === id) {
        setTargetInstance("");
        setInstalledVersionId("");
        setInstalledMinecraftVersion("");
      }
      if (editingInstance === id) setEditingInstance("");
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
  async function installJava(major: 8 | 17 | 21) {
    const representativeVersion: Record<number, string> = { 8: "1.16.5", 17: "1.20.1", 21: "1.21.1" };
    setBusy(true);
    setInstallingJavaMajor(major);
    setNotice(`Baixando e verificando Java ${major}…`);
    try {
      const runtime = await invoke<JavaRuntime>("ensure_java", { minecraftVersion: representativeVersion[major] });
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
  if (!profile)
    return <AuthScreen services={services} onAuthenticated={setProfile} />;
  const filteredInstances = instances.filter((instance) =>
    `${instance.displayName ?? ""} ${instance.id}`.toLocaleLowerCase().includes(instanceFilter.trim().toLocaleLowerCase()),
  );
  const sectionTitles: Record<LauncherSection, string> = {
    instances: "Instâncias",
    discover: "Descobrir",
    appearance: "Guarda-roupa",
    java: "Java & engine",
  };
  const selectedContentFiles = contentType === "mod"
    ? instanceContent.mods
    : contentType === "shader"
      ? instanceContent.shaderpacks
      : instanceContent.resourcepacks;
  const visibleInstalledContent = selectedContentFiles.filter((file) =>
    file.name.toLocaleLowerCase().includes(installedContentFilter.trim().toLocaleLowerCase()),
  );
  const enabledContentCount = selectedContentFiles.filter((file) => file.enabled).length;
  const selectedInstalledCount = selectedContentFiles.filter((file) => selectedContentNames.includes(file.name)).length;
  const selectedVisibleCount = visibleInstalledContent.filter((file) => selectedContentNames.includes(file.name)).length;
  const editingInstanceSummary = instances.find((instance) => instance.id === editingInstance);
  return (
    <>
    <main className="shell app-shell">
      <aside className="app-sidebar">
        <div className="brand-lockup">
          <div className="aurora-mark">✦</div>
          <div>
            <strong>Aurora</strong>
            <span>Smart Launcher</span>
          </div>
        </div>
        <div className="sidebar-user" title={`Conta Aurora: ${profile.username}`}>
          <SkinHeadAvatar source={profile.skinUrl ?? profile.avatarUrl} />
          <span><strong>{profile.username}</strong><small>Conta Aurora</small></span>
        </div>
        <nav aria-label="Seções do launcher" className="app-nav">
          {([
            ["instances", "◈", "Instâncias"],
            ["discover", "⌕", "Descobrir"],
            ["appearance", "✦", "Guarda-roupa"],
            ["java", "▣", "Java & engine"],
          ] as const).map(([section, icon, label]) => (
            <button
              aria-current={activeSection === section ? "page" : undefined}
              className={activeSection === section ? "nav-item active" : "nav-item"}
              key={section}
              onClick={() => {
                setActiveSection(section);
                if (section === "discover" && modrinthPacks.length === 0) void loadModrinthPage(0);
              }}
              type="button"
            >
              <span aria-hidden="true">{icon}</span>{label}
            </button>
          ))}
        </nav>
        <div className="sidebar-bottom">
          <button className={assistantOpen ? "compact sidebar-action" : "secondary compact sidebar-action"} onClick={() => setAssistantOpen((current) => !current)} type="button">Assistente</button>
          <button className="text-button sidebar-action" onClick={() => logoutAuroraUser(services)} type="button">Sair da conta</button>
        </div>
      </aside>
      <section className={`workspace active-${activeSection}`}>
        <header className="workspace-header">
          <div>
            <p className="eyebrow">AURORA SMART LAUNCHER · {profile.username}</p>
            <h1>{sectionTitles[activeSection]}</h1>
          </div>
        </header>
        <section className="workspace-grid">
          <div className="workspace-main">
        <article className="panel instances section-instances">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">INSTÂNCIAS</p>
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
                <div
                  aria-current={targetInstance === instance.id ? "true" : undefined}
                  className={`instance${targetInstance === instance.id ? " selected" : ""}${isRunning ? " running" : ""}`}
                  key={instance.id}
                  onClick={() => void selectInstance(instance.id)}
                  onKeyDown={(event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); void selectInstance(instance.id); } }}
                  role="button"
                  tabIndex={0}
                >
                  {instance.iconUrl ? <img className="instance-artwork" alt={`Imagem de ${instance.displayName || instance.id}`} src={instance.iconUrl} /> : <div className="orb">✦</div>}
                  <div className="instance-copy">
                    <strong>{instance.displayName || instance.id}</strong>
                    {instance.displayName && instance.displayName !== instance.id && <small>{instance.id}</small>}
                    <span>
                      {isRunning
                        ? "Rodando agora"
                        : instance.hasModsDirectory
                        ? instance.hasInstalledVersion
                          ? "Minecraft instalado"
                          : "Somente pastas — instale uma versão"
                        : "Verificação necessária"}
                    </span>
                  </div>
                  <div className="instance-actions">
                    <button
                      className="compact"
                      disabled={busy || !instance.hasInstalledVersion || isRunning}
                      onClick={(event) => { event.stopPropagation(); void launchMinecraft(instance.id); }}
                      type="button"
                    >
                      {isRunning ? "● Rodando" : "▶ Iniciar"}
                    </button>
                    <button className="secondary compact" onClick={(event) => { event.stopPropagation(); void openEditor(instance.id); }} type="button">
                      Editar
                    </button>
                    <button className="secondary compact" disabled={busy} onClick={(event) => { event.stopPropagation(); void openInstanceLog(instance.id); }} type="button">Logs</button>
                    <button className="danger compact" disabled={busy || isRunning} onClick={(event) => { event.stopPropagation(); void deleteInstance(instance.id); }} type="button">
                      Excluir
                    </button>
                  </div>
                </div>
              );})}
            </div>
          )}
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
            }} aria-label="Versão Minecraft">
              <option value="1.12.2">1.12.2</option>
              <option value="1.16.5">1.16.5</option>
              <option value="1.19.2">1.19.2</option>
              <option value="1.20.1">1.20.1</option>
              <option value="1.21.1">1.21.1</option>
            </select>
            <select value={loaderChoice} onChange={(event) => setLoaderChoice(event.target.value as LoaderChoice)} aria-label="Tipo da instância">
              <option value="fabric">Fabric</option>
              <option value="forge">Forge</option>
            </select>
            <button disabled={busy || !targetInstance || (loaderChoice === "forge" && !DEFAULT_FORGE_VERSION[minecraftVersion])} type="submit">
              Instalar {loaderChoice === "fabric" ? "Fabric" : "Forge"}
            </button>
          </form>
          {!COMPANION_SUPPORTED.has(minecraftVersion) && (
            <p className="muted">Companion Aurora ainda está em desenvolvimento para esta versão.</p>
          )}
          <p className="muted version-note">Esta versão mostra apenas releases estáveis. Snapshots, betas e NeoForge serão liberados quando a engine oferecer instalação verificável.</p>
            </section>
          )}
        </article>
        <article className="panel catalog-panel section-discover">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">CATÁLOGO</p>
              <h2>Encontre seu próximo mundo</h2>
            </div>
          </div>
          <div aria-label="Fonte do catálogo" className="catalog-tabs" role="tablist">
            <button aria-selected={catalogSource === "modrinth"} className={catalogSource === "modrinth" ? "selected compact" : "secondary compact"} onClick={() => setCatalogSource("modrinth")} role="tab" type="button">Modrinth</button>
            <button aria-selected={catalogSource === "curseforge"} className={catalogSource === "curseforge" ? "selected compact" : "secondary compact"} onClick={() => setCatalogSource("curseforge")} role="tab" type="button">CurseForge</button>
          </div>
          {catalogSource === "modrinth" ? <>
          <form onSubmit={searchModrinth} className="catalog-search-bar">
            <input value={modpackQuery} onChange={(event) => setModpackQuery(event.target.value)} placeholder="Buscar modpack no Modrinth" />
            <label>Mostrar<select value={modrinthPageSize} onChange={(event) => { const size = Number(event.target.value); setModrinthPageSize(size); void loadModrinthPage(0, size); }}><option value={10}>10</option><option value={20}>20</option><option value={30}>30</option><option value={50}>50</option></select></label>
            <button disabled={busy} type="submit">Buscar</button>
          </form>
          {modrinthPacks.length > 0 && (
            <div className="catalog-browser">
              <div className="pack-compact-list">
                {modrinthPacks.map((pack) => (
                  <button className={selectedModrinthPack?.projectId === pack.projectId ? "pack-row selected" : "pack-row"} key={pack.projectId} onClick={() => { setSelectedModrinthPack(pack); void openModpackDetail(pack); }} type="button">
                    {pack.iconUrl ? <img className="pack-icon" src={pack.iconUrl} alt="" /> : <span className="orb">◈</span>}
                    <span><strong>{pack.title}</strong><small>{pack.author || pack.loaders.join(", ")} · {pack.downloads.toLocaleString("pt-BR")} downloads</small></span>
                  </button>
                ))}
              </div>
              {selectedModrinthPack && <aside className="pack-detail">
                {selectedModrinthPack.iconUrl ? <img className="pack-detail-icon" src={selectedModrinthPack.iconUrl} alt="" /> : <div className="orb large">◈</div>}
                <div><p className="eyebrow">MODRINTH · {selectedModrinthPack.author}</p><h2>{selectedModrinthPack.title}</h2></div>
                <p>{selectedModrinthPack.description || "Sem descrição publicada."}</p>
                <div className="pack-badges">{selectedModrinthPack.loaders.map((loader) => <span key={loader}>{loader}</span>)}<span>{selectedModrinthPack.downloads.toLocaleString("pt-BR")} downloads</span></div>
                <label>Versão do Minecraft<select value={packVersions[selectedModrinthPack.projectId] ?? preferredPackVersion(selectedModrinthPack, minecraftVersion)} onChange={(event) => setPackVersions((current) => ({ ...current, [selectedModrinthPack.projectId]: event.target.value }))}>{(selectedModrinthPack.versions.length ? selectedModrinthPack.versions : [minecraftVersion]).map((version) => <option key={version} value={version}>{version}</option>)}</select></label>
                <div className="pack-detail-actions"><button className="secondary" disabled={busy || !targetInstance} onClick={() => void installModrinth(selectedModrinthPack)} type="button">Instalar na selecionada</button><button disabled={busy} onClick={() => void createAndInstallModrinth(selectedModrinthPack)} type="button">Criar e instalar</button></div>
              </aside>}
            </div>
          )}
          <div className="catalog-pagination"><button className="secondary compact" disabled={busy || modrinthPage === 0} onClick={() => void loadModrinthPage(modrinthPage - 1)} type="button">← Anterior</button><span>Página {modrinthPage + 1} de {Math.max(1, Math.ceil(modrinthTotalHits / modrinthPageSize))}</span><button className="secondary compact" disabled={busy || (modrinthPage + 1) * modrinthPageSize >= modrinthTotalHits} onClick={() => void loadModrinthPage(modrinthPage + 1)} type="button">Próxima →</button></div>
          </> : (
            <div className="catalog-unavailable">
              <strong>CurseForge ainda não está conectado.</strong>
              <span>O serviço protegido do Aurora será publicado antes de habilitar buscas e instalações, sem exigir chave API dos jogadores.</span>
            </div>
          )}
        </article>
        {editingInstance && (
            <section className="editor-panel">
              <div className="editor-heading">
                <div className="editor-title">
                  {editingInstanceSummary?.iconUrl ? <img className="editor-instance-artwork" alt="" src={editingInstanceSummary.iconUrl} /> : <span className="editor-instance-icon">◈</span>}
                  <div>
                  <p className="eyebrow">EDITANDO INSTÂNCIA</p>
                  <form className="editor-name-form" onSubmit={renameEditingInstance}>
                    <input aria-label="Nome da instância" disabled={busy || runningInstances.some((running) => running.instanceId === editingInstance)} maxLength={64} onChange={(event) => setRenameDraft(event.target.value)} value={renameDraft} />
                    <button className="secondary compact" disabled={busy || !renameDraft.trim() || renameDraft.trim() === editingInstance || runningInstances.some((running) => running.instanceId === editingInstance)} type="submit">Salvar nome</button>
                  </form>
                    <span>{enabledContentCount} ativos · {selectedContentFiles.length} nesta categoria</span>
                  </div>
                </div>
                <div className="editor-heading-actions">
                  <button className="secondary compact" disabled={busy} onClick={() => void openInstanceFolder(editingInstance)} type="button">Abrir pasta</button>
                  <button aria-label="Fechar editor" className="editor-close" onClick={() => setEditingInstance("")} type="button">×</button>
                </div>
              </div>
              <div className="content-tabs">
                <button className={contentType === "mod" ? "selected compact" : "secondary compact"} onClick={() => selectContentType("mod")} type="button">Mods <span>{instanceContent.mods.length}</span></button>
                <button className={contentType === "shader" ? "selected compact" : "secondary compact"} onClick={() => selectContentType("shader")} type="button">Shaders <span>{instanceContent.shaderpacks.length}</span></button>
                <button className={contentType === "resourcepack" ? "selected compact" : "secondary compact"} onClick={() => selectContentType("resourcepack")} type="button">Pacotes de recursos <span>{instanceContent.resourcepacks.length}</span></button>
              </div>
              <div className="editor-view-tabs" role="tablist" aria-label="Conteúdo da instância">
                <button aria-selected={editorView === "installed"} className={editorView === "installed" ? "active" : ""} onClick={() => setEditorView("installed")} role="tab" type="button">Instalados</button>
                <button aria-selected={editorView === "discover"} className={editorView === "discover" ? "active" : ""} onClick={showContentCatalog} role="tab" type="button">＋ Adicionar</button>
              </div>
              <div className="editor-body">
                {editorView === "installed" ? (
                  <>
                    <div className="editor-toolbar">
                      <label className="editor-filter">
                        <span aria-hidden="true">⌕</span>
                        <input aria-label="Buscar conteúdo instalado" onChange={(event) => setInstalledContentFilter(event.target.value)} placeholder="Buscar nos instalados" value={installedContentFilter} />
                      </label>
                      <div className="editor-bulk-actions">
                        <button className="secondary compact" disabled={busy || visibleInstalledContent.length === 0} onClick={() => setSelectedContentNames(selectedVisibleCount === visibleInstalledContent.length ? selectedContentNames.filter((name) => !visibleInstalledContent.some((file) => file.name === name)) : [...new Set([...selectedContentNames, ...visibleInstalledContent.map((file) => file.name)])])} type="button">{selectedVisibleCount === visibleInstalledContent.length && visibleInstalledContent.length > 0 ? "Limpar visíveis" : "Selecionar visíveis"}</button>
                        <button className="secondary compact" disabled={busy || selectedInstalledCount === 0} onClick={() => void setSelectedContentEnabled(true)} type="button">Ativar selecionados</button>
                        <button className="secondary compact" disabled={busy || selectedInstalledCount === 0} onClick={() => void setSelectedContentEnabled(false)} type="button">Desativar selecionados</button>
                        <button className="danger compact" disabled={busy || selectedInstalledCount === 0} onClick={() => void removeSelectedContent()} type="button">Desinstalar ({selectedInstalledCount})</button>
                      </div>
                    </div>
                    <div className="installed-content content-table">
                      {selectedContentFiles.length === 0 ? (
                        <div className="editor-empty"><span>✦</span><strong>Nada instalado aqui</strong><p>Use “Adicionar” para encontrar conteúdo no Modrinth ou CurseForge.</p><button onClick={showContentCatalog} type="button">Encontrar conteúdo</button></div>
                      ) : visibleInstalledContent.length === 0 ? (
                        <div className="editor-empty compact-empty"><strong>Nenhum resultado</strong><p>Tente buscar por outra parte do nome do arquivo.</p></div>
                      ) : visibleInstalledContent.map((file) => (
                        <div aria-checked={selectedContentNames.includes(file.name)} className={selectedContentNames.includes(file.name) ? "content-row selected" : "content-row"} key={file.name} onClick={() => toggleContentSelection(file.name)} role="checkbox" tabIndex={0} onKeyDown={(event) => { if (event.key === " " || event.key === "Enter") { event.preventDefault(); toggleContentSelection(file.name); } }}>
                          <input aria-label={`Selecionar ${file.name}`} checked={selectedContentNames.includes(file.name)} className="content-select" disabled={busy} onChange={() => toggleContentSelection(file.name)} onClick={(event) => event.stopPropagation()} type="checkbox" />
                          <ContentArtwork compact iconUrl={contentArtwork[contentArtworkKey(contentType, file.name)]?.iconUrl} title={contentArtwork[contentArtworkKey(contentType, file.name)]?.title ?? contentDisplayName(file.name)} type={contentType} />
                          <span className="content-file-details" title={file.name}>
                            <strong>{contentArtwork[contentArtworkKey(contentType, file.name)]?.title ?? contentDisplayName(file.name)}</strong>
                            <small>{file.name}</small>
                          </span>
                          <span className={`content-status ${file.enabled ? "enabled" : ""}`}>{file.enabled ? "Ativo" : "Pausado"}</span>
                          <label className="content-toggle" onClick={(event) => event.stopPropagation()}>
                            <input aria-label={`${file.enabled ? "Desativar" : "Ativar"} ${file.name}`} checked={file.enabled} disabled={busy} onChange={(event) => void setContentEnabled(file, event.target.checked)} type="checkbox" />
                            <span aria-hidden="true" />
                          </label>
                        </div>
                      ))}
                    </div>
                  </>
                ) : (
                  <div className="editor-discover">
                    <div className="content-source-tabs" role="tablist" aria-label="Fonte dos downloads">
                      <button aria-selected={contentSource === "modrinth"} className={contentSource === "modrinth" ? "active" : ""} onClick={() => { setContentSource("modrinth"); void loadContentCatalog({ source: "modrinth", query: "" }); }} role="tab" type="button">Modrinth</button>
                      <button aria-selected={contentSource === "curseforge"} className={contentSource === "curseforge" ? "active" : ""} onClick={() => { setContentSource("curseforge"); void loadContentCatalog({ source: "curseforge", query: "" }); }} role="tab" type="button">CurseForge</button>
                    </div>
                    <form onSubmit={searchContent} className="editor-search-form">
                      <span aria-hidden="true">⌕</span>
                      <input autoFocus value={contentQuery} onChange={(event) => setContentQuery(event.target.value)} placeholder={contentType === "mod" ? "Pesquisar mods" : contentType === "shader" ? "Pesquisar shaders" : "Pesquisar pacotes de recursos"} />
                      <button disabled={busy} type="submit">Pesquisar</button>
                    </form>
                    <div className="content-filter-bar">
                      <label>Versão<select value={contentVersionFilter} onChange={(event) => setContentVersionFilter(event.target.value)}><option value="">Todas</option>{[...new Set([installedMinecraftVersion, minecraftVersion, "1.21.1", "1.20.1", "1.19.2", "1.16.5", "1.12.2"].filter(Boolean))].map((version) => <option key={version} value={version}>{version}</option>)}</select></label>
                      {contentType === "mod" && <label>Loader<select value={contentLoaderFilter} onChange={(event) => setContentLoaderFilter(event.target.value as "" | LoaderChoice)}><option value="">Todos</option><option value="fabric">Fabric</option><option value="forge">Forge</option></select></label>}
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
            </section>
          )}
          {projectDetail && (
            <section aria-label={`Página de ${projectDetail.title}`} aria-modal="true" className="project-detail-modal" role="dialog">
              <header className="project-detail-header">
                <div className="project-detail-identity">
                  {projectDetail.iconUrl ? <img alt="" src={projectDetail.iconUrl} /> : <span>◈</span>}
                  <div><p className="eyebrow">{projectDetail.source === "modrinth" ? "MODRINTH" : "CURSEFORGE"} · {projectDetail.kind === "modpack" ? "MODPACK" : contentType === "mod" ? "MOD" : contentType === "shader" ? "SHADER" : "PACOTE DE RECURSOS"}</p><h2>{projectDetail.title}</h2>{projectDetail.author && <small>por {projectDetail.author}</small>}</div>
                </div>
                <button aria-label="Fechar página do projeto" className="editor-close" onClick={() => setProjectDetail(undefined)} type="button">×</button>
              </header>
              <div className="project-detail-scroll">
                {projectDetail.gallery[0] && <img className="project-hero" src={projectDetail.gallery[0]} alt={`Destaque de ${projectDetail.title}`} />}
                <div className="project-detail-summary"><strong>Sobre este projeto</strong><p>{projectDetail.summary}</p></div>
                {detailLoading ? <div className="detail-loading">Carregando página completa…</div> : projectDetail.bodyFormat === "markdown" ? (
                  <article className="project-description markdown-body"><ReactMarkdown remarkPlugins={[remarkGfm]} components={{ a: (properties) => <a {...properties} rel="noreferrer" target="_blank" />, img: (properties) => <img {...properties} loading="lazy" /> }}>{projectDetail.body}</ReactMarkdown></article>
                ) : (
                  <article className="project-description" dangerouslySetInnerHTML={{ __html: DOMPurify.sanitize(projectDetail.body) }} />
                )}
              </div>
              <footer className="project-detail-actions">
                {projectDetail.websiteUrl && <a href={projectDetail.websiteUrl} rel="noreferrer" target="_blank">Abrir site oficial</a>}
                {projectDetail.kind === "content" && <small>Instalação compatível com Minecraft {installedMinecraftVersion || minecraftVersion}{contentType === "mod" && instanceLoader() ? ` · ${instanceLoader()}` : ""}</small>}
                {projectDetail.kind === "modpack" && projectDetail.pack && <label className="detail-version-select">Versão<select value={packVersions[projectDetail.pack.projectId] ?? preferredPackVersion(projectDetail.pack, minecraftVersion)} onChange={(event) => setPackVersions((current) => ({ ...current, [projectDetail.pack!.projectId]: event.target.value }))}>{(projectDetail.pack.versions.length ? projectDetail.pack.versions : [minecraftVersion]).map((version) => <option key={version} value={version}>{version}</option>)}</select></label>}
                <span />
                {projectDetail.kind === "content" && projectDetail.content && <button disabled={busy} onClick={() => void installContent(projectDetail.content!)} type="button">Instalar na instância</button>}
                {projectDetail.kind === "modpack" && projectDetail.pack && <><button className="secondary" disabled={busy || !targetInstance} onClick={() => void installModrinth(projectDetail.pack!)} type="button">Instalar na selecionada</button><button disabled={busy} onClick={() => void createAndInstallModrinth(projectDetail.pack!)} type="button">Criar e instalar</button></>}
              </footer>
            </section>
          )}
          {instanceLog && (
            <section className="log-panel">
              <div className="panel-heading">
                <div><p className="eyebrow">CONSOLE DA INSTÂNCIA</p><h2>{instanceLog.id} · {instanceLog.log.filename}</h2></div>
                <button className="secondary compact" onClick={() => setInstanceLog(undefined)} type="button">Fechar console</button>
              </div>
              {instanceLog.log.lines.length === 0 ? <p className="muted">Nenhuma linha de log foi gerada ainda.</p> : (
                <pre aria-label="Últimas linhas do log" className="log-viewer">
                  {instanceLog.log.lines.map((line, index) => <code className={/\b(error|exception|failed|fatal)\b/i.test(line) ? "log-error" : /\b(warn|warning)\b/i.test(line) ? "log-warning" : /\b(joined|loaded|starting|done)\b/i.test(line) ? "log-success" : ""} key={`${index}-${line.slice(0, 24)}`}>{line}{"\n"}</code>)}
                </pre>
              )}
            </section>
          )}
          </div>
        <aside className="side-stack">
          <article className="panel section-java">
            <div className="java-manager-heading">
              <div><p className="eyebrow">JAVA & ENGINE</p><h2>Runtimes disponíveis</h2><p className="muted">O Aurora escolhe a versão compatível com cada Minecraft e pode manter Javas isolados sem alterar o restante do computador.</p></div>
              <div className="java-manager-actions"><button className="secondary compact" disabled={busy} onClick={() => void discoverJava()} type="button">↻ Verificar novamente</button><button className="compact" disabled={busy} onClick={() => setJavaInstallOpen((current) => !current)} type="button">+ Instalar Java</button></div>
            </div>
            {javaInstallOpen && (
              <section className="java-install-picker" aria-label="Versões de Java disponíveis para instalação">
                {([8, 17, 21] as const).map((major) => (
                  <button className="java-install-option" disabled={busy} key={major} onClick={() => void installJava(major)} type="button">
                    <span>Java {major}</span><small>{major === 8 ? "Minecraft 1.12–1.16" : major === 17 ? "Minecraft 1.17–1.20.4" : "Minecraft 1.20.5 ou mais recente"}</small><strong>{installingJavaMajor === major ? "Instalando…" : "Instalar"}</strong>
                  </button>
                ))}
              </section>
            )}
            <div className="java-runtime-list">
              {javaRuntimes.length === 0 ? <div className="editor-empty compact-empty"><strong>Nenhum Java encontrado</strong><p>Use “Instalar Java” para baixar uma versão gerenciada pelo Aurora.</p></div> : javaRuntimes.map((runtime) => {
                const selected = runtime.executable === javaPath;
                const major = javaMajor(runtime.version);
                return <button aria-pressed={selected} className={selected ? "java-runtime selected" : "java-runtime"} key={runtime.executable} onClick={() => selectJava(runtime)} type="button"><span className="java-version-mark">{major || "J"}</span><span><strong>{major ? `Java ${major}` : "Java detectado"}</strong><small>{runtime.version}</small><code title={runtime.executable}>{runtime.executable}</code></span><em>{selected ? "Em uso" : "Usar"}</em></button>;
              })}
            </div>
            <details className="java-manual"><summary>Usar um executável manualmente</summary><form onSubmit={verifyJava} className="java-form"><input value={javaPath} onChange={(event) => { setJavaPath(event.target.value); window.localStorage.setItem("aurora.javaExecutable", event.target.value); }} placeholder="C:\\Java\\bin\\java.exe" /><button className="secondary" disabled={busy} type="submit">Validar Java</button></form></details>
          </article>
          <article className="panel section-appearance wardrobe-panel">
            <div className="wardrobe-heading">
              <div><p className="eyebrow">GUARDA-ROUPA</p><h2>Skins <span>({localSkins.length} neste computador)</span></h2></div>
              <div className="wardrobe-actions">
                <button className={skinAddMode === "file" ? "compact" : "secondary compact"} onClick={() => setSkinAddMode("file")} type="button">+ Adicionar skin</button>
                <button className="secondary compact" disabled={busy || !skinUrl.trim()} onClick={() => void saveAppearance({ preventDefault() {} } as FormEvent)} type="button">Equipar no jogo</button>
              </div>
            </div>
            <div className="wardrobe-layout">
              <aside className="wardrobe-nav" aria-label="Biblioteca de skins">
                <strong>Biblioteca</strong>
                <button className="wardrobe-nav-item active" onClick={() => setFavoriteSearch("")} type="button">Minhas skins <span>{localSkins.length}</span></button>
                <button className="wardrobe-nav-item" type="button">Equipada <span>{profile.skinUrl || localSkinPath ? 1 : 0}</span></button>
                <small>Cosméticos e emotes chegarão depois.</small>
              </aside>
              <div className="wardrobe-controls">
                <div className="skin-import">
                  <div className="import-tabs">
                    <button className={skinAddMode === "file" ? "selected compact" : "secondary compact"} onClick={() => setSkinAddMode("file")} type="button">Arquivo</button>
                    <button className={skinAddMode === "url" ? "selected compact" : "secondary compact"} onClick={() => setSkinAddMode("url")} type="button">URL</button>
                    <button className={skinAddMode === "username" ? "selected compact" : "secondary compact"} onClick={() => setSkinAddMode("username")} type="button">Por nick</button>
                  </div>
                  {skinAddMode === "file" && <label className="file-input">Escolher PNG da skin<input accept="image/png" disabled={busy} onChange={(event) => void uploadAppearance("skin", event.target.files?.[0])} type="file" /></label>}
                  {skinAddMode === "url" && <input value={skinUrl} onChange={(event) => { setSkinUrl(event.target.value); setSelectedLocalSkinId(""); }} placeholder="Cole a URL HTTPS da skin PNG" />}
                  {skinAddMode === "username" && <div className="username-search"><input value={skinUsername} onChange={(event) => setSkinUsername(sanitizeMinecraftNickname(event.target.value))} placeholder="Pesquisar por nick Minecraft" maxLength={16} /><button onClick={selectSkinFromUsername} type="button">Pesquisar</button></div>}
                  <div className="model-row"><span>Modelo</span><select value={skinModel} onChange={(event) => setSkinModel(event.target.value as "classic" | "slim")}><option value="classic">Clássico</option><option value="slim">Slim</option></select></div>
                </div>
                <div className="library-toolbar">
                  <input className="favorite-search" value={favoriteSearch} onChange={(event) => setFavoriteSearch(event.target.value)} placeholder="Pesquisar nas suas skins" />
                  <div className="favorite-tools"><input value={favoriteName} onChange={(event) => setFavoriteName(event.target.value)} maxLength={32} placeholder="Nome desta skin" /><button disabled={busy || !skinUrl.trim() || Boolean(selectedLocalSkinId)} onClick={() => void favoriteCurrentSkin()} type="button">Guardar localmente</button></div>
                </div>
                <div className="skin-library wardrobe-library">
                  {localSkins.filter((skin) => skin.name.toLocaleLowerCase().includes(favoriteSearch.trim().toLocaleLowerCase())).map((skin) => (
                    <button className={selectedLocalSkinId === skin.id ? "skin-card active" : "skin-card"} key={skin.id} onClick={() => useFavoriteSkin(skin)} type="button">
                      <img src={skin.previewUrl} alt={`Skin ${skin.name}`} /><strong>{skin.name}</strong><span>{skin.skinModel === "slim" ? "Slim" : "Clássico"}</span><em onClick={(event) => { event.stopPropagation(); void removeFavoriteSkin(skin.id); }}>Remover</em>
                    </button>
                  ))}
                  {localSkins.length === 0 && <p className="muted empty-library">Adicione um PNG, URL ou nick para montar sua biblioteca local.</p>}
                </div>
              </div>
              <div className="skin-preview-stage">
                <p className="eyebrow">PRÉVIA</p>
                <SkinPreviewCanvas skinModel={skinModel} skinUrl={skinUrl} />
                <strong>{favoriteName || profile.username}</strong>
                <span>{localSkinPath ? "Equipada localmente pelo Aurora Companion." : "A biblioteca permanece guardada neste computador."}</span>
              </div>
            </div>
          </article>
          {telemetry && (
            <article className="panel status">
              <p className="eyebrow">COMPANION</p>
              {telemetry.kind === "Telemetry" ? (
                <p>
                  <strong>{telemetry.fps.toFixed(0)} FPS</strong> · {telemetry.mspt.toFixed(1)} MSPT
                  <br />
                  {telemetry.usedMemoryMb} MB · {telemetry.dimension ?? "Mundo carregando"}
                </p>
              ) : telemetry.kind === "OverlayRequested" ? (
                <p>Overlay aberto pelo Companion.</p>
              ) : (
                <p>{telemetry.kind === "Connected" ? `Conectado: ${telemetry.loader} ${telemetry.minecraftVersion}` : "Companion desconectado."}</p>
              )}
            </article>
          )}
        </aside>
        </section>
        <footer>
          <span>{status?.ready ? "Engine pronta" : "Carregando engine"}</span>
          <code>{status?.dataDirectory ?? "Localizando dados…"}</code>
        </footer>
      </section>
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
              <span>Instalação completa</span>
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
          <button aria-label="Fechar mensagem" onClick={() => setNotice("")} type="button">×</button>
        </aside>
      )}
    </main>
    {assistantOpen && <AssistantPanel services={services} username={profile.username} onClose={() => setAssistantOpen(false)} />}
    </>
  );
}

export default function App() {
  return <LauncherApp />;
}
