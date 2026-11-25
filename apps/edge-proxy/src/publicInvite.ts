const invitePath = /^\/join\/([0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12})$/;

export function publicInvitePage(request: Request, url: URL): Response {
  const id = !url.search && invitePath.exec(url.pathname)?.[1];
  const headers = {
    "Cache-Control": "no-store",
    "Content-Type": "text/html; charset=utf-8",
    "Content-Security-Policy": "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'",
    "Referrer-Policy": "no-referrer",
    "X-Content-Type-Options": "nosniff",
    "X-Robots-Tag": "noindex, nofollow",
  };
  if (!id) return new Response(request.method === "HEAD" ? null : "Convite inválido.", { status: 404, headers });
  if (request.method === "HEAD") return new Response(null, { headers });
  // A message preview and a browser visit must neither disclose a world nor request admission.
  const page = `<!doctype html>
<html lang="pt-BR"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Convite para jogar · Aurora Connect</title>
<meta property="og:title" content="Convite para jogar no Aurora Connect">
<meta property="og:description" content="Abra o convite no Aurora Smart Launcher para ver o mundo e pedir para entrar.">
<meta name="theme-color" content="#171321">
<style>
*{box-sizing:border-box}html{color-scheme:dark;background:#14101c;color:#f4eefb;font:16px/1.6 system-ui,sans-serif}
body{min-height:100vh;margin:0;padding:1.5rem;display:grid;place-items:center}
main{width:min(100%,30rem);padding:clamp(1.4rem,5vw,2.6rem);background:#211b2b;border:1px solid #4e425a;border-radius:1rem}
header{font-weight:650;color:#d7b6ff}h1{font-size:clamp(1.7rem,5vw,2.2rem);line-height:1.2;margin:1rem 0 .65rem}
p{color:#d4cadf;margin:.6rem 0 1.2rem}a{color:#dfc4ff}a.primary{display:inline-block;background:#d8b8ff;color:#281537;padding:.65rem 1.1rem;border-radius:.5rem;font-weight:650;text-decoration:none}
a:focus-visible,summary:focus-visible,input:focus-visible{outline:3px solid #fff;outline-offset:3px}
details{border-top:1px solid #493e54;margin-top:1.8rem;padding-top:1rem}summary{cursor:pointer}
label{display:block;color:#d4cadf}input{width:100%;margin:.35rem 0 .8rem;padding:.6rem;border:1px solid #71617f;border-radius:.4rem;background:#19141f;color:#fff;font:inherit}
</style></head><body><main><header>Aurora Connect</header><h1>Convite para jogar</h1>
<p>Abra o Aurora para ver o mundo. Se a sessão exigir aprovação, o anfitrião receberá seu pedido.</p>
<a class="primary" href="aurora-connect://join/${id}">Abrir convite no Aurora</a>
<details><summary>O launcher não abriu?</summary>
<p>No Aurora, abra <strong>Amigos e comunidade → Convites</strong> e cole o código em <strong>Convite recebido</strong>.</p>
<label>Código do convite<input readonly value="aurora-v1:${id}" spellcheck="false"></label>
<a href="https://github.com/vinitor3/AuroraLauncher/releases">Baixar o launcher para Windows</a>
</details></main></body></html>`;
  return new Response(page, { headers });
}
