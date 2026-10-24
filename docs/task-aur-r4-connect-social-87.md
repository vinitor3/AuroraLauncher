# Tarefa 87 — amigos Aurora e anúncios da comunidade

Data: 24/10/2026. Estado: **PROTÓTIPO**, **IMPLEMENTADO MAS NÃO HOMOLOGADO** para publicação. Base: `f79f61f`, branch `codex/aur-r4-connect`, worktree `C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect`. O checkout ativo principal permanece separado; a base histórica `TESTE-main/aurora-skins-runtime` não foi usada.

O arquivo preexistente não rastreado `docs/aurora-connect-voice-session-binding-proposal.md` foi preservado e está fora deste incremento. Não houve descarte de alterações, sobrescrita de outras worktrees, modificação da matriz, Companion/HUD ou alteração do Assistente desktop. O estado cumulativo alpha.14, Core 1.1.5, Skins 0.3.1, Recovery/CAS/Doctor e nove pares permanece.

## Objetivo e contrato

Atender ao pedido de uma experiência semelhante à organização de amigos e sessões da Steam: busca de contas **do Aurora** pelo nick, pedidos aceitos, atividade com nome do modpack, sessões de amigos/públicas em cartões médios ou lista, imagem de catálogo, versão e população declarada. O ingresso normal deverá ocorrer por **Entrar/Pedir para entrar**, sem cópia de convite; neste incremento, essa etapa permanece desabilitada e explicitamente pendente.

Lidos: `AGENTS.md`, prompt consolidado Connect V2, domínio de experiência social, Firebase/Auth atual, regras existentes, configuração de build, componentes Connect e harness DOM existentes. A [ADR-029](engineering/ADR/ADR-029-connect-social-local.md) fixa dados, autorização e isolamento local. O [domínio 87](aurora-connect-social-87.md) descreve comportamento e limites em detalhe.

Decisão: reutilizar Firebase Auth/Firestore SDK existentes e testar regras reais no emulador. Não ampliar `functions` legado, publicar Worker/regras, usar credenciais administrativas, cadastrar serviços pagos ou transformar amizade/anúncio em autorização de transporte. O perfil normal permanece sem social; o perfil `social-lab` fixa um projeto demo e portas loopback próprias.

## Implementação e arquivos

- `apps/desktop/src/shared/social.ts`: contratos limitados, busca exata, imagens permitidas e validação de anúncios.
- `apps/desktop/src/lib/socialFirebase.ts`: operações reais do SDK, contexto da conta, limites/prazos, transações recíprocas, catálogo com projeção protegida, Join recusado.
- `firebase/firestore.rules`: relações privadas, pedidos e prazo, bloqueios, presença, vínculo dono/anúncio/índice e negação de enumeração de contas.
- `apps/desktop/src/features/social/`: interface, estilos, projeções seguras, 25 testes DOM e harness de navegador separado.
- `apps/desktop/src/App.tsx`, `src/styles.css`, `src/lib/firebase.ts`, `src/lib/firebase.social-lab.test.mjs`: integração no launcher, atividade de instância em execução e isolamento do modo local.
- `apps/desktop/package.json`, `src-tauri/tauri.social-lab.conf.json`, `social-lab.html`: build local sem bundle, identificador separado e preview específico.
- `firebase/social-emulators.json`, `firebase/tests/social.seed.mjs`, `social.rules.test.mjs`, `social-client-boundary.test.mjs`: emuladores, três contas descartáveis e provas locais.
- Documentos desta tarefa, domínio, requisitos canônicos e ADR-029. O integrador também atualizou os ponteiros gerais em AGENTS, matriz, backlog, tarefa principal e índice de ADRs.

O Rust do transporte, seus formatos/serviços/aprovações e Core/Skins não foram ampliados por 87. Compilar o executável social não homologa a ponte nem os mods.

## Comandos efetivamente executados

Na pasta `apps/desktop`, pelo responsável da interface:

```powershell
node --test src/features/social/SocialView.smoke.mjs
npx tsc -b --noEmit
```

Último resultado focal: 25 PASS, 0 falhas/ignorados, 1.056,9003 ms; TypeScript PASS. A primeira rodada teve 20 PASS/1 falha de fixture: Linkedom conservou a grafia `referrerPolicy` no atributo. O assert foi corrigido para conferir o mesmo valor sem depender da normalização de nome do DOM; a aplicação já usava `no-referrer`. As rodadas posteriores cobrem essa condição; não ocultar a falha inicial.

