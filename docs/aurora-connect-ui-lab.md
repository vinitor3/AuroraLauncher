# Teste do Connect pela interface, em perfil separado

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. Este perfil prepara o launcher completo com a interface e a biblioteca nativa Connect já existentes, somente para teste local autorizado. Em24/10, o guia foi incorporado ao executável, os dois papéis da interface passaram em transporteTCP real no mesmo PC e um ensaio separado consultou um servidorMinecraft real pelo painel convidado, conforme a evidência delimitada abaixo. Não é release, instalador, publicação, homologação completa ou distribuição geral. Voz permanece fora destes ensaios. [Contrato desktop](aurora-connect-desktop.md), [sessão TCP](aurora-connect-desktop-session-lifetime.md) e [evidência externa dos convites](aurora-connect-external-invites-85.md).

## Perfil e preservação dos dados

`apps/desktop/src-tauri/tauri.connect-lab.conf.json` sobrepõe somente nome, identificador, janela principal e bundle desativado. Mantém a janela `main`, CSP, comandos e autenticação normais. O comando de build seleciona explicitamente `connect-lab`, sem selecionar `connect-voice-lab`, e usa `--debug --no-bundle`, Cargo offline/locked/dois jobs. Não altera default features, versão, matriz, dependências, lockfiles ou fluxo oficial de release.

O identificador de teste é `com.aurora.smartlauncher.connectlab`, diferente de `com.aurora.smartlauncher`. No Tauri2.11.5 travado, `app_data_dir` e o diretório WebView2 padrão Windows derivam desse identificador. Assim, os dados nativos ficam em `%APPDATA%\com.aurora.smartlauncher.connectlab` e os dados WebView em `%LOCALAPPDATA%\com.aurora.smartlauncher.connectlab`. Não se copiam automaticamente contas, tokens, instâncias, configurações ou cache do launcher normal. É isolamento de armazenamento do aplicativo, não sandbox do Windows: comandos normais de seleção de arquivos, rede e Java conservam suas permissões. Autenticar/selecionar/importar/iniciar jogo continua exigindo as ações e revisões normais; não há bypass de login ou entitlement.

O ensaio básico pode usar Minecraft1.21.1 já aberto separadamente. O botão de abrir cópia exige uma cópia válida, os módulos e os gates73 próprios; não instala arquivos nem faz uma cópia nova passar por consequência. Os dados do perfil de teste são novos e ficam preservados ao encerrar. Não excluir ou renomear pastas do usuário para recuperar o ensaio.

O isolamento é local: Firebase, Worker e a conta remota continuam os serviços normais; entrar ou usar ações de conta conserva seus efeitos remotos. Não automatizar login nem compartilhar credenciais no chat. Selecionar arquivos/Java e importar conteúdo continua podendo acessar caminhos escolhidos explicitamente. Este perfil não muda tais contratos nem transforma o aplicativo inteiro em laboratório sem rede.

## Comandos de desenvolvimento do responsável

Da raiz desta worktree, com as dependências locais existentes:

```powershell
npm --prefix apps/desktop run connect:lab:build
```

Isso compila o frontend e o executável nativo com assets locais, sem NSIS/MSI/bundle. Não inicia o launcher, Java ou transporte, e não aceita termos por alguém. Antes de usar, conferir o hash do executável efetivamente copiado para uma pasta privada nova; build/hashes/resultados ficam no task. O cache `target/debug` é mutável, não é um kit imutável nem deve ser compartilhado como release.

O perfil debug conserva o console Windows do desenvolvimento. Ele pode abrir junto da janela, mas não exige digitar comandos; deixá-lo aberto até usar **Fechar o launcher**. Fechar esse console à força pode interromper o cleanup. O comando não gera atalho de instalação ou assinatura de distribuição.

Para desenvolvimento com servidor frontend local, o comando separado é:

```powershell
npm --prefix apps/desktop run connect:lab:dev
```

O comando antigo `tauri dev --features connect-lab` continua usando o identificador normal; não usá-lo neste ensaio separado. Não iniciar duas compilações Cargo simultâneas ou alterar fontes durante uma compilação.

## Ensaio a preparar antes de abrir aos dois computadores

