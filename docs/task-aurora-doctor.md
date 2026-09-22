# Aurora Doctor — diagnóstico local

Estado da entrega: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. As verificações abaixo distinguem testes sintéticos de uso real; nenhuma execução de testes sintéticos homologa a matriz completa de Minecraft/loaders.

## Base e escopo

Referência: repositório principal `AuroraLauncher`, commit-base `4ef78961d5e34a7e982d72182e6a345eb6f89a90`, launcher `0.1.1-alpha.9`. Implementação isolada na branch `codex/aur-doctor-local`, worktree exclusiva `codex-aur-doctor-local`.

O incremento implementa backend Rust, integração com tentativas de lançamento, comandos Tauri, área Diagnóstico e relatório sanitizado. A análise da instância é somente leitura. Core, Skins e Assistente desktop são preservados; Companion não é recriado. Não amplia versões/loaders, não adiciona dependências de nuvem, não gera instalador e não publica release.

Ownership: integração/comandos e persistência de tentativas; engine e regras; interface React; QA independente e documentação. Arquivos monolíticos permanecem sob propriedade exclusiva durante a implementação.

## Verificação

Fixtures em `apps/desktop/src-tauri/tests/fixtures/doctor/` são **sintéticas**, sem logs reais de jogadores. A suíte independente `doctor_qa.rs` cobre: dependência obrigatória explícita; stack trace ambígua; crash antigo inalterado; primeiro registro anexado; linha herdada parcialmente escrita; modificação posterior ao fim da execução; sucesso/cancelamento; resultado em andamento/desconhecido; UTF-16 grande e limitado; UTF-8 inválido; ausência de Java/perfil; perfil inválido/grande; relatório completo sem dados privados; ausência de mutação; junctions de logs e ancestrais no Windows.

Durante a revisão independente, foram identificadas condições de corrida entre finalização, polling do processo e gravação do estado de lançamento. A integração passou a proteger a atualização de resultado e a preservação de estados terminais. Cinco testes do journal cobrem concorrência, saída normal/cancelada, privacidade, reinício do observador e recriação da instância sem herdar uma execução antiga. A engine também passou a conservar a primeira linha completa anexada, excluir fragmentos herdados, limitar estritamente o fim temporal da execução e decodificar corretamente o final de logs UTF-16.

Execução independente: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test doctor_qa` — **CONCLUÍDO**, 15 testes aprovados, 1 smoke real ignorado por padrão.

| Verificação executada | Resultado e limite |
| --- | --- |
| `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` | **CONCLUÍDO**: 89 testes da biblioteca, 27 do binário e 15 da QA, total de 131 aprovados; 5 ignorados por padrão. |
| `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings` | **CONCLUÍDO**, sem warnings. |
| `npm --prefix apps/desktop run build` | **CONCLUÍDO**; aviso de tamanho de chunk do Vite, sem falha do build. |
| `cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml` | **CONCLUÍDO**, binário nativo debug; não é instalador/release. |
| `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all -- --check` | **CONCLUÍDO**, formatação validada. |
| `git diff --check` | **CONCLUÍDO**, sem erro de whitespace. |

O teste `real_instance_read_only_smoke` é opt-in (`--ignored`) e usa somente uma instância explicitamente selecionada por variáveis `AURORA_DOCTOR_SMOKE_ROOT`, `AURORA_DOCTOR_SMOKE_INSTANCE`, `AURORA_DOCTOR_SMOKE_VERSION` e `AURORA_DOCTOR_SMOKE_MINECRAFT`. **CONCLUÍDO**: 1 teste aprovado em um modpack realmente instalado, usando os dados de versão de seu manifesto; 2 logs disponíveis; resultado `incomplete`; snapshots de logs antes/depois iguais. Sem registro de execução fornecido, o relatório manteve os logs como não associados e não lhes atribuiu causalidade. O teste não iniciou Minecraft nem simulou um crash real.

Análise acionada no launcher com instância real: **CONCLUÍDO** no binário debug/WebView2, via `scripts/doctor-native-smoke.mjs` (porta CDP local de teste). O script consultou o inventário real (8 instâncias instaladas), abriu **Editar → Diagnóstico → Analisar instância** e verificou o JSON efetivamente renderizado: perfil salvo correspondente, 2 logs, nenhum registro anterior de execução, resultado `incomplete`, 0 causas atribuídas e relatório sanitizado. O teste não iniciou Minecraft nem alterou arquivos da instância. A navegação existente pode atualizar o cache local de imagens do editor.

A suíte DOM `node apps/desktop/src/features/doctor/DoctorView.smoke.mjs` passou em 13 verificações **sintéticas**: carregamento, troca de instância/Java durante análise, invalidação por execução, ausência de logs, falha com erro genérico, encerramento normal/cancelamento, atalho para o ID correto, evidências, proveniência e cópia/exportação usando somente o JSON sanitizado. A dependência `linkedom` já está declarada; neste ambiente foi disponibilizada em instalação temporária isolada porque o cache compartilhado de `node_modules` estava incompleto.

Falha real de Minecraft observada de ponta a ponta após esta mudança, cópia para clipboard do sistema e arquivo baixado pelo WebView: **PRECISA DE TESTE**. A renderização foi verificada pelo DOM real, sem captura de tela ou homologação visual por imagem. A tarefa não inclui homologação geral das versões existentes nem validação online de Gemini/TTS/microfone.

## Arquivos principais

- `apps/desktop/src-tauri/src/engine/doctor.rs`: regras, associação, limites de leitura e relatório controlado.
- `apps/desktop/src-tauri/src/doctor_runs.rs`, `commands.rs` e `main.rs`: registro local, ciclo de vida observado, comandos e integração Tauri.
- `apps/desktop/src/features/doctor/` e `App.tsx`: Diagnóstico, estados de UI, atalho de falha e cópia/exportação.
- `apps/desktop/src-tauri/tests/doctor_qa.rs` e fixtures: QA adversarial independente.
- [ADR-021](engineering/ADR/ADR-021-aurora-doctor-local.md): decisão arquitetural e consequências.

## Limitações e continuidade

O Doctor não interpreta toda falha possível nem atribui causalidade a um mod citado apenas em stack trace. Sem dados suficientes, explica a ausência de evidência. A análise de logs históricos não comprova a causa de uma nova tentativa. O relatório resumido sanitizado omite os trechos livres dos logs e não substitui uma investigação manual quando as regras não concluem.

O próximo incremento é uma proposta de recuperação estreita, revisável e reversível para um problema confirmado, com confirmação de mutações e rollback. Esse incremento não foi implementado. Comportamento e fronteiras: [Aurora Doctor](aurora-doctor.md).