Lote DOM do integrador, na mesma pasta:

```powershell
node --test src/features/social/SocialView.smoke.mjs src/lib/firebase.social-lab.test.mjs src/features/connect/ConnectView.smoke.mjs src/features/connect/account-transition.smoke.mjs
```

Resultado: 255 PASS em 3.616,6494 ms; inclui as camadas discriminadas abaixo.

Após a correção isolada de HMR e o teste adicional de CSP, o integrador executou o lote final, na mesma pasta:

```powershell
node --test src/features/social/SocialView.smoke.mjs src/features/social/preview-lifecycle.test.mjs src/lib/firebase.social-lab.test.mjs src/features/connect/ConnectView.smoke.mjs src/features/connect/account-transition.smoke.mjs
```

Resultado final:262 PASS/0 falhas/0 ignorados em3.298,4839ms. Esse lote contém25 sociais +6 HMR +4 Firebase/CSP +227 Connect. Não somar a rodada anterior255 ou os testes focais repetidos.

Da raiz da worktree, pelo responsável de integração:

```powershell
npm --prefix apps/desktop run build
npm --prefix apps/desktop run social:lab:build
```

Build frontend normal PASS; build social executado duas vezes, primeira compilação nativa em 48,85 s e final em 11,60 s, ambas PASS, sem bundle/instalador. Ambos os perfis conservaram o aviso de chunk maior que 500 kB. O build social executou `tsc -b && vite build --mode social-lab`; somente seu artefato final privado está fixado no domínio 87.

Emuladores locais iniciados pelo integrador, da raiz, com Firebase CLI 15.28.2 em cache e Firestore Emulator 1.22.0:

```powershell
node C:/Users/usuario/AppData/Local/npm-cache/_npx/ba4f1959e38407b5/node_modules/firebase-tools/lib/bin/firebase.js emulators:start --only auth,firestore --project demo-aurora-social --config firebase/social-emulators.json
```

Testes SDK/regras e fronteira causal executados pelo responsável do backend, da raiz da worktree:

```powershell
node --test firebase/tests/social.rules.test.mjs > .aurora/connect-validation/social-rules-expanded-final.log 2>&1
node --test firebase/tests/social-client-boundary.test.mjs > .aurora/connect-validation/social-client-boundary-final.log 2>&1
$env:FIRESTORE_EMULATOR_HOST = '127.0.0.1:8187'
node --test firebase/tests/firestore.rules.test.mjs > .aurora/connect-validation/social-existing-rules.log 2>&1
node --test firebase/tests/social.rules.test.mjs > .aurora/connect-validation/social-rules-account-boundary-final.log 2>&1
```

A última rodada inclui uma prova adicional: uma identidade Auth sem cadastro Aurora não consegue ler/listar metadados públicos; uma conta Aurora válida consegue. As regras já exigiam o cadastro; esse teste não altera os bytes do aplicativo/executável. Os 18 testes substituem a contagem anterior de 17, sem somá-las. O backend também executou `npx tsc -b --pretty false` na pasta `apps/desktop`, exit0. A repetição das sete regras pelo integrador está registrada separadamente, sem atribuí-la ao comando/log do agente.

## Resultados e fontes

| Prova | Resultado | Evidência privada preservada |
| --- | --- | --- |
| SDK/regras sociais reais, rodada final | 18 PASS/0 falhas/0 ignorados,6.021,6117ms | `.aurora/connect-validation/social-rules-account-boundary-final.log` |
| Rodada social anterior, subconjunto | 17 PASS,5.582,0716ms; não somar18+17 | `social-rules-expanded-final.log` no mesmo diretório |
| Fronteira causal de conta/prazo | 3 PASS,333,5363ms | `social-client-boundary-final.log` no mesmo diretório |
| Regras anteriores | 7 PASS,975,4074ms | `social-existing-rules.log` |
| Repetição das mesmas regras pelo integrador | 7 PASS,980,2737ms; não somar como14 | `social-87-existing-rules.log` |
| DOM integrado anterior | 255 PASS/0 falhas/0 ignorados,3.616,6494ms; não somar ao final | `social-87-desktop-tests.log` |
| DOM integrado final | 262 PASS/0 falhas/0 ignorados,3.298,4839ms | `social-87-desktop-tests-final.log` |
| Frontend normal | PASS,378módulos | `social-87-normal-build.log` |
| Build social final | PASS,378módulos/11,60snativo | `social-87-native-build-final.log` |

