# Master Roadmap

Revisão documental de 29/09/2026: **Aurora Connect V2**, sem implementação nesta tarefa. Roadmap canônico lido integralmente; `MASTER_ROADMAP(1).md` não foi fornecido/localizado nos dois workspaces pesquisados. Base reconciliada após a correção do responsável: launcher **0.1.1-alpha.14**, HEAD `6a107b87adc4882bd464909e22650185f862ea5e`, Core **1.1.5**, Skins **0.3.1**. A base inicial alpha.13 não incluía as últimas entregas; a revisão preserva integralmente recuperação manual e cópias de diagnóstico da alpha.14. [Diagnóstico e alterações](../task-aur-r4-connect-v2-review.md).

As fases, prioridades, IDs, ordem de waves e gates anteriores permanecem. Estados citados de entregas anteriores são registros documentais, não testes repetidos nesta revisão. Propostas Connect não são funcionalidades prontas. Referência de compatibilidade: [matriz](COMPATIBILITY_MATRIX.md) e `scripts/active-mod-matrix.json`, sem expansão dos nove pares.

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
| R4 | P2 | multiplayer/social — Aurora Connect | PoCs TCP/UDP medidos, convites autorizados, voz homologada por mod e sync por manifesto |
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
- Preservar configurações e detectar edições concorrentes ao restaurar. Snapshots/journals do updater existente não são substituídos automaticamente pelo CAS da recuperação. Pontos automáticos gerais antes de updates continuam **NÃO IMPLEMENTADO** na alpha.14.

## Wave 4 — recovery local

- Parser determinístico com fixtures reais redigidas; Doctor local e histórico já implementados, com homologação runtime ainda pendente.
- Doctor mantém análise somente leitura e relatório de confiança. Orientação Gemini, revisão de ações e acesso à recuperação são fluxos separados com autorização própria; diagnóstico não executa restauração.
- Recuperação manual com prévia, confirmação, proteção anterior/desfazer, CAS SHA-256 + SQLite e identidade da instância: **IMPLEMENTADO MAS NÃO HOMOLOGADO**, [ADR-027](ADR/ADR-027-manual-instance-recovery.md). Escopo limitado; mundos, credenciais e módulos Aurora administrados não são restaurados.
- Observações verificadas do Doctor e cópias independentes de diagnóstico com aplicação seletiva e desfazer: **IMPLEMENTADO MAS NÃO HOMOLOGADO**, [ADR-028](ADR/ADR-028-doctor-recovery-lab.md), alpha.14. Preservar estes recursos; cópia de teste não é sandbox de segurança.
- Retenção, unificação dos executores e GC seguro continuam pendentes. A implementação atual conserva blobs sem referência; futuro GC começa em dry-run e não pode apagar referências vivas.
- Safe Mode apenas após restauração testada.

## Wave 5 — Tools

- Nível 0 leitura e Nível 1 sugestão.
- Red-team de logs, nomes, manifests e páginas maliciosas.
- Nível 2 somente com plano, diff, snapshot, confirmação e auditoria.
- O broker restrito de ações locais, revisão, histórico e restauração já foi entregue, **IMPLEMENTADO MAS NÃO HOMOLOGADO** ([ADR-025](ADR/ADR-025-confirmed-local-ai-tools.md)); não refazer como se inexistisse. Novas ações continuam sujeitas aos mesmos gates, sem shell genérico ou mutação direta por texto não confiável.

## Wave 6+ — ecossistema

- Manifesto v3, server-pack e gerador de recipes/datapack com export KubeJS opcional.
- Master Panel mínimo após contratos backend.
- Aurora Connect é o nome de trabalho de R4. Avaliar primeiro Iroh e comparar Quinn com descoberta/traversal compatíveis; Pion ICE + quic-go em processo auxiliar só diante de vantagem concreta. Preservar e4mc/World Host como comparativos medidos, sem rejeitá-los por relay nem presumir UDP de voz. Infraestrutura própria obrigatória permanece adiada.
- Priorizar direto, **permitir relay gratuito verificado para o uso pretendido**, sem gasto obrigatório/automático. Gratuidade de desenvolvimento não autoriza distribuição; nenhum relay de produção está aprovado nesta revisão. Sem direto nem relay permitido, falha clara; nunca troca paga silenciosa.
- Escopo inclui servidor público normal, host privado autorizado, dedicado local e mundo LAN, distinguindo transporte, descoberta, abertura LAN, auto-join e sync. O host permanece disponível; Minecraft continua autoritativo.
- Transporte de bytes TCP por streams confiáveis e serviços UDP por datagramas autenticados, com retorno isolado por participante/sessão/serviço, limites e controle de MTU. Somente destinos aprovados pelo host; sem VPN/driver/admin permanente no fluxo final, proxy aberto ou acesso geral à LAN.
- Integrar separadamente **Simple Voice Chat** e **Plasmo Voice**, preservando autenticação/criptografia, com endereçamento real de cada convidado, testes de dedicado/LAN quando suportados e modo misto apenas homologado. Isso é transporte de mods existentes, não criação de voz própria nem retorno do Companion.
- Convites/presença usam identidade, expiração, revogação e proteção contra replay. Sync por manifesto depende de R3 e updater reversível: origem/licença/hash, plano/diff, consentimento e rollback. Firestore não transporta jogo, áudio nem arquivos de modpack.

