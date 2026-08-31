# Relatório de pesquisa externa

Pesquisa encerrada em 2026-08-30. Foram priorizadas documentação oficial, repositórios dos autores, RFCs e termos dos provedores. “Referência” abaixo significa estudar contratos e comportamento; não autoriza copiar código, marca ou ativo.

## Conclusões executivas

1. O custo obrigatório inicial pode permanecer em zero usando GitHub Releases/Actions, limites rígidos dos free tiers e degradação local. “Grátis” não significa ilimitado ou com SLA.
2. Multiplayer direto é viável para parte dos pares; NAT simétrico/CGNAT exige relay em parte dos casos. Um relay Aurora universal criaria custo variável e operação 24×7.
3. O Companion precisa de core de estado/protocolo compartilhado e adaptadores por loader/versão. UI Swing externa não comprova integração in-game.
4. A distribuição pública exige autenticação Microsoft/Xbox/Minecraft e verificação de entitlement. Firebase autentica a conta Aurora, não a propriedade do jogo.
5. Conteúdo de terceiros deve ser baixado da origem autorizada, com licença e permissão registradas por arquivo. CurseForge requer revisão contratual e respeitar download manual/URL ausente.
6. Segurança transacional precede updater, CAS, Tools mutáveis e sincronização multiplayer.

## Launchers de referência

