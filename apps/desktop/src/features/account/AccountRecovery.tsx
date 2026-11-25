import { useEffect, useId, useRef, useState, type FormEvent } from "react";
import { createPortal } from "react-dom";
import { linkAuroraRecoveryEmail, resetAuroraPassword, verifiedAuroraEmail, type FirebaseServices } from "../../lib/firebase";
import "./accountRecovery.css";

function InboxInstructions({ email, purpose, services }: {
  email: string; purpose: "confirm" | "reset"; services: FirebaseServices;
}) {
  const heading = useRef<HTMLHeadingElement>(null);
  const id = useId();
  useEffect(() => { heading.current?.focus(); }, []);
  const official = services.config.projectId === "auroralauncher" && services.config.authDomain === "auroralauncher.firebaseapp.com";
  return <section className="recovery-inbox" aria-labelledby={id}>
    <h3 id={id} ref={heading} tabIndex={-1}>Confira sua caixa de entrada</h3>
    <p>{purpose === "reset" ? "Se houver uma conta vinculada a este endereço, você receberá um link de recuperação." :
      "Se o endereço estiver disponível, você receberá um link para confirmá-lo."}</p>
    <p className="recovery-destination"><strong dir="auto">{email.trim().split("@")[0]}@<wbr />{email.trim().split("@")[1]}</strong></p>
    <p>{purpose === "reset" ? "Abra o link recebido, escolha a nova senha e volte ao launcher para entrar." :
      "Abra o link recebido. Na próxima entrada, use esse e-mail com sua senha atual."}</p>
    <small>{official && <>A página oficial da conta abre em <bdi>auroralauncher.firebaseapp.com</bdi>. </>}
      Se a mensagem demorar, confira também o spam.</small>
  </section>;
}

export function PasswordResetForm({ services, back }: { services: FirebaseServices; back: () => void }) {
  const [email, setEmail] = useState("");
  const [busy, setBusy] = useState(false);
  const [sent, setSent] = useState(false);
  const [error, setError] = useState("");
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (busy) return;
    setBusy(true); setError("");
    try {
      await resetAuroraPassword(services, email);
      setSent(true);
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : "Tente novamente.");
    } finally {
      setBusy(false);
    }
  }
  return <div className="auth-form">
    {sent ? <InboxInstructions purpose="reset" email={email} services={services} /> : <>
      <p>Digite o e-mail confirmado na sua conta Aurora para receber um link de recuperação.</p>
      <form className="auth-form" onSubmit={submit}>
        <label>E-mail confirmado<input type="email" autoComplete="email" maxLength={254} value={email} disabled={busy} onChange={event => setEmail(event.target.value)} required /></label>
        {error && <p className="form-error" role="alert">{error}</p>}
        <button type="submit" disabled={busy}>{busy ? "Enviando…" : "Enviar link de recuperação"}</button>
      </form>
      <details className="recovery-help"><summary>Minha conta usa apenas um nick</summary><p>Entre com seu nick e a senha atual. No menu da conta, abra <strong>Recuperação da conta</strong> para vincular um e-mail.</p></details>
    </>}
    <button type="button" className={sent ? "" : "secondary"} onClick={back}>Voltar para entrar</button>
    {sent && <button type="button" className="text-button" onClick={() => { setSent(false); setError(""); }}>Corrigir endereço</button>}
  </div>;
}

export function RecoveryEmailDialog({ services, uid, close }: { services: FirebaseServices; uid: string; close: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const title = useId();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [sent, setSent] = useState(false);
  const [error, setError] = useState("");
  const user = services.auth.currentUser;
  const current = user?.uid === uid ? verifiedAuroraEmail(user) : null;
  useEffect(() => {
    const element = dialog.current;
    element?.showModal();
    return () => element?.close();
  }, []);
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (busy) return;
    setBusy(true); setError("");
    try {
      await linkAuroraRecoveryEmail(services, uid, password, email);
      setSent(true);
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : "Tente novamente.");
    } finally {
      setPassword(""); setBusy(false);
    }
  }
  return createPortal(<dialog ref={dialog} className="account-recovery-dialog" aria-labelledby={title}
    onCancel={event => { event.preventDefault(); close(); }} onKeyDown={event => event.stopPropagation()}>
    <header><h2 id={title}>Recuperação da conta</h2><button className="secondary compact" type="button" onClick={close}>Fechar</button></header>
    {sent ? <>
      <InboxInstructions purpose="confirm" email={email} services={services} />
      <button type="button" className="secondary" onClick={() => { setSent(false); setError(""); }}>Corrigir endereço</button>
    </> : <>
      {current && <p className="recovery-destination"><span>E-mail confirmado</span><strong dir="auto">{current}</strong></p>}
      <p>{current ? "Informe o novo endereço para receber o link de confirmação." : "Vincule um e-mail para poder recuperar sua senha. Até confirmar o endereço, entre com seu nick."}</p>
      <form className="auth-form" onSubmit={submit}>
        <label>{current ? "Novo e-mail" : "Seu e-mail"}<input type="email" autoComplete="email" value={email} maxLength={254} required disabled={busy} onChange={event => setEmail(event.target.value)} /></label>
        <label>Senha atual<input type="password" autoComplete="current-password" value={password} required disabled={busy} onChange={event => setPassword(event.target.value)} /></label>
        {error && <p role="alert" className="form-error">{error}</p>}
        <button type="submit" disabled={busy}>{busy ? "Enviando…" : "Enviar link de confirmação"}</button>
      </form>
    </>}
  </dialog>, document.body);
}
