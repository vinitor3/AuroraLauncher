# Master Roadmap

## Escala

P0 bloqueia segurança, rastreabilidade ou promessa principal. P1 entrega valor necessário. P2 é expansão. P3 é futuro. Prazos não são compromissos.

| Fase | Prioridade | Objetivo | Saída verificável |
| --- | --- | --- | --- |
| R0 | P0 | baseline, CI, licença, release | checks obrigatórios, versão única, hashes e documentos coerentes |
| R1 | P0 | fechar desktop/Core/Skins | UI modular, HTTP cancelável, Assistente desktop e 9/9 runtimes Core/Skins |
| R1.5 | P1 | atualizar conteúdo | plano compatível, snapshot mínimo e rollback de mod |
| R2 | P1 | diagnóstico/recuperação | fixtures, parser, Doctor, CAS e restore bit a bit |
| R2.5 | P1 | Tools seguras | níveis 0–2, schemas, capabilities, diff e auditoria |
| R3 | P2 | manifesto/creator | schema v3, import/export e Creator MVP determinístico |
| R4 | P2 | multiplayer/social | PoC medido, convites autorizados e sync por manifesto |
| R5 | P1 contínuo | produção | updater assinado, privacidade, SLO e budgets zero-spend |

## Wave 0 — gate de distribuição pública (agora)

| Tarefa | Impacto | Esforço | Risco | Paralelo |
| --- | --- | --- | --- | --- |
| AUR-R0-001 regras Firestore + auditoria | crítico | médio | alto | sim, `firebase/**` exclusivo |
| AUR-R0-002 licença do monorepo | crítico | médio | médio | sim, arquivos legais exclusivos |
| AUR-R0-003 CI completa | alto | médio | baixo | sim, workflows exclusivos |
| AUR-R0-004 proveniência/release/docs | alto | médio | médio | sim, manifests/docs exclusivos |

## Wave 1 — baseline operacional

| Tarefa | Impacto | Esforço | Risco | Paralelo |
| --- | --- | --- | --- | --- |
| AUR-R1-001 timeout e limites HTTP/IPC | alto | médio | médio | sim, Rust exclusivo |
| AUR-R1-002 harness de evidência runtime | alto | médio | baixo | sim |
| AUR-R1-003 primeira decomposição React | alto | alto | médio | restrito, `App.tsx` exclusivo |
| AUR-R1-004 substituição transacional de arquivos | crítico | médio | alto | sim após delimitar `download.rs` |

## Wave 2 — Assistente desktop e Core/Skins

A wave antiga de Companion/HUD foi substituída pela [ADR-018](ADR/ADR-018-remove-companion-keep-desktop-assistant.md). O Assistente permanece no Aurora Launcher; o mod Companion e sua integração in-game foram retirados.

- Verificar painel, conversa, botão/atalho e voz desktop sem jogo instalado/em execução.
- Homologar IPC público, menu e aparência Core/Skins nos nove pares preservados.
- Verificar migração recuperável de JARs gerenciados antigos, sem afetar terceiros.
- Qualquer atualização geral de Loom/Gradle fica em mudança isolada.

## Wave 3 — matriz e updater mínimo

- Preservar os nove pares Core/Skins atuais; expansão multiversão é tarefa posterior.
- Inventário normalizado e resolução compatível de conteúdo — implementado, aguardando homologação runtime.
- Atualização Modrinth com staging, snapshot mínimo e rollback — implementada, aguardando homologação runtime.
- CurseForge somente onde a API/autor permite download; ação manual oficial nos demais casos — implementado para conteúdo individual. Modpack permanece detectável, sem aplicação automática.

## Wave 4 — recovery local

- Parser determinístico com fixtures reais redigidas.
- Doctor somente leitura e relatório de confiança.
- CAS local SHA-256 + SQLite, retenção e GC dry-run por padrão.
- Safe Mode apenas após restauração testada.

## Wave 5 — Tools

- Nível 0 leitura e Nível 1 sugestão.
- Red-team de logs, nomes, manifests e páginas maliciosas.
- Nível 2 somente com plano, diff, snapshot, confirmação e auditoria.

## Wave 6+ — ecossistema

- Manifesto v3, server-pack e gerador de recipes/datapack com export KubeJS opcional.
- Master Panel mínimo após contratos backend.
- PoC e4mc/World Host; depois convites/presença; relay próprio permanece adiado.

## Gates de fase

- R0: regras negativas/positivas verdes, perfis existentes auditados, todos os checks em PR, licença decidida, release checklist e nenhum artefato mutável novo.
- R1: 9/9 evidências runtime Core/Skins, Assistente desktop funcional, bundle dividido e erros de rede recuperáveis.
- R1.5: atualização reversível sem perder configs.
- R2: restore bit a bit e GC sem apagar referência viva.
- R2.5: nenhum texto não confiável aciona mutação diretamente.
- R4: ameaça/custo/latência medidos; sem depender de gasto obrigatório.