| Projeto | Evidência primária | Licença/limite | Aplicação no Aurora | Decisão |
| --- | --- | --- | --- | --- |
| Prism Launcher | [repositório](https://github.com/PrismLauncher/PrismLauncher), [copying](https://prismlauncher.org/wiki/overview/copying/) | código GPL-3.0-only; identidade visual com licença própria | instâncias, componentes, UX e distribuição como referência; qualquer cópia exige rastreabilidade e compatibilidade GPL | MANTER como referência; não copiar marca/keys |
| MultiMC | [repositório](https://github.com/MultiMC/Launcher) | Apache-2.0; marca e chaves/API não acompanham automaticamente | referência histórica para modelo de componentes/instâncias | REFERÊNCIA com atribuição por arquivo |
| Modrinth App | [repositório](https://github.com/modrinth/code), [API](https://docs.modrinth.com/api/) | app GPL-3.0; API tem contrato próprio | IDs estáveis, lookup por hash, filtros por versão/loader | INTEGRAR API oficial |
| ATLauncher | [repositório](https://github.com/ATLauncher/ATLauncher) | GPL-3.0; CDN, API keys e ativos não são presumidamente reutilizáveis | importação e ciclo de packs como estudo | REFERÊNCIA apenas |
| XMCL | [launcher](https://github.com/voxelum/x-minecraft-launcher) | MIT no repositório consultado | separação de módulos e serviços | REFERÊNCIA técnica |
| GDLauncher Carbon | [repositório](https://github.com/gorilla-devs/GDLauncher-Carbon) | Business Source License 1.1 até change date | arquitetura pode ser observada; código não deve entrar no Aurora GPL sem análise específica | NÃO REUTILIZAR agora |

### Decisão sobre Prism

O Aurora mantém engine própria. Prism é fonte de padrões e, se no futuro algum arquivo for adaptado, o PR deve registrar origem, commit, licença e alterações. Não importar branding, credenciais, traduções ou blocos estruturais sem inventário de proveniência.

## Minecraft, loaders e APIs de conteúdo

### Distribuição e identidade

As [Usage Guidelines](https://www.minecraft.net/en-us/usage-guidelines) e a [EULA](https://www.minecraft.net/en-us/eula) permitem mods originais dentro das condições, mas não autorizam redistribuir o jogo/conteúdo Mojang nem representar o launcher como oficial. O app, instalador, README e página de download devem declarar de forma proeminente que Aurora não é produto oficial nem associado à Mojang/Microsoft.

Antes de download/launch público, o fluxo precisa obter autorização Microsoft, tokens Xbox/Minecraft, verificar perfil/entitlement e manter tokens em armazenamento seguro. O `access_token` fictício atual é apenas scaffolding de alpha e não pode chegar a uma release pública. Modo offline só pode iniciar conteúdo previamente obtido legitimamente; não é bypass de propriedade.

### Modrinth

A [API oficial](https://docs.modrinth.com/api/) documenta identificação por User-Agent, IDs, hashes e limites. O Aurora deve preferir SHA-512, guardar `projectId/fileId`, versão do jogo, loader, ambiente e dependências, tratar `410`, `ETag`, `429` e backoff. O endpoint de [update por hash](https://docs.modrinth.com/api/operations/getlatestversionfromhash/) é a base do inventário normalizado. Os [termos](https://modrinth.com/legal/terms) proíbem usar dados/conteúdo do serviço para treinar ou melhorar IA: metadados Modrinth não devem ser enviados ao Gemini.

### CurseForge

A [API REST](https://docs.curseforge.com/rest-api/) exige chave. Os [termos de API para terceiros](https://support.curseforge.com/support/solutions/articles/9000207405-curseforge-3rd-party-api-terms-and-conditions), o processo de [solicitação de chave](https://support.curseforge.com/support/solutions/articles/9000208346-about-the-curseforge-api-and-how-to-apply-for-a-key) e o [distribution toggle](https://support.curseforge.com/support/solutions/articles/9000207877-project-distribution-toggle) precisam ser preservados no registro do projeto. Não inferir URL CDN, re-hospedar JAR ou contornar página manual quando `downloadUrl` estiver ausente. O cache atual em `localStorage` deve ser revisto contra o contrato aprovado antes de expansão.

### Loaders e UI nativa

- [Fabric custom screens](https://docs.fabricmc.net/develop/rendering/gui/custom-screens), [key mappings](https://docs.fabricmc.net/develop/key-mappings) e [HUD](https://docs.fabricmc.net/develop/rendering/hud) sustentam o primeiro adaptador moderno.
- [Forge screens](https://docs.minecraftforge.net/en/1.20.1/gui/screens/) e [key mappings](https://docs.minecraftforge.net/en/1.20.1/misc/keymappings/) sustentam o adaptador Forge 1.20.1.
- [Fabric Loader](https://github.com/FabricMC/fabric-loader) e [Fabric API](https://github.com/FabricMC/fabric) usam Apache-2.0 nos repositórios consultados.
- [MinecraftForge](https://github.com/MinecraftForge/MinecraftForge) e ForgeGradle têm licenças/componentes próprios; mappings e artefatos devem ser conferidos por versão.

O rollout correto é Fabric 1.20.1 → Forge 1.20.1 → 1.19.2/1.16.5 → 1.21.1 → Forge 1.12.2 isolado. “JAR compilou” não equivale a `VERIFIED_RUNTIME`.

## Multiplayer e P2P

### Limite físico/econômico

[STUN](https://www.rfc-editor.org/rfc/rfc8489), [ICE](https://www.rfc-editor.org/rfc/rfc8445), [ICE-TCP](https://www.rfc-editor.org/rfc/rfc6544), [TURN](https://www.rfc-editor.org/rfc/rfc8656), [PCP](https://www.rfc-editor.org/rfc/rfc6887) e as recomendações de firewall [IPv6](https://www.rfc-editor.org/rfc/rfc6092) mostram por que não existe conectividade direta garantida. IPv6, PCP/NAT-PMP/UPnP e hole punching melhoram a taxa direta; NAT simétrico e redes bloqueadas exigem relay. Relay transporta bytes e, portanto, tem custo/limite operacional.

### Projetos avaliados

| Projeto | Licença/estado observado | Compatibilidade útil | Uso permitido/recomendado |
| --- | --- | --- | --- |
| [e4mc client](https://github.com/vgskye/e4mc-minecraft-architectury) / [relay](https://github.com/vgskye/e4mc-quiclime) | MIT; relay MIT/Apache-2.0 | Fabric/Forge 1.20.1 e Fabric 1.21.1 nas árvores consultadas; não cobre toda a matriz | PoC opcional, medir dependência/disponibilidade; sem prometer SLA |
| [World Host](https://github.com/Gaming32/world-host) / [server Rust](https://github.com/Gaming32/world-host-server-rust) | MIT no núcleo consultado; componentes UPnP podem carregar LGPL-2.1 | moderno, sem cobertura integral 1.12.2/Forge 1.21.1 | melhor referência arquitetural: UPnP, UDP punch e proxy fallback |
| [playit agent](https://github.com/playit-cloud/playit-agent) | BSD-2-Clause; companion consultado MIT | externo ao loader, inclusive legado | fallback BYO/user-owned; confirmar termos/branding antes de embutir |
| [Essential](https://essential.gg/) | licença do código consultado restringe uso/cópia/modificação/distribuição | ampla | somente instalação oficial opcional; **não usar como referência de implementação** |
| [MineTogether](https://github.com/CreeperHost/MineTogether) | GPL-3.0; backend comunitário não totalmente aberto | inclui Forge 1.12.2 e parte da matriz moderna | referência social limitada, não base de transporte |
| [Plasmo Voice](https://github.com/plasmoapp/plasmo-voice) | LGPL-3.0 | voz por UDP | voz futura, rota separada; não resolve hosting de gameplay |

### Arquitetura adotada

1. LAN/local server continua sendo o endpoint Minecraft.
2. Aurora oferece descoberta de capacidades de rede e escolhe IPv6/PCP/NAT-PMP/UPnP/hole punch quando possível.
3. Um provider de túnel opcional é um adaptador explícito, removível e autorizado pelo usuário.
4. Relay Aurora próprio fica adiado até haver métricas de taxa direta, latência, banda, abuso, privacidade e orçamento.
5. Convite/presença não compartilha token Firebase bruto; usa sessão curta, escopo e replay protection.

O produto pode prometer “multiplayer simples e gratuito quando houver caminho direto ou serviço externo disponível”, nunca “universal, ilimitado e com SLA”.

## Atualização, CAS e rollback

O [OCI Image Descriptor](https://specs.opencontainers.org/image-spec/descriptor/) valida o trio digest/tamanho/media type como modelo de manifesto. A garantia transacional do [SQLite](https://www.sqlite.org/transactional.html) e seu [atomic commit](https://www.sqlite.org/atomiccommit.html) sustentam o índice local. [Nix GC roots](https://nix.dev/manual/nix/2.24/command-ref/nix-store/gc) é referência para marcar referências vivas antes de coleta.

Aplicação Aurora:

- download para staging com tamanho máximo e hash forte;
- plano puro (`SAFE`, `AMBIGUOUS`, `BLOCKED`) antes de IO;
- journal, troca atômica que preserve o destino se o commit falhar e rollback testado;
- CAS em `blobs/sha256/aa/<digest>` + SQLite, manifests/snapshots como roots;
- GC começa em dry-run e nunca remove blob referenciado.

O fluxo atual remove o destino antes do rename. Isso deve ser corrigido antes de updater/lote, porque uma falha entre as operações pode perder o arquivo anterior.

## IA e Tools

A [function calling API do Gemini](https://ai.google.dev/gemini-api/docs/function-calling) fornece chamadas estruturadas, mas validação/autorização continuam responsabilidade do Aurora. [OWASP Excessive Agency](https://genai.owasp.org/llmrisk/llm062025-excessive-agency/), [Prompt Injection Prevention](https://cheatsheetseries.owasp.org/cheatsheets/LLM_Prompt_Injection_Prevention_Cheat_Sheet.html) e [MCP Security](https://cheatsheetseries.owasp.org/cheatsheets/MCP_Security_Cheat_Sheet.html) sustentam o broker proposto.

- Nível 0: leitura limitada e redigida.
- Nível 1: diagnóstico/sugestão, sem mutação.
- Nível 2: plano tipado, diff, snapshot, confirmação e auditoria.
- Nível 3/autonomia geral: adiado.

Não expor shell, caminhos arbitrários, tokens ou filesystem cru. Logs, manifests, nomes de arquivos e páginas são conteúdo não confiável. No free tier do [Gemini](https://ai.google.dev/gemini-api/docs/pricing), quotas variam e dados podem ter tratamento diferente de tiers pagos; screenshots/logs devem ser opt-in, redigidos localmente, com fallback sem IA.

## Creator e ecossistema

- [KubeJS](https://github.com/KubeJS-Mods/KubeJS) é LGPL-3.0 no repositório consultado e permite extensões poderosas, inclusive carregamento Java; export gerado deve usar subset restrito, determinístico e analisado.
- [CraftTweaker](https://github.com/CraftTweaker/CraftTweaker) é MIT e serve como export opcional posterior.
- FTB Quests se declara All Rights Reserved em metadados consultados; não copiar código, assets ou formato sem autorização/revisão.

O Creator MVP gera recipes/tags/datapack determinísticos, faz preview/diff e exporta subset KubeJS somente com consentimento. Editor genérico de scripts e quests fica para depois.

## Operação e custo zero

| Serviço | Faixa gratuita documentada consultada | Uso Aurora e proteção |
| --- | --- | --- |
| [Cloudflare Workers](https://developers.cloudflare.com/workers/platform/pricing/) | 100 mil requests/dia no plano Free e CPU limitada | API leve; rate limit distribuído, timeout e budget; nunca relay |
| [Cloudflare R2](https://developers.cloudflare.com/r2/pricing/) | 10 GB-mês, quotas de operações e egress gratuito na classe Standard | cosméticos pequenos/versionados; quota por usuário e GC |
| [Firestore](https://firebase.google.com/docs/firestore/pricing) | 1 GiB, quotas diárias de leitura/escrita/delete e 10 GiB de saída | perfis pequenos, listeners limitados e índices controlados |
| [Firebase Auth](https://firebase.google.com/docs/auth) | quota varia por método/plano; telefone pode gerar cobrança | e-mail/OAuth sem SMS; identidade Aurora, não entitlement Minecraft |
| [Supabase](https://supabase.com/pricing) | 2 projetos, storage/egress/MAU limitados no Free | aparência temporária; um objeto atual por usuário, resize e exclusão anterior |
| [GitHub Actions](https://docs.github.com/en/billing/concepts/product-billing/github-actions) / [Releases](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases) | runners padrão gratuitos para repositório público; Releases para artefatos | CI e binários públicos; retenção curta de artifacts e releases imutáveis |

Cada integração deve falhar fechada para gasto: sem upgrade automático, sem cartão obrigatório, `429` tratado, teto diário, cache com limite e função útil offline/degradada.

## Release e supply chain

O [updater Tauri](https://v2.tauri.app/plugin/updater/) exige assinatura; sua chave privada precisa de backup seguro. Assinatura Tauri não substitui Authenticode do Windows. O GitHub recomenda fixar Actions por SHA completo em [Secure use](https://docs.github.com/en/actions/reference/security/secure-use) e oferece [artifact attestations](https://docs.github.com/en/actions/concepts/security/artifact-attestations).

No custo zero:

- GitHub Release imutável, `SHA256SUMS`, SBOM e attestation;
- updater Tauri assinado quando a governança da chave estiver pronta;
- transparência de “publisher não assinado” enquanto não houver Authenticode;
- candidatura ao programa OSS da [SignPath Foundation](https://signpath.org/) é opção, não garantia.

O instalador Git e o asset GitHub de `v0.1.0-alpha` divergem em bytes. Ambos ficam como histórico; a próxima release usa versão inédita e nunca substitui asset/tag.

## Matriz de decisão

| Item | Decisão | Motivo |
| --- | --- | --- |
| Engine Rust própria | MANTER | base testada e adequada |
| Prism/MultiMC | REFERÊNCIA controlada | padrões maduros; licenças/branding exigem disciplina |
| Firebase Functions legadas | DEPRECAR após telemetria/busca | evitar dois backends e duas superfícies de segredo |
| Supabase appearance | MELHORAR no curto prazo | já integrado; quota/GC antes de migrar |
| R2 | ADIAR/avaliar | útil, mas migração sem métrica cria complexidade |
| Swing Companion | SUBSTITUIR incrementalmente | não é HUD in-game |
| Próprio relay multiplayer | ADIAR | custo variável/abuso/operação |
| e4mc/World Host/playit | PoC/INTEGRAR opcional | acelera validação sem infraestrutura própria |
| Essential | NÃO COPIAR | licença restritiva |
| CAS local | IMPLEMENTAR após updater transacional | rollback/dedupe com custo zero |
| Gemini Tools mutáveis | ADIAR até snapshot | excesso de agência sem reversibilidade |
| Master completo | IMPLEMENTAR FUTURAMENTE      | depende de auth/roles/manifest/auditoria |

## Lacunas que ainda exigem validação

- acordo/chave CurseForge efetivamente aprovado para o Aurora;
- compatibilidade runtime 9/9 em máquinas limpas;
- termos de redistribuição de cada mod/asset selecionado;
- elegibilidade SignPath e política de privacidade;
- métricas reais de NAT/latência/banda dos PoCs multiplayer.

Interpretações de licenças/EULA/termos são triagem técnica de risco, não parecer jurídico.
