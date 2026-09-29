# Aurora Connect V2 — revisão documental e limpeza

Data: **29/09/2026**. Entrega documental; nenhum código Connect, serviço, faturamento, deploy ou instalador foi produzido. Estado do Connect no inventário limitado: **NÃO IMPLEMENTADO**. Especificação/decisão: **HIPÓTESE** até execução e aceite dos gates. A alpha.14 existente foi preservada como base, não criada por esta tarefa.

## Ordem dos sete entregáveis

1. **Diagnóstico e conflitos corrigidos:** este documento, seções abaixo.
2. **Roadmap completo revisado:** [MASTER_ROADMAP.md](engineering/MASTER_ROADMAP.md), editado na cópia de trabalho, sem duplicar o roadmap num novo arquivo.
3. **Especificação Aurora Connect:** [AURORA_CONNECT_SPEC.md](engineering/AURORA_CONNECT_SPEC.md).
4. **Proposta de ADR sem número atribuído:** [PROPOSAL-aurora-connect.md](engineering/ADR/PROPOSAL-aurora-connect.md).
5. **Backlog incremental e matriz de compatibilidade:** [AUR-R4-CONNECT-BACKLOG.md](engineering/CODEX_TASKS/AUR-R4-CONNECT-BACKLOG.md), oito incrementos, casos de teste e linhas candidatas por mod/versão/loader/hospedagem/rede.
6. **Dependências e zero-spend:** [AURORA_CONNECT_DEPENDENCIES.md](engineering/AURORA_CONNECT_DEPENDENCIES.md); rastreabilidade e registro de alterações neste documento.
7. **Prompt completo para implementação pelo Codex:** [AUR-R4-CONNECT-V2.md](engineering/CODEX_PROMPTS/AUR-R4-CONNECT-V2.md). Ler/copiar a partir de “Tarefa”; ele repete escopo, restrições, auditoria, testes e formato de relatório.

## Diagnóstico e autoridade das fontes

| Categoria | Autoridade e material | Consequência nesta revisão |
| --- | --- | --- |
| Original canônico | `docs/engineering/MASTER_ROADMAP.md`, integralmente lido, inicialmente em `dc2ec5a` | Preservados R0–R5, R1.5/R2.5, prioridades, Waves 0–6+, AUR-R0/AUR-R1 e gates |
| Requisitos novos | Prompt Aurora Connect V2 e correções do responsável: alpha.14, preservar entregas, limpar documentação | Prevalecem sobre política/escopo anteriores conflitantes; não autorizam código nesta execução |
| Evidência local de base | Git/manifests/matriz, tarefa AUR-1167 e manifesto alpha.14 | Alpha.14 comprovada em worktree posterior; base avançada sem descartar recursos |
| Código observado | Inventário delimitado descrito em INV-01..06 no backlog | Confirma stack e fronteiras; não constitui auditoria integral nem ausência global de transporte em toda branch |
| Testes anteriores | Registros AUR-1166/AUR-1167 e matriz histórica | Resultados atribuídos às entregas originais; nenhum build/runtime repetido nesta revisão |
| Pesquisa oficial | URLs/data nos documentos técnicos | Capacidade de biblioteca/mod e política publicada, nunca homologação Aurora |
| Proposta técnica | Iroh primeiro, contratos v1, limites de laboratório, adaptadores e fases R4 | Sujeita a pin, código/teste e aceite; sem API/arquivos implementados inventados |
| Decisão pendente | Relay produção, versão final, modo misto, configuração/mod API, MTU, rollout | Mantida explícita; não convertida em conclusão ou aprovação |

Não foi disponibilizado como anexo nem localizado `MASTER_ROADMAP(1).md` nas árvores pesquisadas de `AuroraLauncher` e `TESTE-main`. A versão canônica efetivamente disponível foi usada; não foi feita busca indiscriminada em documentos pessoais fora desses escopos. A cópia `aurora-skins-runtime` é histórica e não serviu de base.

### Correção da base alpha.14

Inicialmente `C:/Users/usuario/Desktop/AuroraLauncher` estava limpo em `dc2ec5ac36a88f54b1130bac4789f8ecd56641d2`, manifests alpha.13. Após a informação do responsável, a busca nas worktrees identificou a entrega concluída em `C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-1167-doctor-lab`, HEAD **`6a107b87adc4882bd464909e22650185f862ea5e`**, com manifests **0.1.1-alpha.14** e manifesto de instalador. A revisão passou a essa base por fast-forward, preservando todos os ancestrais; nenhum arquivo de outra worktree foi editado.