O lote DOM anterior contém 25 sociais + 3 de isolamento Firebase + 227 Connect; o final acrescenta seis casos HMR e um de CSP. As rodadas focais são subconjuntos do final, sem soma duplicada. Os 3 testes causais do adaptador usam SDK substituído para controlar eventos, distintos dos 18 que exercitam o SDK e as regras reais no emulador. Os testes de regras usam contextos autenticados de teste; login real no Auth Emulator foi exercitado separadamente pela interface.

Executável privado da primeira rodada: `.aurora/connect-validation/social-87-native/AuroraSocialLab.exe`,64.704.512B, SHA256 `ad366e84b4c86f6b14c8f6e128ab3b3b5d6cf6940d079b1476dd860672bc2f33`. Identificador próprio `com.aurora.smartlauncher.sociallab`; Auth/Firestore/Worker sociais fixados na configuração local. Não é isolamento de todo o executável: a função nativa preexistente `available_java_versions`/`engine/jre` pode consultar `api.adoptium.net`, fora da CSP. O ensaio não comprova ausência global de tráfego externo nem promove consulta Java a transferência observada. O build normal permanece com social desligado.

### Interface real — ciclos sociais e encerramento operacional observados

Computer Use pelo responsável de integração operou dois clientes de navegador do harness: conta localA buscouB pelo nick, enviou pedido;B aceitou;A viu amizade. B publicou voluntariamente um anúncio público com título/modpack/versão e população declarada0/8;A observou o anúncio e mudou para lista. A bloqueouB; a amizade e a projeção do anúncio desapareceram paraA.

