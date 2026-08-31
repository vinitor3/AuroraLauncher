# Plano de execução paralela Codex

Este plano é executável por worktrees locais. A branch atual `docs/aur-r0-engineering-plan` contém apenas a baseline/planejamento e o patch de regra ainda não implantado. Nenhum agente deve trabalhar diretamente em `main`.

## Regras comuns

- Baseline: `5aa5fe8`; atualizar o SHA explicitamente antes de iniciar se `main` avançar.
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
- Nenhum agente da Wave 0 toca `App.tsx`, `commands.rs`, Worker ou Companion.

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

R1-001 e R1-004 podem tocar módulos Rust próximos; Integration Agent deve separar ownership por arquivo antes do spawn. HUD começa somente depois do harness.

## Política de worktrees

Exemplo conceitual (ajustar caminho e baseline, sem copiar cegamente):

```text
main checkout:       C:\Users\vinic\Desktop\AuroraLauncher
security worktree:   C:\Users\vinic\Desktop\AuroraLauncher-wt-r0-001
license worktree:    C:\Users\vinic\Desktop\AuroraLauncher-wt-r0-002
ci worktree:         C:\Users\vinic\Desktop\AuroraLauncher-wt-r0-003
release worktree:    C:\Users\vinic\Desktop\AuroraLauncher-wt-r0-004
integration:         C:\Users\vinic\Desktop\AuroraLauncher-wt-integration
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
- CI: checks determinísticos cobrindo frontend, Rust, Worker, Functions, Companion e regras.
- Release: versão única, release histórica congelada, manifesto/hash reproduzível e docs sem alegação falsa.
- QA final sem P0/P1 aberto para o escopo de distribuição.