O manifesto registra origem `89211c735093d2a310eb5ef60eb2c10f33a4c61e` e instalador de 9.129.361 bytes, SHA-256 `06ec491a62b050d2551d55ed5fba025e5272b3bdd9e40457a4520e5d815b0cfc`. Esses dados foram lidos do manifesto e da tarefa, não reempacotados nem usados como prova de runtime. Consulte [AUR-1167](task-aur-1167-doctor-lab.md) e [manifesto alpha.14](<../releases/Aurora Smart Launcher_0.1.1-alpha.14_x64-setup.exe.manifest.json>).

Preservados: recuperação manual/CAS restrito de `be3a1c4`, Doctor e histórico, Gemini/Tools confirmadas, sessões independentes, cópias de diagnóstico e aplicação seletiva com desfazer, grade/cache/multiplayer do Skins 0.3.1. A matriz continua **Core 1.1.5 + Skins 0.3.1, 18 JARs nos nove pares existentes**. Os gates runtime permanecem; não se afirma que todos os recursos alpha.14 foram homologados.

### Conflitos corrigidos

| Antes/risco de interpretação | Regra V2 e justificativa |
| --- | --- |
| Veto absoluto a relay no prompt anterior citado pelo responsável | Substituído por direto preferencial + relay gratuito verificado; cobrança obrigatória/automática continua proibida. As frases literais do veto não foram encontradas como regras nos documentos pesquisados desta base; não inventamos sua remoção de um arquivo ausente |
| Falha obrigatória sem direto | Falha só quando não existir caminho direto **nem relay permitido disponível**; testes aceitam relay elegível e verificam quota/indisponibilidade |
| “Voz futura” confundida com fora de escopo | SVC e Plasmo são requisitos R4, com transporte **e endereçamento**; criar voz própria permanece fora |
| PoC apenas TCP como aprovação multiplayer | UDP bidirecional e limites de DATAGRAM são explícitos; TCP não aprova voz/relay/R4 |
| e4mc/World Host como sequência única aprovada | Comparação preservada; Iroh avaliado primeiro, Quinn alternativa, sidecar Go só com vantagem concreta; nenhum escolhido para produção antecipadamente |
| Biblioteca livre = relay gratuito de produção | Separados código/serviço, teste/distribuição, quotas/termos e custo; nenhum relay de produção aprovado |
| Alpha.13 tratada como versão atual | Base reconciliada alpha.14; recuperação, Doctor/lab e CAS existente não voltam ao backlog como ausentes |
| CAS/Doctor/Tools inteiramente inexistentes | Corrigido para implementação restrita e homologação pendente; GC e snapshots automáticos gerais ainda pendentes |
| HUD começaria após harness | Passagem obsoleta removida do plano paralelo; ADR-018 preserva apenas Assistente desktop/Core/Skins |
| Skins sem mod servidor como resultado existente | Resumo de produto alinhado à ADR-022/Skins 0.3.1; não prometer propagação universal sem mod servidor |

O retransmissor de aparência da ADR-022 não é relay de transporte Connect. Não removê-lo por coincidência do termo “relay”. Da mesma forma, a voz/TTS do Assistente desktop não é o UDP dos mods de voz.

### Leituras efetivas e limites

Leitura integral do roadmap e documentos normativos/de domínio: `AGENTS.md`, `SECURITY.md`, `docs/ROADMAP.md`, índice de engenharia, resumo executivo, revisão de arquitetura, grafo, riscos, decisões, estratégia de testes/releases e planos paralelo/integração; matriz de compatibilidade; pesquisa externa histórica; `docs/core-ipc.md`, recuperação, diagnóstico/lab e task AUR-1167. Foram lidas ADR-006/007/010/017/018/025/027/028 e, pela revisão auxiliar de domínio, ADR-021/024 e documentos Doctor/AI Tools/tarefas relacionadas. Fontes antigas foram mantidas como histórico, não adotadas como estado atual por conterem uma referência.

Manifests reais, matriz ativa, contrato cumulativo Skins, entradas pertinentes de lockfiles e módulos/trechos estão discriminados em **INV-01..06** do backlog. A busca de transporte em Rust/desktop/Core não encontrou ponte Connect no escopo lido; isso não prova ausência em branches não inspecionadas. O trecho de launch examinado usa identidade offline, portanto o futuro auto-join online exige auditoria real e não pode reduzir `online-mode`.

Não foram executados testes Minecraft, áudio, NAT, relay, microfone, login, CI, rules em produção, serviços pagos ou leitura de segredos. Termos Iroh tiveram limitação de acesso: extrato indexado disponível, texto integral não lido; aprovação de uso continua pendente. Documentação latest/branches móveis não fixa versão nem obriga atualizar lockfile. Pesquisa SVC 26.3 não amplia matriz Aurora; Plasmo 2.1.17 não é mod já homologado.

