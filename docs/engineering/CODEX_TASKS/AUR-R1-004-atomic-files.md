# AUR-R1-004 — Substituição transacional de arquivos

- **Fase/prioridade:** R1 / P0.
- **Objetivo:** garantir que falha de download/commit nunca apague a versão anterior válida.
- **Contexto/problema:** o fluxo atual remove o destino antes do rename; falha posterior perde o arquivo anterior. Operações em lote também podem aplicar estado parcial.
- **Resultado esperado:** staging no mesmo volume, validação tamanho/hash, replace com backup/journal, rollback e limpeza segura.
- **Dependências:** R0 verde; políticas de tamanho/erro de R1-001 coordenadas.

## Escopo e ownership

- **Pode/deve tocar:** domínio/executor Rust de download/substituição e testes temporários.
- **Não tocar:** HTTP/IPC pertencente a R1-001, UI, updater de mods completo, CAS/SQLite futuro.
- **Interfaces:** API existente continua; internamente introduzir `Prepared → Validated → Committed/RolledBack` e erro tipado.

## Passos

1. Reproduzir falha entre remoção e rename em teste.
2. Preparar staging no mesmo filesystem; fsync quando necessário; validar tamanho e hash antes de commit.
3. Preservar destino anterior por replace/backup; commit só encerra após verificação.
4. Journal mínimo para lote; falha no item N restaura itens 1..N-1.
5. Limpar partial/backup apenas após sucesso ou rollback comprovado; proteger symlink/reparse/path escape.

## Testes/comandos

`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`. Casos: destino ausente/existente, hash/tamanho errado, rename negado, disco/IO simulado, cancelamento, lote parcial e path hostil. Verificar bytes antigos bit a bit após toda falha.

## Aceitação/DoD

Nenhum caso de falha remove/corrompe o destino anterior; staging não atravessa volume; rollback idempotente; partials limitados/limpos; contratos e docs atualizados.

## Riscos/rollback

Semântica de replace difere no Windows e antivírus pode bloquear rename. Testar Windows real; rollback mantém implementação antiga somente se o teste de perda permanecer vermelho/bloqueador — não liberar updater.

## Git/PR

- Branch/worktree: `codex/aur-r1-004-atomic-files` / `AuroraLauncher-wt-r1-004`.
- Commit: `fix(storage): make file replacement transactional [AUR-R1-004]`.
- PR depende de R0 e interface R1-001; precede updater/CAS/Tools nível 2.
- Execução paralela: **sim**, com ownership Rust por arquivo explícito.

