type Props = {
  error?: string;
  onRetry?: () => void;
  onSignOut?: () => void;
};

export function StartupScreen({ error, onRetry, onSignOut }: Props) {
  return (
    <main className="startup-shell" aria-busy={!error}>
      <section className={`startup-content${error ? " startup-content-error" : ""}`} aria-labelledby="startup-title">
        <div className="startup-emblem" aria-hidden="true">
          <div className="startup-orbit"><span className="startup-spark startup-spark-one" /></div>
          <div className="startup-orbit startup-orbit-inner"><span className="startup-spark startup-spark-two" /></div>
          <div className="aurora-mark startup-mark"><span className="startup-star">✦</span></div>
        </div>
        <p className="eyebrow">AURORA SMART LAUNCHER</p>
        <h1 id="startup-title">{error ? "Vamos tentar de novo?" : "Preparando sua próxima aventura"}</h1>
        <p className="startup-status" role={error ? "alert" : "status"}>
          {error ?? "Carregando sua sessão com segurança…"}
        </p>
        {error ? (
          <div className="startup-actions">
            <button onClick={onRetry} type="button">Tentar novamente</button>
            {onSignOut && <button className="secondary" onClick={onSignOut} type="button">Voltar para entrar</button>}
          </div>
        ) : (
          <div className="startup-loading" aria-hidden="true">
            <div className="startup-dots"><span className="startup-dot" /><span className="startup-dot" /><span className="startup-dot" /></div>
            <div className="startup-progress"><span /></div>
          </div>
        )}
      </section>
    </main>
  );
}