## Rastreabilidade: requisito → documento → tarefa → evidência

“Exigida” abaixo significa teste futuro, **NÃO VERIFICADO** nesta revisão. IDs T-* são casos planejados, não comandos de teste existentes. Origem 1–11 refere-se às seções do pedido V2.

| Requisito/origem | Documento/seção | Tarefa | Evidência atual ou exigida |
| --- | --- | --- | --- |
| Autoridade, preservar roadmap/gates/IDs/ownership (1) | Este diagnóstico; roadmap; plano paralelo | 001, 008 | Base/manifests e diff documental; T-INV-01, T-REG-01/02 |
| Alpha.14 e não perder entregas (correções) | Diagnóstico; roadmap Waves 4/5; resumo/AGENTS | 001 | AUR-1167 e manifesto lidos; INV-06; sem nova homologação |
| Público/privado/dedicado/LAN/capacidades distintas (2) | Spec §§1–3 | 005 | T-MC-01..05; host disponível e autorização |
| Direto/relay gratuito, teste vs produção, zero-spend (3) | Spec §6; matriz externa; D-003; ADR | 001, 004, 008 | URLs/data e lacunas registradas; T-COST-01..03, T-NET-05/06 |
| Camadas e pequeno registro autorizado (4) | Spec §§2–3 | 001–003 | T-SEC-02/03; sem destino arbitrário/proxy aberto |
| TCP íntegro, streams, backpressure e cancelamento (4.1) | Spec §4 | 002 | T-TCP-01..05, T-LIFE-01 |
| UDP retorno/isolamento, MTU/framing/filas (4.2) | Spec §5 | 003 | T-UDP-01..09 e T-MIX-01..03; cálculo de limite explícito |
| Serviços futuros só por extensão aprovada (4.3) | Spec §3; matriz candidata C-EXT | 001, 008 | T-SEC-02; revisão por adaptador |
| SVC endereço/API/config/versão (5) | Spec §7; dependências DEP-SVC | 006 | T-VOICE-01..07; config efetiva/artefatos ainda pendentes |
| Plasmo endereço/API/config/versão (5) | Spec §7; DEP-PLASMO | 007 | Mesmos testes em execução separada; API 2.1.17 pesquisada |
| Misto/diretos, conflitos, instâncias e snapshot (5) | Spec §7; matriz C-SVC-M/C-PV-M | 006, 007 | T-VOICE-05/07, T-CONFIG-01..04 |
| Stack, Iroh/Quinn/sidecar, plataformas/pin (6) | ADR; dependências; INV-02 | 001, 004 | Manifestos reais; fontes técnicas; composição ainda não validada |
| NAT/IPv4/IPv6/UDP bloqueado/portmap (6) | Spec §6 | 004 | T-NET-01..08; consentimento/limpeza e caminho observado |
| e4mc/World Host e relay próprio adiado (6) | ADR e DEP-E4MC/WORLDHOST | 001, 004 | Fontes oficiais atualizadas, sem presumir UDP/voz |
| Identidade/key/session/invite, E2E/replay/limits (7) | Spec §8 | 001–005 | T-SEC-01..05, T-LIFE-02 |
| Sinalização/rules/retention/secrets/IP (7) | Spec §8; DEP-FIREBASE-SIGNAL/WORKER | 005 | T-SIGNAL-01/02; planos reais não verificados, sem billing |
| UX estados reais e falha de voz isolada (8) | Spec §9 | 005–007 | T-UX-01..04, T-MIX-03 |
| Incrementos com risco/aceite/rollback/evidência (9) | Backlog tarefas 001–008 | Todas | Contratos completos, sem prazo inventado |
| Testes redes/mods/métricas e liberação (9) | Spec §10; backlog §§4–6 | 004, 006–008 | T-NET-07, T-VOICE, T-MIX, T-MAN, T-REG |
| Sete entregas e prompt autossuficiente (10) | Índice acima; prompt completo | 001 | Arquivos reais e validação de links/IDs; implementação futura |
| URLs oficiais/versões/data/limites (11) | Spec §11; matriz externa; ADR | 001, 004 | Consulta 29/09/2026, latest separado de pin |
| Limpeza sem apagar trabalho útil (correção) | Registro abaixo | Revisão documental | Diff restrito a Markdown, histórico e artefatos preservados |

## Registro de alterações e limpeza

