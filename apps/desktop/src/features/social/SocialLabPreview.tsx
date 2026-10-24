import { useLayoutEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { deleteApp } from "firebase/app";
import { signOut } from "firebase/auth";
import { terminate } from "firebase/firestore";
import { bundledFirebaseConfig, createFirebaseServices, loadAuroraProfile, loginAuroraUser, socialLabBuild, type FirebaseServices } from "../../lib/firebase";
import { createSocialFirebaseService } from "../../lib/socialFirebase";
import type { SocialService } from "../../shared/social";
import { SocialView } from "./SocialView";
import { createPreviewOwner } from "./preview-lifecycle";
import "../../styles.css";
import "./social-preview.css";

// Public, disposable credentials from firebase/tests/social.seed.mjs. This
// entrypoint cannot initialize outside the fixed local-emulator build mode.
const fixturePassword = "aurora-local-fixture-24";
const fixtureAccounts = ["social_alfa", "social_beta", "social_gama"] as const;
type FixtureAccount = typeof fixtureAccounts[number];
type Session = { services: FirebaseServices; client: SocialService; uid: string; nickname: string };

function SocialLabPreview({ registerCleanup }: { registerCleanup: (cleanup: () => Promise<void>) => void }) {
  const [session, setSession] = useState<Session>();
  const [pending, setPending] = useState(false);
  const [cleanupBlocked, setCleanupBlocked] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const owned = useRef<FirebaseServices | undefined>(undefined);
  const closing = useRef(new WeakMap<FirebaseServices, Promise<void>>());
  const authFlight = useRef<{ services: FirebaseServices; promise: Promise<unknown> } | undefined>(undefined);
  const busy = useRef(false);
  const alive = useRef(true);
  const epoch = useRef(0);
  const allowed = socialLabBuild && ["127.0.0.1", "localhost"].includes(window.location.hostname);

  function closeServices(services: FirebaseServices): Promise<void> {
    const prior = closing.current.get(services);
    if (prior) return prior;
    const close = (async () => {
      let failed = false;
      // An in-flight login must settle before this SDK app is destroyed. The
      // generation check below prevents it from starting a later profile read.
      if (authFlight.current?.services === services) await authFlight.current.promise.catch(() => undefined);
      try { await signOut(services.auth); } catch { failed = true; }
      try { await terminate(services.db); } catch { failed = true; }
      try { await deleteApp(services.app); } catch { failed = true; }
      if (failed) throw new Error("LOCAL_SESSION_CLEANUP_FAILED");
    })();
    closing.current.set(services, close); return close;
  }

  useLayoutEffect(() => {
    let detached: Promise<void> | undefined;
    const detach = () => {
      if (detached) return detached;
      alive.current = false; ++epoch.current;
      const services = owned.current;
      detached = services ? closeServices(services).then(() => {
        if (owned.current === services) owned.current = undefined;
      }) : Promise.resolve();
      return detached;
    };
    registerCleanup(detach);
    return () => { void detach().catch(() => undefined); };
  }, [registerCleanup]);

  async function open(nickname: FixtureAccount) {
    if (!alive.current || !allowed || busy.current || cleanupBlocked || session || !fixtureAccounts.includes(nickname)) return;
    busy.current = true; setPending(true); setError(""); setMessage(""); const generation = ++epoch.current;
    let services: FirebaseServices | undefined;
    try {
      services = createFirebaseServices(bundledFirebaseConfig, { id: `social-preview-${crypto.randomUUID()}`, persistent: false });
      owned.current = services;
      const localServices = services;
      const login = (async () => {
        const credential = await loginAuroraUser(localServices, nickname, fixturePassword);
        if (!alive.current || epoch.current !== generation) return null;
        const profile = await loadAuroraProfile(localServices, credential.user);
        if (profile.username !== nickname || profile.usernameLower !== nickname || profile.uid !== credential.user.uid) throw new Error("LOCAL_FIXTURE_PROFILE_MISMATCH");
        return profile;
      })();
      authFlight.current = { services, promise: login };
      const profile = await login;
      if (!profile || !alive.current || epoch.current !== generation) { await closeServices(services); return; }
      setSession({ services, client: createSocialFirebaseService(services), uid: profile.uid, nickname });
    } catch {
      let cleanupFailed = false;
      if (services) try { await closeServices(services); } catch { cleanupFailed = true; }
      if (!cleanupFailed && owned.current === services) owned.current = undefined;
      if (alive.current && epoch.current === generation) {
        if (cleanupFailed) { setCleanupBlocked(true); setError("O encerramento da tentativa não foi confirmado. Feche esta aba antes de abrir outro laboratório; novas entradas estão bloqueadas aqui."); }
        else setError("Não foi possível entrar na conta local. Confira os emuladores e a preparação das três contas de teste.");
      }
    } finally {
      if (alive.current && epoch.current === generation) { busy.current = false; setPending(false); }
    }
  }

  async function close() {
    if (busy.current || !owned.current) return;
    busy.current = true; setPending(true); setError(""); setMessage(""); const generation = ++epoch.current;
    const services = owned.current; setSession(undefined);
    try {
      await closeServices(services);
      if (owned.current === services) owned.current = undefined;
      if (alive.current && epoch.current === generation) setMessage("Sessão local encerrada; autenticação e conexão de dados liberadas.");
    } catch {
      owned.current = services;
      if (alive.current && epoch.current === generation) { setCleanupBlocked(true); setError("O encerramento local não foi confirmado por completo. Feche esta aba antes de outra tentativa; novas entradas estão bloqueadas aqui."); }
    } finally {
      if (alive.current && epoch.current === generation) { busy.current = false; setPending(false); }
    }
  }

  if (!allowed) return <main className="social-preview"><div className="social-notice" data-kind="warning"><strong>Laboratório indisponível neste endereço.</strong><p>Esta página só funciona no modo social-lab, servido localmente. Nenhuma conta foi acessada.</p></div></main>;
  return <main className="social-preview">
    <header className="social-preview-bar"><div><strong>Aurora · teste social local</strong><p>Contas descartáveis · emuladores neste PC · sem conexão com contas publicadas</p></div><span className="social-tag">Ambiente local de teste</span></header>
    {error && <div className="social-notice" data-kind="error" role="alert">{error}</div>}
    {message && <div className="social-notice" role="status">{message}</div>}
    {session ? <><div className="social-preview-account"><span>Conta local: <strong>{session.nickname}</strong></span><button className="secondary" disabled={pending} onClick={() => void close()}>Encerrar sessão local</button></div><SocialView service={session.client} accountId={session.uid} nickname={session.nickname} /></>
      : <section className="social-preview-login"><p className="eyebrow">ENTRAR NO LABORATÓRIO</p><h1>Escolha uma conta de teste</h1><p>Abra outra aba e escolha outra conta para exercitar amizade e descoberta pela interface real. As contas precisam ter sido preparadas no emulador.</p><div className="social-actions">{fixtureAccounts.map(nickname => <button key={nickname} disabled={pending || cleanupBlocked} onClick={() => void open(nickname)}>{pending ? "Aguarde…" : nickname}</button>)}</div><small>Esta página não cria contas, amizades ou anúncios automaticamente.</small></section>}
  </main>;
}

// HMR must not create another root over a live root or reopen a local account
// before the previous owner has completed its original SDK cleanup.
const container = document.getElementById("root")!;
const previous = import.meta.hot?.data.previousCleanup as Promise<void> | undefined;
const owner = createPreviewOwner({
  previous,
  mount(registerCleanup) {
    const root = createRoot(container);
    root.render(<SocialLabPreview registerCleanup={registerCleanup} />);
    return { unmount: () => root.unmount() };
  },
  blocked() {
    const message = document.createElement("p");
    message.className = "social-notice"; message.setAttribute("role", "alert");
    message.textContent = "O encerramento do laboratório anterior não foi confirmado. Feche esta aba; novas entradas estão bloqueadas.";
    container.replaceChildren(message);
  },
});
if (import.meta.hot) {
  import.meta.hot.accept();
  import.meta.hot.dispose(data => { data.previousCleanup = owner.dispose(); });
}
