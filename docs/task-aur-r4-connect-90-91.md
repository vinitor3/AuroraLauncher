# Aurora Connect — continuação local de descoberta e versões

Data: 07/11/2026. Estado de trabalho: **PARCIAL**, implementação e validação em andamento.

## Base e escopo

Repositório ativo conferido em `dc2ec5a`, branch `codex/aur-1165-multiplayer`, limpo.
O trabalho continua na worktree exclusiva `codex-aur-r4-connect`, branch
`codex/aur-r4-connect`, base `142c869`. A revisão documental `5ac2f7e` é ancestral
desta base; o pacote de documentos `d6f31db` não é ancestral e foi reconciliado por
conteúdo nos incrementos anteriores. Nenhuma outra worktree é alterada.
O rascunho preexistente não rastreado `docs/aurora-connect-voice-session-binding-proposal.md`
é preservado e excluído dos commits desta continuação.

Foram reconferidos os AGENTS principal e desta worktree, o prompt consolidado
original completo, HEAD/status/worktrees/ancestralidade, manifests desktop/Rust,
dependências fixadas, matriz ativa e contrato cumulativo Skins. Não existe lockfile
npm na raiz; desktop e edge têm lockfiles próprios. Iroh permanece1.3.0,
Tauri travado2.11.5, Core1.1.5 e Skins0.3.1. A base contém os recursos posteriores
alpha.14 de recovery/CAS/Doctor. O histórico TESTE-main não é base.

O pedido de atender todas as versões é aplicado aos nove pares já selecionados
em `scripts/active-mod-matrix.json`: Forge1.12.2 e Fabric/Forge1.16.5,1.19.2,
1.20.1,1.21.1. Não amplia a matriz e não transforma STATUS/build em homologação.
SHA-256 da matriz: `96c4ec789902f8aa87931ecd72aca531fec53d4ac9c6055fcb9837ef77e6918a`

## Incrementos

- 90: [seleção de origem social](aurora-connect-social-discovery-90.md), derivada
  do processo próprio, substitui a dependência da porta fixa no novo fluxo.
- 91: STATUS usa o perfil imutável do jogo iniciado, com os nove pares
  reconhecidos explicitamente. Perfil desconhecido não recebe fallback767.
- 86A1: revisão nativa de voz a partir do binding original86A0; os limites e
  gates de transporte/retirada86A/B permanecem distintos.
- A saída explícita **Fechar o launcher** fica acessível na interface principal,
  usando o mesmo `ExitRequested` da bandeja. Não mata o processo nem contorna
  cleanup Connect ou decisões de recuperação/recibo de voz.

Root é proprietário de App.tsx e dos registros main/engine; colaboradores têm
arquivos separados para90,91 e86A1. Cargo e fixtures com portas fixas serão
executados em sequência. Testes DOM, fontes/processos sintéticos, transporte real,
interface e Minecraft/áudio serão registrados separadamente.

## Evidências

Primeira rodada de `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml
--offline --locked --jobs 2 --all-features --tests`: falhou por arquivo de teste86A1
ainda não criado e novo erro91 ausente no classificador86. Segunda rodada: falhou
por helper de fixture social privado. Foram corrigidos respectivamente a inclusão
do arquivo, classificação explícita `RuntimeUnavailable` e visibilidade restrita
`pub(super)` do helper. As falhas permanecem nos logs ignorados de validação.

Com o mesmo manifesto e flags offline/locked/jobs2/all-features, `cargo test --lib
connect_status_profile -- --test-threads=1` passou **6/6**,0,81s;
`cargo test --lib engine::connect_discovery::tests -- --test-threads=1` passou
**9**, com **1 fixture filha ignorada** na invocação direta,3,27s. Inclui os nove
perfis oficiais projetados e nove consultas STATUS/pong em socket real de filho
próprio. Os processos são fixtures, sem Java/Minecraft. Protocolo divergente e
runtime desconhecido são recusados sem fallback.

`cargo test --lib engine::connect::live::social::source_tests --
--test-threads=1` passou **6/6**,0,40s. Inclui porta derivada, consumo único,
cancelamento com query retida, encerramento antecipado do Host e STATUS/pong pela
ponte autenticada. Não é prova de GUI, backend, mundo ou voz.

Em `apps/desktop`, `node --test` nos arquivos
`features/social/{connect-controller.test,SocialView.smoke}.mjs`,
`features/connect/{ConnectView,account-transition}.smoke.mjs` e
`features/connect-game-launch/ReadyGameLaunch.smoke.mjs` passou **314/314**,4,92s.
Os caminhos completos começam em `src/`. As rodadas focais dos colaboradores
estão contidas nessa contagem e não são somadas. `npm run build` passou; conserva
o aviso existente de chunk acima500kB. Nenhuma dependência foi alterada.

`cargo test` com os mesmos flags e filtro `native_voice` passou **13/13**,238,52s:
sete pais86A0 existentes, cinco pais86A1 novos e o golden novo da preimagem.
Inclui ambos os perfis, alteração real de configuração da fixture com TCP ainda
válido, prazo2s, reserva anterior ao despacho, query retida e perda de waiters.
Não abre Host nem transporta áudio; o contrato86A/B permanece separado.