Branch de entrega: `codex/aur-r4-connect-v2-review`. Cópia revisada: `C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect-v2-review`. O mecanismo gerenciado de worktree retornou `Not a git repository` para o contexto deste chat; foi usada worktree Git do repositório Aurora correto. Uma revisão semelhante em outro chat estava ativa: seus arquivos foram preservados, sem mensagens/comandos para interferir nela e sem integração silenciosa.

Não é uma limpeza por idade nem por promessa de que um arquivo “nunca” será usado. Nenhum documento histórico completo foi demonstrado inútil/duplicado byte a byte a ponto de justificar exclusão. A limpeza foi feita **corrigindo e consolidando orientações atuais**, removendo trechos operacionais obsoletos e apontando histórico para fontes canônicas. Releases, ADRs históricos, evidências, módulos, lockfiles e código ficaram intactos. Não foram criadas cópias numeradas de roadmaps ou documentos de implementação redundantes.

| Arquivo existente atualizado | Alteração e motivo |
| --- | --- |
| `AGENTS.md` | Estado alpha.14 e distinção CAS/Doctor/Tools existentes; referência Connect V2 e prevenção de base antiga |
| `docs/ROADMAP.md` | Resumo alinhado à alpha.14, Skins multiplayer real e link ao roadmap completo |
| `docs/aurora-doctor.md` | Corrigido CAS genericamente ausente; preservada separação diagnóstico/ações |
| `docs/engineering/MASTER_ROADMAP.md` | Texto completo preservado estruturalmente; Waves 4/5 com estado atual; R4 TCP/UDP/voz/relay e gates complementares |
| `docs/engineering/EXECUTIVE_SUMMARY.md` | Estados antigos reconciliados com implementações atuais e evidência limitada |
| `docs/engineering/ARCHITECTURE_REVIEW.md` | Removidas contagens de linhas desatualizadas; executores reais/limites e camadas Connect |
| `docs/engineering/DEPENDENCY_GRAPH.md` | Diferenciadas PoC isolada, gates de produto e sync por manifesto; recovery existente |
| `docs/engineering/DECISIONS_REQUIRED.md` | D-001/D-002 preservadas; D-003 atualizada por requisito explícito V2 |
| `docs/engineering/RISK_REGISTER.md` | IDs/histórico preservados; R-013 custos/relay, CAS/Tools e mutações atualizados |
| `docs/engineering/RELEASE_STRATEGY.md` | Versão/proveniência alpha.14, assinatura/runtime separados, zero-spend e fluxo oficial |
| `docs/engineering/CODEX_PARALLEL_PLAN.md` | Baseline antiga histórica, retirada da ordem obsoleta de HUD, ownership Connect |
| `docs/engineering/TEST_STRATEGY.md` | Voz de mods diferenciada do Assistente; referências a testes R4/relay permitido |
| `docs/engineering/ADR/ADR-010-multiplayer.md` | Decisão antiga explicitamente histórica/revisada pelo V2, sem apagar contexto |
| Índices de engenharia, ADR, tarefas e prompts | Caminho de leitura dos sete entregáveis e orientação de uso atual |
| `docs/engineering/COMPATIBILITY_MATRIX.md` | Registro documental Connect sem homologação e link à matriz candidata separada |

Novos arquivos necessários, exatamente seis: este task file, especificação, matriz de dependências, ADR proposta sem número, backlog com matriz/testes e prompt completo (caminhos no índice). Cada um tem função distinta; não há arquivo novo de roadmap completo duplicado.

## Verificação desta entrega

Revisão auxiliar independente conferiu requisitos/segurança e APIs candidatas. Foram corrigidos o cálculo do orçamento UDP (unidades, header e prevenção de underflow), caminhos abreviados no prompt/backlog, a distinção gráfica entre PoC e gate de liberação, estado parcial de modularização e a descrição de testes Firestore históricos. Conferência documental: **24 arquivos Markdown**, **187 links locais resolvidos**, **oito IDs originais AUR-R0/R1 preservados**, **oito novos IDs R4 e 62 casos T-* únicos**, `git diff --check` sem erro. A comparação das árvores `apps`, `scripts`, `firebase`, `functions`, `releases`, `shared` e `.github` contra alpha.14 não apresentou alteração. Nenhum desses checks é teste de rede ou homologação.

Não foram removidos arquivos inteiros nem executados builds/runtime porque as alterações são documentais. Não houve commit/push/publicação de código Connect, infraestrutura ou alteração de custo. Para desfazer esta revisão, reverter somente seu commit documental em branch apropriada; não reverter a base alpha.14 nem apagar os recursos anteriores.