1. Verificar build, hash, WebView2/runtime e que o perfil de teste conserva o identificador/CSP/autenticação esperados. Abrir a janela de teste não comprova o fluxo IPC.
2. Exercitar no mesmo PC a interface de anfitrião/convidado, revisão, aprovação, STATUS e encerramento com as autoridades nativas reais. Fixtures DOM e testes do manager são evidências distintas, sem substituir cliques Tauri reais.
3. Somente depois, coordenar um convidado em outra internet com dados de conexão novos. O teste básico seleciona **Sessão de jogo · até 1 hora**, somenteTCP, e modo IPv6 externo explícito; ingressos/revisões curtos continuam seus prazos originais, sem renovação automática. Não aguardar troca de mensagens desnecessária depois de `conectado` para entrar no jogo.

Este roteiro não declara por si os passos executados; os resultados das rodadas ficam nas seções de evidência abaixo. Endereço inicial e código público precisam ser novos e conferidos no anfitrião; não copiar valores históricos. A interface já permite copiar os códigos/convite/endereço, revisar e autorizar sem colar JSON ou `approve` no PowerShell. O serviço Minecraft precisa estar aberto e autorizado; descoberta de LAN só vê processos iniciados pelo mesmo launcher e não abre o mundo para LAN.

Esse painel manual é o laboratório de integração. A experiência final exigida permite sessões privadas, para amigos e públicas, com **Entrar** sem convite manual ou **Pedir para entrar** conforme a política escolhida pelo anfitrião. O [contrato social](aurora-connect-social-experience.md) registra o requisito; amigos, catálogo público e seleção automática de caminho continuam **NÃO IMPLEMENTADO / PRECISA DE TESTE**. O ensaio deste documento não os libera por consequência.

## Resultados e encerramento

Registrar separadamente build, abertura da janela, invocações Tauri, admissão, STATUS, entrada/caminhada no mundo, desconexão, recursos liberados e topologia. `Conectado` é transporte autenticado; STATUS/população declarada não identifica jogador nem comprova mundo. Não atribuir áudio, contas mistas, Core/Skins ou suporteNAT/CGNAT ao ensaioTCP.

No convidado, sair do servidor no Minecraft e usar **Encerrar Connect**; aguardar **Recursos liberados**. No anfitrião, encerrar Connect e aguardar o mesmo resultado. Depois usar **Fechar o launcher** na bandeja; fechar a janela pode apenas ocultá-la. O Minecraft e o servidor continuam sob seus proprietários, com encerramento próprio. Não matar Java ou remover configuração como atalho. Um convite consumido/expirado não é retomado; nova tentativa exige preparação/revisão novas.

Rollback de código remove somente o perfil/script de teste por revisão local; o build normal continua com Connect desativado. Rollback operacional encerra a aplicação e seus recursos, preservando os dados de teste e os dados normais. Não gerar instalador, publicar artefatos ou alterar firewall/roteador/faturamento neste fluxo.

## Evidência inicial23/10/2026

`npm --prefix apps/desktop run connect:lab:build` passou: TypeScript + Vite372módulos/1,52s e nativo `dev`1m36s, exit0, sem bundle/NSIS. O aviso de chunkJavaScript acima500kB permanece, sem refactor incidental. Copiado com CreateNew para pasta privada nova: `AuroraConnectLab.exe`,64688128B, SHA256 `28fcaba79dc7115af0b4363faf029af7aaff3a0e970ce62da7841b99dd03085b`. Dados do plano/build e logs ficam no diretório privado `connect-ui-lab-3171cbf98d07428eaf99d62e4513ac04`; o executável anterior do cache foi preservado antes do build. Não é kit publicado nem instalador.

Rodada existente de DOM: `node --test src/features/connect/ConnectView.smoke.mjs src/features/connect-game-launch/ReadyGameLaunch.smoke.mjs src/features/connect/account-transition.smoke.mjs`, na pasta desktop, **241PASS/0falhas/0skip**,6854,7195ms. São fixtures sintéticas já existentes, sem novos testes nem provaIPC/runtime. Log `connect-ui-lab-dom.log`. Dependências/lockfiles/matriz permanecem byte-exatos; nenhum Rust alterado para este perfil.

Computer Use abriu somente a cópia pinada e observou a janela **Aurora Connect — teste isolado** carregando assets de `http://tauri.localhost/`, sem Vite em execução. Diretórios do novo identificador e uso desse diretório pelo WebView próprio foram observados por leitura local. Isso confirma abertura/assets e isolamento de caminho nessa amostra; não é comparação byte-exata de toda a pasta normal nem sandbox. Relatório privado `first-window-observed.json`, SHA256 `9a16f1add6361ba7a03f74d928336d4b90ad505134bd4fb4281708d1a319c33a`.

