# Master Roadmap

Revisão documental Aurora Connect V2 — 29/09/2026. Fonte canônica lida integralmente: este arquivo no commit `dc2ec5ac36a88f54b1130bac4789f8ecd56641d2`. O nome `MASTER_ROADMAP(1).md` não foi localizado nos diretórios pesquisados; não se usou a base histórica `aurora-skins-runtime`. [Diagnóstico, autoridade e limites](AURORA_CONNECT_REVIEW.md).

As fases, prioridades, IDs e gates anteriores permanecem. Conteúdo não alterado descreve o plano/estado registrado na sua entrega original, não uma nova homologação. A revisão V2 especifica R4; não promove outras fases nem autoriza distribuição. Stack verificada em manifests: React/TypeScript, Tauri v2 e Rust; launcher `0.1.1-alpha.13`, Core `1.1.5`, Skins `0.3.1`. A matriz vigente é `scripts/active-mod-matrix.json`, não uma inferência sobre versões futuras.

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
| R4 | P2 | multiplayer/social — Aurora Connect (nome de trabalho) | PoCs TCP/UDP medidos, convites autorizados, voz homologada por mod e sync por manifesto |
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

Snapshots e restauração devem conservar configurações do usuário e detectar divergências/edições concorrentes. R4 reutiliza esses contratos; não abre uma segunda via de instalação ou redistribuição de mods.

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
- R4: avaliar primeiro a adequação de Iroh ao transporte Rust, comparando Quinn com descoberta/NAT traversal compatíveis; Pion ICE + quic-go só justifica processo auxiliar mediante vantagem demonstrada. [ADR proposta, sem número reservado](ADR/PROPOSED-aurora-connect-transport.md).
- Reavaliar PoC e4mc/World Host por integração, TCP/UDP, voz, licença e custo, sem rejeitar uma opção só por usar relay. Depois, convites/presença e sincronização consentida por manifesto; relay próprio obrigatório permanece adiado. Nenhuma dessas opções está homologada nesta revisão.
- Aurora Connect é a camada candidata de conectividade entre agentes autorizados. Minecraft permanece autoritativo, o anfitrião precisa continuar disponível e hospedagem oficial 24 horas não entra no escopo. Conexão pública normal, servidor privado cooperativo, servidor dedicado local e mundo aberto para LAN são modos distintos.
- Preferir caminho direto; permitir relay gratuito somente com termos, integração, quotas e uso pretendido verificados. Proibidos gasto obrigatório/automático, créditos temporários como base do MVP e provedor com excedente cobrável sem bloqueio efetivo comprovado. Nenhum faturamento é habilitado. Ausência de relay aprovado mantém o modo direto com limite explícito, sem fallback pago silencioso.
- TCP: ponte de bytes bidirecional, confiável, ordenada e isolada por conexão. UDP: ponte de datagramas bidirecional, com fronteiras preservadas e retorno isolado por participante/sessão/serviço/associação. Áudio não usa, por padrão, o stream confiável ordenado do jogo.
- Simple Voice Chat e Plasmo Voice integram o backlog de R4, homologados separadamente. Encaminhamento UDP e endereçamento anunciado/local são problemas distintos. Não criar voz própria, recuperar Companion/HUD nem presumir compatibilidade de todos os mods/versões.

### Incrementos de R4 — sem reabrir capacidades já comprovadas

| ID novo | Incremento | Saída e dependência principal |
| --- | --- | --- |
| AUR-R4-001 | inventário, decisão, ameaças, custo e contratos | evidência real, versão candidata e plano mínimo; nenhum gate anterior presumido verde |
| AUR-R4-002 | ponte TCP autenticada | integridade, cancelamento, isolamento e auth negativa; fronteiras concretas para UDP |
| AUR-R4-003 | ponte UDP e coexistência | retorno correto, MTU, perda/reordenação, limites e TCP+UDP simultâneos |
| AUR-R4-004 | redes distintas e caminhos | direto/relay autorizado observáveis; falhas/quota/UDP externo bloqueado reproduzidos |
| AUR-R4-005 | Minecraft, convites e UI real | autorização, expiração/revogação e estados baseados em evidência; abrir LAN/auto-join são capacidades próprias |
| AUR-R4-006 | voz por adaptador | Simple Voice Chat e Plasmo Voice, cada versão/loader/modo/endereço testados isoladamente |
| AUR-R4-007 | robustez, manifesto e liberação gradual | redes reais, compatibilidade, documentação e gates anteriores satisfeitos antes da exposição |

O [backlog e a matriz de compatibilidade](AURORA_CONNECT_BACKLOG.md) definem objetivo, ownership, dependências, risco, testes, aceite, rollback, estado e evidências. A ordem pode mudar conforme o bloqueio comprovado; TCP existente não deve ser refeito. Uma PoC local isolada é experimento e não libera fase, UI pública, sync ou distribuição. Recursos sem validação ficam experimentais e desativados para usuários por padrão.

Contratos detalhados: [especificação](AURORA_CONNECT_SPEC.md), [dependências e zero-spend](AURORA_CONNECT_DEPENDENCIES.md), [rastreabilidade e alterações](AURORA_CONNECT_TRACEABILITY.md) e [prompt autossuficiente do próximo incremento](CODEX_PROMPTS/AUR-R4-connect-v2.md). Nenhuma implementação faz parte desta entrega documental.

### Edição paralela preservada

Permanecem o [plano paralelo](CODEX_PARALLEL_PLAN.md), o [plano de integração](CODEX_INTEGRATION_PLAN.md) e os owners exclusivos de `App.tsx`, `commands.rs` e `apps/edge-proxy/src/index.ts`. R4 delimita arquivos antes de delegar; não mistura refatoração ampla com transporte nem toma ownership das waves R0/R1. Dependências, protocolo e lockfile exigem integração coordenada. Homologação Core/Skins continua sendo a dos nove pares existentes; nenhum novo par é criado por um adaptador de rede.

## Gates de fase

- R0: regras negativas/positivas verdes, perfis existentes auditados, todos os checks em PR, licença decidida, release checklist e nenhum artefato mutável novo.
- R1: 9/9 evidências runtime Core/Skins, Assistente desktop funcional, bundle dividido e erros de rede recuperáveis.
- R1.5: atualização reversível sem perder configs.
- R2: restore bit a bit e GC sem apagar referência viva.
- R2.5: nenhum texto não confiável aciona mutação diretamente.
- R4: ameaça/custo/latência medidos; sem depender de gasto obrigatório. Acrescentam-se TCP e UDP autenticados/isolados, destino aprovado, voz homologada por mod/versão/loader/modo anunciado, expiração/revogação de convites, caminho direto/relay observável e matriz de redes com falhas/quota/retorno/MTU. PoC TCP não aprova UDP, voz, relay nem R4 inteira. Testes externos somente em ambiente autorizado, sem custo. Sync depende de manifesto e updater reversível; a liberação preserva todos os gates anteriores.
- R3/R5: esta revisão não reduz contratos de manifesto/Creator, assinatura, privacidade, SLO e budgets zero-spend existentes. R4 não transforma orçamento local ou alerta em bloqueio de cobrança do provedor.
