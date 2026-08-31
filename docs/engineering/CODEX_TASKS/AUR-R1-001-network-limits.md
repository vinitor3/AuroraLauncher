# AUR-R1-001 — Limites de rede e autenticação IPC

- **Fase/prioridade:** R1 / P0.
- **Objetivo:** impedir espera infinita, consumo ilimitado e conexão loopback que monopolize o Companion.
- **Contexto/problema:** downloads/HTTP precisam de timeout/cancelamento/teto de bytes; IPC pode esperar autenticação indefinidamente e nonce em linha de comando não protege contra processos do mesmo usuário.
- **Resultado esperado:** políticas tipadas de timeout/tamanho/conexões, cancelamento e erros recuperáveis, sem quebrar comandos Tauri existentes.
- **Dependências:** R0 verde; coordenar ownership Rust com AUR-R1-004.

## Escopo e ownership

- **Pode/deve tocar:** módulos Rust de HTTP e IPC, testes/fixtures correspondentes.
- **Não tocar:** substituição final de arquivo de R1-004, UI React, Worker, protocolo remoto/social.
- **Interfaces:** nomes/payloads de comandos Tauri e protocolo Companion permanecem compatíveis; novo erro deve ser serializável.

## Passos

1. Mapear cada cliente/request e cada estado de conexão IPC.
2. Aplicar connect/read/total timeout, cancelamento e limite de bytes por tipo.
3. Exigir autenticação inicial IPC em 2–5 s, frame máximo e limite/rejeição de conexões concorrentes.
4. Reduzir vida/reuso do segredo; avaliar named pipe com ACL por usuário em ADR separado, sem ampliar escopo.
5. Testar servidor lento, stream infinito, frame grande, nonce errado, cliente ocioso e cancelamento.

## Testes/comandos

`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`; fixtures locais sem internet. Confirmar que arquivo parcial é limpo e destino anterior permanece responsabilidade de R1-004.

## Aceitação/DoD

Nenhuma espera ilimitada; erro identifica fase sem vazar token; limites têm defaults documentados e podem ser testados; comandos públicos não quebram; testes determinísticos verdes.

## Riscos/rollback

Timeout agressivo prejudica rede lenta. Usar classes/configuração limitada, não “sem timeout”. Rollback reverte policy mantendo testes de reproduções como evidência.

## Git/PR

- Branch/worktree: `codex/aur-r1-001-network-limits` / `AuroraLauncher-wt-r1-001`.
- Commit: `fix(network): bound http and ipc sessions [AUR-R1-001]`.
- PR depende de R0; integrar antes de R1-004 se interface compartilhada.
- Execução paralela: **sim**, somente com ownership Rust por arquivo acordado.

