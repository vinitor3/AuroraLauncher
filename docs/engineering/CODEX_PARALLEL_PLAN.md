# Plano de execução paralela Codex

Este plano é executável por worktrees locais. A branch `docs/aur-r0-engineering-plan` e a base `5aa5fe8` registravam a revisão inicial; não são a base atual de produto. A revisão Connect V2 foi reconciliada com alpha.14 em `6a107b8`; confirmar HEAD, manifests, matriz e entregas concluídas antes de iniciar. Nenhum agente deve trabalhar diretamente em `main`.

## Regras comuns

- Baseline: selecionar a base atual reconciliada e registrar o SHA; nunca reutilizar `5aa5fe8` ou alpha.13 descartando entregas posteriores.
- Uma tarefa, uma branch, um worktree e um commit lógico; não misturar refactor oportunista.
- Não editar arquivos fora do ownership. Dependência nova ou mudança de contrato exige registrar no PR e avisar integração.
- Nunca commitar secrets, tokens, `.env`, credenciais, chaves de updater ou material Microsoft/Firebase.
- Antes do commit: testes da tarefa, `git diff --check` e revisão do diff completo.
- Entrega do implementador: resumo, arquivos, comandos/resultados, riscos restantes, rollback e SHA.

## Wave 0 — iniciar agora

| Agente | Tarefa | Branch sugerida | Ownership exclusivo | Dependência | Paralelo |
| --- | --- | --- | --- | --- | --- |
| Security | AUR-R0-001 | `codex/aur-r0-001-firestore-rules` | `firebase/firestore.rules`, `firebase/tests/**`, scripts de teste Firebase estritamente necessários | nenhuma | sim |
| Licensing | AUR-R0-002 | `codex/aur-r0-002-license` | `LICENSE*`, `NOTICE*`, `THIRD_PARTY_NOTICES.md`, headers/manifest metadata de licença | decisão D-001 antes do merge final | sim |
| CI | AUR-R0-003 | `codex/aur-r0-003-ci-baseline` | `.github/workflows/**`, scripts de CI novos | contratos de comandos existentes | sim |
| Release/docs | AUR-R0-004 | `codex/aur-r0-004-release-provenance` | checker de versão/hash, manifests de versão, `README.md`, `docs/module-a.md`, docs de release | não alterar artefato/tag histórica | sim |

### Fronteiras de conflito

- `package.json` raiz pertence a Release/docs durante a wave; CI chama scripts existentes ou adiciona script em arquivo próprio. Se CI precisar mudar o root manifest, entrega patch separado ao Integration Agent.
- `LICENSE.md` e `THIRD_PARTY_NOTICES.md` pertencem exclusivamente a Licensing.
- `SECURITY.md` pertence à baseline desta revisão; implementadores só propõem adendo em patch separado.
- Nenhum agente da Wave 0 toca `App.tsx`, `commands.rs`, Worker ou módulos Java.

## Integração da Wave 0

Ordem candidata:

1. AUR-R0-001, porque fecha a vulnerabilidade e estabelece testes de regra.
2. AUR-R0-002, após decisão de licença.
3. AUR-R0-003, já apontando para os checks novos.
4. AUR-R0-004, reconciliando a versão/documentação final sem reescrever história.
5. Integration Agent executa suíte agregada e corrige somente glue.
6. QA Agent faz revisão adversarial e emite `GO`, `GO WITH EXCEPTIONS` ou `NO-GO`.

## Wave 1 — somente após gate

| Agente | Tarefa | Ownership | Dependência |
| --- | --- | --- | --- |
| Rust network | AUR-R1-001 | módulos IPC/HTTP e testes correlatos | R0 verde |
| Runtime QA | AUR-R1-002 | harness/fixtures/evidências, sem reescrever adapters | R0 verde |
| Frontend | AUR-R1-003 | `App.tsx` e novos módulos feature/service | R0 verde; ownership exclusivo de `App.tsx` |
| Rust filesystem | AUR-R1-004 | executor de download/substituição e testes | R0 verde; contrato coordenado com R1-001 |

R1-001 e R1-004 podem tocar módulos Rust próximos; Integration Agent deve separar ownership por arquivo antes do spawn. HUD/Companion foram retirados pela ADR-018; o harness homologa Core/Skins e, separadamente, o Assistente desktop.

## R4 — Connect V2 em incremento isolado

Usar [backlog](CODEX_TASKS/AUR-R4-CONNECT-BACKLOG.md) e [prompt](CODEX_PROMPTS/AUR-R4-CONNECT-V2.md). Pesquisa e PoC local de transporte autorizadas não liberam produto, UI pública ou sincronização antes dos gates. Não refazer recovery/Doctor/Tools já entregues. Um proprietário do domínio Connect, outro da revisão de ameaça/testes; manifests, registro Tauri e monólitos continuam com owner exclusivo. Adaptadores SVC/Plasmo podem ser investigados separadamente, mas qualquer edição compartilhada no Core exige serialização e matriz afetada. A revisão documental paralela de outro chat não autoriza sobrescrever seus arquivos.

Execução atual `codex/aur-r4-connect`: base `5ac2f7e`, worktree exclusiva. Ownership delimitado: transporte em `apps/connect-lab/`; documentação canônica em `docs/engineering/`; coordenador possui scripts, documento operacional e task principal; QA independente revisa sem editar os mesmos arquivos. Os monólitos desktop/Worker e Java permanecem sem owner de edição nesta wave porque não são necessários à ponte de laboratório. [IDs e contratos reconciliados](AURORA_CONNECT_REVIEW.md). Continuar incrementos locais independentes após cada teste/revisão; uma pendência de rede externa não interrompe o restante autorizado.

## Política de worktrees

Exemplo conceitual (ajustar caminho e baseline, sem copiar cegamente):

```text
main checkout:       C:\Users\usuario\Desktop\AuroraLauncher
security worktree:   C:\Users\usuario\Desktop\AuroraLauncher-wt-r0-001
license worktree:    C:\Users\usuario\Desktop\AuroraLauncher-wt-r0-002
ci worktree:         C:\Users\usuario\Desktop\AuroraLauncher-wt-r0-003
release worktree:    C:\Users\usuario\Desktop\AuroraLauncher-wt-r0-004
integration:         C:\Users\usuario\Desktop\AuroraLauncher-wt-integration
```

Não criar worktree dentro de outro worktree. Não remover worktree com alterações. A criação/remoção real permanece responsabilidade de quem inicia a wave.

## Contrato de handoff

Cada agente devolve:

```text
Task ID:
Branch / commit:
Arquivos alterados:
Resultado e decisões:
Testes executados e saída resumida:
Testes não executados e por quê:
Riscos/pendências:
Rollback:
```

## Critério para abrir a próxima wave

- Firestore: suíte positiva/negativa no Emulator, perfis existentes auditados e deployment registrado.
- Licença: escopo aprovado, texto integral, SPDX e avisos coerentes.
- CI: checks determinísticos cobrindo frontend, Rust, Worker, Functions, Core/Skins e regras.
- Release: versão única, release histórica congelada, manifesto/hash reproduzível e docs sem alegação falsa.
- QA final sem P0/P1 aberto para o escopo de distribuição.