### R4 — incrementos identificáveis

Todos os IDs abaixo são novos propostos, sem colisão encontrada nesta base. Detalhes completos de objetivo, arquivos, dependências, risco, testes, aceite, rollback e evidências estão no [backlog](CODEX_TASKS/AUR-R4-CONNECT-BACKLOG.md). Ordem ajustável ao código real; não refazer implementação comprovada.

| ID | Incremento | Evidência que habilita o próximo passo |
| --- | --- | --- |
| AUR-R4-001 | Inventário, ADR, ameaça, custo e serviços | Base reconciliada, contratos e opção de PoC justificados |
| AUR-R4-002 | Ponte TCP autenticada | Integridade, isolamento, backpressure, cancelamento e recusas |
| AUR-R4-003 | Ponte UDP e TCP+UDP simultâneos | Datagramas íntegros, retorno correto, MTU/filas e limpeza |
| AUR-R4-004 | Redes distintas e relay gratuito elegível | Caminho observado, termos/quotas e falhas reproduzíveis |
| AUR-R4-005 | Minecraft, convites e UX real | Estado backend, permissões, servidor e jogador diferenciados |
| AUR-R4-006 | Simple Voice Chat | Versão/loader/endereço/modo testados individualmente |
| AUR-R4-007 | Plasmo Voice | Homologação independente, API/endereço e segurança preservados |
| AUR-R4-008 | Robustez, manifesto, matriz e liberação gradual | Redes reais, custos, compatibilidade e gates do produto |

[Especificação detalhada](AURORA_CONNECT_SPEC.md), [ADR proposta sem número atribuído](ADR/PROPOSAL-aurora-connect.md), [dependências zero-spend](AURORA_CONNECT_DEPENDENCIES.md), [prompt executável](CODEX_PROMPTS/AUR-R4-CONNECT-V2.md). A pesquisa/PoC isolada autorizada nesta revisão não libera UI ou distribuição antes dos gates; recursos experimentais ficam desligados ao usuário. TCP aprovado não aprova UDP, voz, relay ou R4 inteira.

## Gates de fase

- R0: regras negativas/positivas verdes, perfis existentes auditados, todos os checks em PR, licença decidida, release checklist e nenhum artefato mutável novo.
- R1: 9/9 evidências runtime Core/Skins, Assistente desktop funcional, bundle dividido e erros de rede recuperáveis.
- R1.5: atualização reversível sem perder configs.
- R2: restore bit a bit e GC sem apagar referência viva.
- R2.5: nenhum texto não confiável aciona mutação diretamente.
- R4: ameaça/custo/latência medidos; sem depender de gasto obrigatório.

Complementos V2 ao gate R4: autorização criptográfica por sessão/serviço; TCP e UDP validados separadamente e juntos; anúncio dos mods comprovado; caminho direto/relay observado; relay habilitado somente conforme termos e zero-spend efetivos; métricas BR–BR/BR–EUA em ambientes autorizados; falhas/revogação/rollback testados; matriz sem extrapolar laboratório para NAT real. Sem evidência, registrar **NÃO VERIFICADO** ou **PRECISA DE TESTE**.

R3 mantém manifesto/Creator determinísticos; R5 permanece contínuo com assinatura/updater, privacidade, SLO e budgets zero-spend. Nenhuma PoC revoga R0, a homologação R1 9/9, a proteção de configurações R1.5, restore/GC R2 ou confirmação/auditoria R2.5. Monólitos e manifestos mantêm ownership exclusivo e integração/QA conforme o [plano paralelo](CODEX_PARALLEL_PLAN.md).
