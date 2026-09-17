# Tarefa — downloads externos automáticos

Data: 17/09/2026. Branch: `codex/aur-auto-external-downloads`.
Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**.

## Entrega

- Reutilização de arquivo local validado; alternativa oficial Modrinth somente
  por SHA-1 exato; fallback para a página oficial CurseForge visível.
- Redirecionamentos de download HTTPS para `forgecdn.net` permitidos e
  captura do arquivo sem diálogo de destino; hash validado antes de instalar.
- Fila automática serial com correlação por UUID, proteção contra duplicatas,
  timeout/cancelamento e descarte de eventos de tentativas antigas.
- Liberação do bloqueio da instância antes do próximo arquivo, cancelamento
  anterior ao registro lembrado e publicação tardia recusada.
- Finalização/hash fora da thread de interface; preservação de arquivos
  existentes em falhas; confinamento de destino e proteção contra links.

## Validação

- Build frontend aprovado; apenas aviso preexistente de tamanho do bundle.
- Sete testes da fila frontend aprovados: sequência, duplicidade, eventos
  antigos, timeout, falha pendente, desmontagem e nova tentativa.
- Suíte Rust: 85 testes aprovados e três online ignorados por padrão.
- Teste online específico aprovado separadamente: resolução e download real
  de `shotsfired-1.20.1-0.2.2.jar` no Modrinth, com SHA-1, tamanho (1.036.594
  bytes), progresso e limpeza do staging conferidos. Não instalou o mod em
  uma instância do jogador.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` e
  `git diff --check` aprovados.
- QA independente revisou contrato/eventos, cancelamento, caminhos e hash;
  as corridas concretas encontradas foram corrigidas e ganharam testes.

Comandos reproduzíveis:

```powershell
npm --prefix apps/desktop run build
node --test apps/desktop/tests/externalDownloadQueue.test.mjs
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml live_shotsfired_exact_hash_resolves_and_downloads -- --ignored
```

## Limites e pendências

- Fluxo completo no WebView2 real (countdown, confirmação, fechamento e vários
  arquivos) e execução do modpack no Minecraft: **PRECISA DE TESTE**.
- O exemplo `shotsfired-1.20.1-0.2.2.jar` está publicado no Modrinth com SHA-1
  `f5ba631f02cd9df539417b0be4393a81e00ed0e0`. A igualdade com o SHA-1 da API
  CurseForge é exigida em execução; o hash primário CurseForge não foi
  confirmado neste teste público.
- Sites podem pedir confirmação/CAPTCHA. O Aurora não ignora essas exigências.
- Cancelamento do mirror interrompe entre blocos; uma leitura já bloqueada
  permanece limitada pelo timeout de rede. Não há publicação após cancelar.
- Updater de conteúdo/modpacks não ganha aplicação automática de arquivos
  bloqueados nesta tarefa; o escopo é instalação e dependências.
- Sem deploy, mudança de versão ou novo instalador, conforme pedido do usuário.
  A instalação alpha.6 já existente não recebe esta mudança automaticamente.

Arquitetura: `engineering/ADR/ADR-017-external-content-downloads.md`.