A janela chegou ao login normal. Nenhuma credencial foi lida, digitada ou copiada, e nenhuma autenticação foi automatizada. A ação humana de login foi solicitada, com o fundamento explícito da skill Computer Use. Até este checkpoint23/10, não houve clique nos controlesConnect, ponte/Java iniciados, prova de encerramento do aplicativo ou homologação gráfica. Naquele momento, a janela permanecia aberta para essa ação; os testes posteriores são registrados separadamente abaixo, sem reatribuir evidências à abertura inicial.

## Guia no executável e transporte pela interface —24/10/2026

Novo `npm --prefix apps/desktop run connect:lab:build` **PASS**, frontendVite373módulos/1,54s e nativo `dev`21,21s, sem bundle/instalador e sem selecionar `connect-voice-lab`. Cópia própria nova em `.aurora/connect-validation/connect-ui-guide-24/AuroraConnectLab.exe`: **64692224B**, SHA256 `8a535bd65131ce93c2a1df9990a7b10067019ec05ec36dda8cddaad3a108ba2c`. O executável inclui o cartão **Próximo passo**, instruções e contagens de prazos existentes; revisão, confirmação e encerramento mantêm as autoridades nativas originais. Não substituir este hash pelo build anterior23/10.

Computer Use exercitou a janela real **Aurora Connect — teste isolado**, com assets embarcados e a conta de teste já pronta; nenhuma credencial foi digitada ou gravada. Dois casos próprios usaram loopback, `game/tcp`, convite original e endpoint sintético limitado, sem Java/Minecraft:

| Caso | Resultado observado |
| --- | --- |
| Interface anfitriã + agenteCLI convidado | Guia, revisão e aprovação explícita observados; convite exportado; admissão em74ms; transferência de1048576B em cada direção, valor de byte71 e EOF corretos; admissão refletida no painel.74ms mede admissão, não RTT da rede. |
| AgenteCLI anfitrião + interface convidada | Guia, convite original revisado e aprovação explícita observados; transporte autenticado e caminho direto; ponteTCP efêmera observada em127.0.0.1:53190;1048576B em cada direção, valor de byte71 e EOF corretos. A porta é resultado daquela tentativa, não endereço fixo para repetição. |

Nos dois casos, **Encerrar Connect** levou a **Encerrado/Recursos liberados**. O agenteCLI próprio terminou exit0, recolhido e sem encerramento forçado; echo foi fechado sem interrupção, probe/arquivo de plano liberados, watchdog não venceu e `clean=true`. Os três rebinds do helper verificaram somente as portas fixas **UDP42000, TCP42010 e TCP43001**. No caso4, a ponte realGUI era **TCP53190**: seu encerramento foi observado pelo estado nativo da interface, **sem rebind explícito dessa porta registrado**. Não atribuir o rebind de42010 à ponte53190, nem inferir prova retrospectiva. O socketUDP de transporte liberado não é prova de UDP de aplicação. Relatórios privados próprios: `gui-host-case3.json`, SHA256 `c0eb92530b34e7f7115a741e1409c7059b132456253be5ee252bb3b6abc5f4d1`, e `gui-guest-case4.json`, SHA256 `f5e217219dd4751249b6313ce0ff565e31443955942a8e8d0f8f87d0a59e2b0f`, no mesmo diretório do executável. Não contêm convites ou chaves; os relatórios imutáveis não foram regravados para esta precisão de interpretação.

As duas falhas preparatórias anteriores permanecem preservadas: uma antes de criar agente por indisponibilidade de `process` no helper e outra com o agente já indisponível antes de revisar o convite. Não são PASS de transporte e não foram apagadas para obter os resultados acima. Depois de confirmar Connect fechado, duas instâncias antigas próprias do aplicativo foram encerradas forçadamente para abrir a nova cópia; isso **não comprova saída normal do aplicativo**. A nova janela permanece aberta com Connect encerrado; não declarar cleanup completo do processo do launcher.

Fixtures combinadas: `node --test src/features/connect/ConnectView.smoke.mjs src/features/connect-game-launch/ReadyGameLaunch.smoke.mjs src/features/connect/account-transition.smoke.mjs`, na pasta desktop, **254PASS/0falhas/0skip**,3742,3289ms, log `desktop-ui-guidance-combined.log`. Os204 testes do cartão/painel incluem13 novos; os50 dos outros arquivos já existiam. A revisão independente final do guia e helper não encontrou bloqueadores; fixtures, build e ensaio gráfico são evidências distintas, não três homologações do mesmo resultado.

