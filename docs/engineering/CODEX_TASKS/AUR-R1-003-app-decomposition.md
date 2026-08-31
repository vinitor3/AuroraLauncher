# AUR-R1-003 — Primeira decomposição do App React

- **Fase/prioridade:** R1 / P1.
- **Objetivo:** reduzir o acoplamento do `App.tsx` sem redesenhar a UI nem mudar comportamento.
- **Contexto/problema:** `App.tsx` tem cerca de 2.293 linhas e mistura autenticação, navegação, dados, features e apresentação; bundle principal supera 1,5 MB.
- **Resultado esperado:** primeiro slice extraído por comportamento, serviços tipados testáveis e lazy-load onde não muda UX.
- **Dependências:** R0 verde; ownership exclusivo de `App.tsx`.

## Escopo e ownership

- **Pode/deve tocar:** `apps/desktop/src/App.tsx`, novos `features/**`, `services/**`, hooks/testes e estilos estritamente associados.
- **Não tocar:** comandos Rust, Worker/API, schemas Firebase, redesign visual, dependências sem justificativa.
- **Interfaces:** props/eventos/calls Tauri/API continuam idênticos; extrair primeiro a feature de menor risco e maior peso, sugerida Discover/Wardrobe/Assistant após medir imports.

## Passos

1. Caracterizar comportamento atual com testes/smoke e mapa de imports/estado.
2. Extrair tipos/serviço puro, depois hook/controlador, depois view.
3. Preservar estados loading/error/empty e acessibilidade/foco.
4. Lazy-load somente rota/painel independente; medir bundle antes/depois.
5. Não combinar limpeza geral ou renomeação não necessária.

## Testes/comandos

`npm ci`, `npm run build` e testes disponíveis; smoke de login/nav/feature extraída; capturar tamanhos Vite antes/depois. TypeScript zero erro.

## Aceitação/DoD

`App.tsx` diminui materialmente; comportamento observável e contratos preservados; feature testável isoladamente; bundle não regride sem justificativa; diff revisável.

## Riscos/rollback

Closure/state implícito pode mudar ordem de efeitos. Extrair em passos, testes antes; rollback do commit restaura arquivo sem migração de dados.

## Git/PR

- Branch/worktree: `codex/aur-r1-003-app-decomposition` / `AuroraLauncher-wt-r1-003`.
- Commit: `refactor(ui): extract first app feature boundary [AUR-R1-003]`.
- PR depende de R0; integrar isoladamente.
- Execução paralela: **sim**, mas nenhum outro agente toca `App.tsx`.