No executável completo, a conta localC autenticou no Auth Emulator e viu o anúncio público como não amiga. BuscouB e enviou pedido;B aceitou pelo navegador. A atualização nativa mostrou **Amigos1**, **Já é seu amigo** e **Enviados0**. B alterou o anúncio de público para amigos com confirmação explícita. Depois, na interface nativa, **Comunidade** ficou vazia e **Dos amigos** apresentou o anúncio com modpack/versão1.21.1/Fabric/**Para amigos**. Não inferir essas provas a partir da compilação, do navegador ou de um botão visível. Não há tráfego de jogo, Minecraft ou voz por estes cliques.

B encerrou o anúncio pelo navegador, que confirmou **Anúncio encerrado**; a atualização nativa deC em **Dos amigos** mostrou **Nenhuma sessão disponível**. A eB usaram **Encerrar sessão local** e ambas as páginas confirmaram **autenticação e conexão de dados liberadas**. C usou **Sair da conta** no launcher nativo e retornou à tela de login. Um primeiro clique fora da área visível foi recusado pelo Computer Use sem ação; após rolagem, o clique e retorno ao login foram observados. Nenhum transporteConnect foi iniciado, e a janela Connect anterior permanece separada.

Esses encerramentos de anúncio/SDK/conta são observados. Depois do logout, o responsável filtrou o caminho exato `.aurora/connect-validation/social-87-native/AuroraSocialLab.exe` e executou `Stop-Process` somente sobre uma instância própria correspondente. A consulta posterior confirmou zero processos nesse caminho; o launcher Connect anterior não foi tocado. O encerramento foi forçado, portanto `normalApplicationExitVerified=false`; não promover a tela de login ou ausência posterior a prova de saída normal do aplicativo.

As sessões próprias de Vite e Firebase CLI receberam Ctrl+C e terminaram com exit1, sem prova de saída graciosa normal. Consulta posterior por `Get-NetTCPConnection` encontrou zero listeners TCP nas portas 8187/9187/1437/4487/4587/9150. Não houve rebind explícito dessas portas neste registro; ausência de listener também não é prova individual de reap/exit0 de todos os possíveis filhos do emulador. As verificações ficam limitadas aos processos/caminhos e portas observados, sem encerrar processos pessoais ou inferir estado global do PC.

Imagem privada `social-87-community-list.png` registra a lista da fixture; não publica dados de usuário real. A CSP da primeira rodada nativa só permitia imagem local/data/blob; portanto essa rodada não comprova foto remota. A subrodada posterior está separada abaixo. Os títulos/projeções/credenciais/IDs de contas descartáveis não são reproduzidos nos documentos; nomes dos três perfis de fixture ficam somente no harness próprio.

### Subrodada nativa posterior — capa real de catálogo

O integrador alterou somente a CSP entre nove pins runtime conferidos, permitindo em `img-src` os três CDNs já autorizados pelo contrato de imagem e conservando `connect-src` social loopback. Executou novamente, da raiz:

```powershell
npm --prefix apps/desktop run social:lab:build
```

Resultado PASS:13,80s de compilação nativa, Vite488ms. Log `social-87-native-artwork-build.log`. Novo executável privado `.aurora/connect-validation/social-87-artwork-native/AuroraSocialLab.exe`,64.704.512B, SHA256 `25801e971f42ca991f84fa9c57879f0a5ec24bc0422c5623ad9d9910f431d495`; não sobrescreveu a cópia anterior. Os quatro testes de `firebase.social-lab.test.mjs` passaram em528,2245ms, com três já existentes e um novo de CSP; não somar quatro capacidades novas ao lote integrado255.

Uma fixture privada `social-87-artwork-fixture.mjs` criou e fechou o anúncio via SDK autenticado no projeto demo. A capa é a imagem pública Fabric API da Modrinth, `https://cdn.modrinth.com/data/P7dR8mSH/icon.png`, obtida pelo integrador na API oficial por `Invoke-RestMethod`. Uso somente como imagem de fixture, não como alegação de modpack/homologação. Não houve publicação em serviço remoto nem publicação desse anúncio pela GUI nesta subrodada.

Na interface nativa, a terceira conta autenticou e a atualização mostrou a capa efetivamente carregada em **Grade** e **Lista**, além de versão/população e botão **Entrar** desabilitado. Após fechamento do anúncio pela fixture, a atualização apresentou **Nenhuma sessão disponível**. Evidência privada `social-87-native-artwork.png`. Somente a imagem/CDN Modrinth foi observada; os CDNs Forge não foram exercitados.

No encerramento desta subrodada, a interface confirmou a atualização sem anúncio e retornou ao login após **Sair da conta**. O integrador encerrou forçadamente somente uma instância no caminho exato do executável de imagem. A primeira consulta de processo ainda contou um em transição; uma consulta posterior confirmou zero. O Firebase CLI próprio recebeu Ctrl+C e terminou exit1; Vite não foi iniciado nesta subrodada. `Get-NetTCPConnection` final encontrou zero listeners nas portas8187/9187/1437/4487/4587/9150. A janela Connect anterior permaneceu intacta. Não declarar saída normal do launcher, rebind explícito ou reap/exit0 de todos os possíveis filhos; o cleanup da primeira rodada não foi reutilizado como prova desta.

Registros privados separados do integrador: `social-87-native/gui-observation.json`, SHA256 `6ddb6fe96e68fc5ac44cec45f4121a58d82acd6a7b4ddef5d66a89e5f2e4eb71`; e `social-87-artwork-native/gui-observation.json`, SHA256 `625189d2b3fc301d205f83bf46dc20e90a8d8ff83aaa479f26345f3b29b0b5bc`, ambos sob `.aurora/connect-validation`. A existência desses registros não altera as limitações de cada rodada.

### Correção posterior de HMR do preview — sem reatribuir a GUI

O aviso real de raiz React duplicada motivou uma correção isolada em `SocialLabPreview.tsx` e no novo `preview-lifecycle.ts`, depois dos fluxos GUI congelados. Não altera SocialView, adaptador social, App normal ou os executáveis anteriores. Cada geração do preview possui uma raiz; no hot reload, desmonta e entrega à próxima geração a promessa original de cleanup SDK. A próxima raiz só monta após essa promessa concluir. Falha permanece como barreira rejeitada e bloqueia novas entradas, inclusive após outro reload. Dispose repetido usa a mesma operação e uma geração substituída enquanto espera nunca monta tardiamente.

O proprietário também aguarda um login já iniciado antes de destruir o app Firebase e verifica a geração antes da leitura de perfil seguinte. Durante o botão de encerramento, a referência ao SDK continua pertencendo ao proprietário até terminar o cleanup; assim, um hot reload concorrente não perde a limpeza em andamento. O novo código não transforma encerramento de aba em cleanup comprovado.

Na pasta `apps/desktop`, o responsável do preview executou:

```powershell
node --test src/features/social/preview-lifecycle.test.mjs
npx tsc -b --noEmit
```

Resultado:6 PASS/0 falhas/0 ignorados,584,663ms; TypeScript PASS. O teste focal usa raízes React reais em DOM sintético, com promessas de cleanup controladas: espera antes de uma segunda raiz, falha propagada a sucessores, geração obsoleta, dispose idempotente e falhas síncronas/de unmount. Não chama Firebase ou rede e não substitui um novo teste manual de hot reload com contas. **As observações GUI anteriores pertencem ao preview anterior; não foram repetidas nem reatribuídas ao preview corrigido.** Os executáveis anteriores continuam imutáveis.

O integrador leu o proprietário, o preview e os testes, sem bloqueador encontrado, e incluiu os seis casos na rodada final de262 PASS. Essa revisão é distinta de novo ensaio GUI do HMR.

## Falhas anteriores preservadas

1. O harness inicial de testes tentou um transpiler não disponível e depois apresentou incompatibilidade CommonJS/ESM. Foi substituído pelo TypeScript já instalado e importações de módulo correspondentes; não instalar ferramenta só para esconder a falha.
2. Expressões de regras com avaliação ternária de `getAfter` produziram `IllegalStateException`/`phaseOne` em Firestore Emulator1.19.8 e1.22. A implementação passou a usar condições booleanas explícitas e fases create/update, mantendo provas positivas/negativas. Os logs de falha permanecem; não declarar comportamento da produção a partir do erro do emulador.
3. Após essa correção, um contexto de projeto antigo ainda apresentou travamento/contenção. A rodada final usou projeto demo isolado `demo-aurora-social-rules-v2`; não promove o contexto anterior nem apaga a falha.
4. A suíte de regras anterior passou com a nova negação de LIST de perfis públicos. Os sete testes não demonstram todas as integrações possíveis; o código usa consulta pontual para o fluxo social, sem ampliar a enumeração.
5. Abrir o aplicativo completo no navegador encontrou a suposição antiga de evento Tauri/`transformCallback`. O ensaio web usa o harness dedicado com o componente e SDK reais; não foi feita refatoração ampla do App para mascarar essa limitação. O executável Tauri foi ensaiado separadamente.
6. O primeiro assert DOM de `referrerPolicy` está qualificado acima. Logs e limites anteriores não foram substituídos por uma declaração de homologação geral.
7. O desenvolvimento com HMR também registrou aviso de criação duplicada de raiz React. Os fluxos GUI finais ocorreram após congelar aquele preview; o aviso permanece no log. A correção posterior e seus seis testes causais estão separados acima, sem inferir ausência de perda de dados ou homologação de hot reload com Firebase a partir deles.

Os logs negativos podem conter recusas esperadas do emulador, inclusive orçamento de expressões. Um teste que confirma negação não prova que toda regra suporta a capacidade máxima com bom desempenho; isso continua parte do orçamento/liberação.

## Segurança, custo e pendências

Nenhuma senha/token de usuário real, chave de serviço ou credencial de transporte foi adicionada. O harness usa credencial pública de fixture que só se autentica no projeto demo local fixado. Não há criação automática de relações/sessões ao montar a interface, consulta por tecla, heartbeat, nova cobrança, deploy, push, publicação, release ou instalador.

Antes de produção faltam serviço confiável de autorização conta–chave–sessão, emissão/admissão/revogação do transporte social, controle global de frequência, orçamento de leituras e retenção. O catálogo limita20candidatos/página, mas pode ler até20projeções adicionais e não limita por si só o número de páginas feitas por um cliente malicioso. Presença/anúncio são declarações com prazo, sem prova de disponibilidade de jogo. Recursos sem validação seguem experimentais e desligados para usuários normais.

TCP/UDP/direto/relay/Minecraft/SVC/Plasmo conservam as provas e pendências anteriores, sem novo PASS por87. Amigos, busca e anúncios têm prova local real; **Entrar sem convite manual**, áudio, seleção automática por menor latência elegível, produção e duas máquinas permanecem pendentes, avaliados separadamente.

Rollback: fechar as sessões próprias, encerrar os emuladores/preview iniciados para este laboratório e reverter apenas os arquivos do incremento revisado. Não encerrar Java alheio nem apagar dados/configurações pessoais, releases, worktrees, rascunhos ou estágios anteriores. Se cleanup ficar incerto no harness, ele bloqueia novas entradas e exige fechar a aba; não declarar liberação completa.

O hash do incremento fica no histórico Git, sob o commit `feat(social): add local Aurora friends and community sessions`, após revisão do integrador. Este documento não inventa um hash antes da operação. Nenhum push/deploy será feito.

**O que você precisa fazer agora:** nenhuma intervenção para continuar a implementação local. A liberação publicada e os ensaios com dois computadores/áudio são etapas posteriores, não pedidos de ação nesta rodada.