Este checkpoint fecha o percurso local de revisão/aprovação/admissão/bytesTCP/encerramento de ambos os papéis na interface. **Os casos3/4 acima não executaram Minecraft STATUS, entrada no mundo, UDP de aplicação, áudio SVC/Plasmo, rede externa, amigos ou catálogo público.** O ensaioMinecraft posterior tem registro próprio abaixo; dois computadores e saída normal do aplicativo continuam separados. Defaults, gates, nove pares, configurações pessoais e custo zero permanecem; nenhum push/deploy/publicação/release/instalador.

## ConsultaMinecraft real pelo painel convidado —24/10/2026

Uma execução posterior, independente dos casos3/4 de echo, usou o mesmo executávelSHA `8a535bd65131ce93c2a1df9990a7b10067019ec05ec36dda8cddaad3a108ba2c` e servidor descartável próprio **Minecraft1.21.1/Fabric0.16.14/JDK21.0.12.1**, sem mods, `online-mode=false` e whitelist vazia. A aceitação EULA e autorização desse teste já existiam; o responsável leu os planos e aprovou os hashes locais sem nova aceitação em nome do usuário. Nenhuma instância/configuração pessoal foi usada.

`runtime.mjs plan/stage` verificou nove artefatos/55582631B pelo planoSHA `f6ab6d12656111a7fcea8d490c86b76535885e7278571e15de4610a4f31773c1`, criando somente o estágio novo `prepared-ZlEyln`. `server.mjs run-plan/run` usou planoSHA `f82c5bcfca6d69f5778f41abc78df92a9f13919e2c841caa27262d3113711bd7` e prazo300s, sem ampliar preparação. `server_ready` comprovou bindloopback do próprio Java, STATUS767 e pong correspondente. Os comandos exatos estão no [registro da execução](task-aur-r4-connect.md).

O helperHost próprio revisado, SHA `ea53cd59afbbd3743b745ea21f58882433d95a8eafa81777490d2f58cfd3fb83`, autorizou somenteTCP para o servidor fixo em127.0.0.1:43001, com planoSHA `e44babd5464375573f09b47cb22b97e248c54845621a90e934cc0677537b8561`, sessão600s e preparação60s originais. Não gerencia nem encerraJava: o executor de servidor conserva esse ownership.

Na janelaTauri real, o responsável preparou o convidado, revisou o convite original, confirmou o consentimento e autorizou. Após transporte autenticado/caminho direto observados, clicou **Consultar servidor uma vez**. A tela mostrou **Servidor respondeu**, protocolo767 correspondente e população declarada0de2, enquanto mundo continuou **Não verificada** e ambos os mods de voz desativados. **Copiar endereço** confirmou sucesso. Isso comprova consultaSTATUS/pong pela ponte e projeção da resposta na interface, não entrada no mundo ou dois jogadores.

**Encerrar Connect** chegou a **Recursos liberados**. A ponteGUI efêmera observada nesta tentativa foiTCP63224; seu encerramento tem a evidência do estado nativo da interface, **sem rebind explícito de63224 registrado**. HostCLI saiu0, recolhido/sem força, UDP42000 admitiu rebind e plano foi removido, `clean=true`. Separadamente, o responsável enviou `stop` ao executorJava: `server_stopped` com `closed=true`, exit0, `forced=false` e `eulaRestored=true`; a portaTCP43001 admitiu rebind. Esses dois rebinds não verificam a porta efêmeraGUI. A nova janela do launcher continua aberta com Connect encerrado; saída normal do aplicativo não foi comprovada.

O primeiro postcheck recusou **TEST_PROPERTIES_CHANGED**, pois esperava indevidamente bytes idênticos de `server.properties` após o próprio Minecraft executar. Investigação somente leitura encontrou as16 chaves originais com valores iguais e45 defaults adicionados pelo jogo: `testPropertiesByteExact=false`, nenhuma chave original alterada. O arquivo não foi regravado/restaurado para obter PASS; o resultado é qualificado, **não** “properties byte-exatas”. EULA teve hash original exatamente restaurado; marcador de uso do estágio foi preservado, portanto o estágio não deve ser reutilizado.

Evidências privadas em `.aurora/connect-validation/connect-ui-minecraft-24`: `gui-observed.json`, SHA256 `04737de6f392b32b9d10875c993665adcae8bc54882a4cfc21b06cdc102cbc1e`, e `postchecks.json`, SHA256 `fce35fd50fca9a3120d8abbd4fefc9303b4a613637cf16b9bf0081817655790d`. Sem convites/chaves nos documentosGit. Este ensaio não executou mundo, contas mistas, UDP de aplicação, áudio, rede externa ou homologação dos nove pares; relay continua desligado e nenhuma configuração pessoal foi alterada.