Auth/Firestore Emulator foram iniciados em projeto `demo-aurora-social`, somente
loopback, pelo CLI Firebase previamente disponível e configuração
`firebase/social-emulators.json`. Com `AURORA_SOCIAL_DESKTOP_INTEGRATION=1` e
`AURORA_SOCIAL_DESKTOP_TEST_BINARY` apontando para o binário de teste local,
`node --test --test-concurrency=1 firebase/tests/social-desktop.integration.test.mjs`
passou **7/7** (seis casos e pai),17,93s. Amigos/público transferiram1MiB/direção;
bloqueio, troca de conta e perda do backend encerraram a ponte. Não amigo e conta
bloqueada não abriram conexão à origem. O novo caso90 selecionou porta efêmera
de filho próprio, consultou STATUS/pong pela ponte, encerrou o filho normalmente
e observou fechamento dos dois peers e rebind antes da expiração da sessão.
Essa fonte é sintética, sem Java/Minecraft/áudio.

Primeiro Clippy all-features/all-targets com `-D warnings` falhou por oito
argumentos no ator social e match condicional colapsável no perfil Forge.
Corrigidos com escopo emprestado da política/origem e match guard equivalente.
Clippy repetido com `--offline --locked --jobs 2 --all-targets -- -D warnings`
passou em all-features, default e somente connect-lab (25,15s/13,08s/20,01s).
`git diff --check` passou. O comando `npm --prefix apps/desktop run social:lab:build`
passou, com `--debug --no-bundle`,1m21s de compilação nativa e frontend493ms.
O executável privado copiado para teste tem SHA256
`b048ef81718e81f89db656b7d102e9b0708e73efe1dad7c2c50aa393024c58d9`;
o helper de teste copiado, `b08971475129d951a671fe1fb27aa23c2ffa8b17b7c8dfc54c608425b304e3c1`.
O checkpoint revisado foi commitado localmente em `0bca0ea`. Regressão completa
será executada após os próximos incrementos, sem somar as provas focais acima.

### Execução gráfica posterior ao checkpoint

O mesmo exe privado e helper acima foram copiados antes da próxima wave para
`.aurora/connect-validation/social-gui90-e4287d4a95e24181b708cc77b2eb03b4/`,
preservando bytes mesmo durante compilações futuras. O helper
`firebase/tests/social-desktop-gui.mjs` foi executado com opt-in
`AURORA_SOCIAL_DESKTOP_GUI=1` e caminhos absolutos nas variáveis
`AURORA_SOCIAL_DESKTOP_EXE` e `AURORA_SOCIAL_DESKTOP_TEST_BINARY`.

Pela skill Windows/computer-use, o operador automatizado entrou numa conta
descartável do emulador, viu lista de amigos/catálogo e acionou **Entrar** no
cartão público. A tela mostrou **Conexão autenticada** e endereço local derivado;
o probe externo à UI confirmou1MiB por direção. **Desconectar** produziu
**Conexão encerrada / Recursos da conexão liberados** e o Host real informou
`phase=closed`, `cleanupState=complete`, sem erro.

Outro Host local com visibilidade **friends** foi preparado; **Entrar** na linha
do amigo autenticou nova ponte e outro probe confirmou1MiB por direção. Com a
tela ainda exibindo conexão autenticada, **Fechar o launcher** fechou a janela.
O supervisor observou exit0, o Host chegou a closed/complete e o helper terminou
normalmente com `cleanup=true`, `nativeApplicationNormalExit=true`. O teste não
forçou o processo e verificou rebind dos endpoints próprios no teardown.
Emuladores receberam um único Ctrl+C e emitiram shutdown controlado; seu shell
retornou1 após SIGINT, distinto do exit0 do launcher/helper.

A primeira tentativa de acionar o cartão encontrou o botão abaixo da área
visível; a automação recusou o clique sem executá-lo. A janela foi rolada e uma
nova autoridade local foi criada antes do clique válido. Não é falha de
transporte nem nova prova de expiração. Esta rodada não exercitou hospedagem
gráfica por processo Minecraft, mundo, áudio, internet ou autoentrada73/92/93.

## Estado por componente

| Componente | Estado desta continuação |
| --- | --- |
| TCP | Ponte autenticada local comprovada no ingresso social, com retirada e rebind. |
| UDP | Contratos e provas anteriores preservados;86A1 não abre ponte UDP. |
| Direto | Loopback desta rodada; as provas IPv6 externas anteriores continuam separadas. |
| Relay | Nenhum relay de produção ou serviço pago habilitado. |
| Minecraft | STATUS de fixtures para os nove pares; nenhum jogo real novo ou homologação. |
| Convites/interface | Entrega interna social, Entrar público/amigo, Desconectar e saída normal passaram no exe privado local. Hospedagem descoberta tem prova nativa/backend, sem GUI Minecraft. |
| Simple Voice Chat | Preparação/revisão nativas do perfil passaram em fixture; transporte vinculado/áudio pendentes. |
| Plasmo Voice | Preparação/revisão próprias passaram em fixture; transporte vinculado/áudio pendentes. |

## Segurança, limites e rollback

Sem push, deploy, regras remotas, faturamento, instalador, mudança de firewall,
relógio, configuração pessoal, mod ou dependência. Build normal continua desligado
para os experimentos. Stop/saída aguardam os workers reais e preservam o jogo
já iniciado. Rollback de código é revert local revisado dos commits próprios,
preservando evidências e alterações posteriores; sem reset/clean.

TCP, UDP, direto, relay, Minecraft, convites/interface, Simple Voice Chat e Plasmo
Voice mantêm os estados separados acima. O serviço social remoto, aprovação
individual, grupos sociais dinâmicos, autoentrada multiversão, áudio real, redes
adicionais e sincronização de modpacks não são concluídos por estes incrementos.

## O que você precisa fazer agora

Nenhuma intervenção é necessária para esta implementação e seus testes locais.
