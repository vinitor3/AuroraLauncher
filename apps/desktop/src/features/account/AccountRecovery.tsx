import { useEffect, useId, useRef, useState, type FormEvent } from "react";
import { createPortal } from "react-dom";
import { linkAuroraRecoveryEmail, resetAuroraPassword, verifiedAuroraEmail, type FirebaseServices } from "../../lib/firebase";
import "./accountRecovery.css";

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
    {sent ? <p role="status">Se esse e-mail estiver vinculado a uma conta Aurora, você receberá um link para escolher uma nova senha. Confira também a pasta de spam.</p> : <>
      <p>Use o e-mail que você confirmou em sua conta Aurora. Se sua conta usa apenas um nick, você precisa da senha atual para cadastrar um endereço.</p>
      <form className="auth-form" onSubmit={submit}>
        <label>E-mail confirmado<input type="email" autoComplete="email" maxLength={254} value={email} onChange={event => setEmail(event.target.value)} required /></label>
        {error && <p className="form-error" role="alert">{error}</p>}
        <button type="submit" disabled={busy}>{busy ? "Enviando…" : "Enviar link de recuperação"}</button>
      </form>
    </>}
    <button type="button" className="secondary" onClick={back}>Voltar para entrar</button>
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
    {sent ? <p role="status">Confira a caixa de entrada e o spam. Se o endereço puder ser vinculado, o Firebase enviará um link. Depois de confirmar, entre no Aurora com esse e-mail e sua senha atual.</p> : <>
      {current && <p>E-mail confirmado: <strong>{current}</strong></p>}
      <p>{current ? "Para trocar seu endereço, confirme o novo e-mail." : "Vincule um e-mail seu para recuperar a senha. Até confirmar o link, continue entrando com o nick."} Após a confirmação, o e-mail será seu login. Seu nick no jogo e suas amizades continuam no mesmo perfil.</p>
      <form className="auth-form" onSubmit={submit}>
        <label>{current ? "Novo e-mail" : "Seu e-mail"}<input type="email" autoComplete="email" value={email} maxLength={254} required onChange={event => setEmail(event.target.value)} /></label>
        <label>Senha atual<input type="password" autoComplete="current-password" value={password} required onChange={event => setPassword(event.target.value)} /></label>
        {error && <p role="alert" className="form-error">{error}</p>}
        <button type="submit" disabled={busy}>{busy ? "Enviando…" : "Enviar confirmação"}</button>
      </form>
    </>}
  </dialog>, document.body);
}
