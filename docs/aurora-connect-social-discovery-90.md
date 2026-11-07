# Fonte de jogo descoberta para a sessão social — incremento90

Contrato de implementação local, extensão da ADR-033. Estado inicial: **PROTÓTIPO**;
as provas executadas serão registradas separadamente. Não amplia a matriz de nove
pares Core/Skins nem homologa Minecraft, GUI, internet ou voz.

O anfitrião escolhe explicitamente um jogo iniciado pelo Aurora, revisa uma porta
do próprio processo e autoriza a consulta STATUS existente48. A seleção de uso
único é entregue ao mesmo ConnectManager, sem endereço/PID de autoridade vindo do
renderer. `connect_social_reserve_discovered` recebe conta/epoch, sessão,
visibilidade e selectionId; reserva o mesmo Attempt antes de qualquer consulta de
origem. O controlador mantém reserva antes do token SDK, como89. O worker nativo
retido consome `DiscoveryManager::take_target` e `ApprovedTarget::activate`, fora
do mutex Connect, antes de pedir a prova assinada. A seleção conserva seus30s e a
reserva conserva seus60s originais, inclusive durante consultas lentas.

Somente destinos derivados `127.0.0.1:porta-não-zero` são elegíveis ao contrato
social atual. Uma linha IPv4 wildcard pode produzir essa representação pelo
descobridor existente; sua linha original e o handle do processo permanecem
vinculados. IPv6, outro127/8, porta manual ou substituição do processo/listener
não são aceitos por este caller. O digest social de fonte existente já cobre a
porta derivada; política/grant/continuidade, uma vaga e máximo60s não mudam.

O worker reconfere a origem antes de publicar revisão, após consentimento, antes
e depois da abertura do Host e durante a sessão. Uma consulta bloqueante pertence
ao mesmo worker, nunca segura o mutex Connect e nunca possui Endpoint. Stop,
logout, expiração ou perda de continuidade revogam e fecham os recursos próprios
antes de aguardar a coleta da consulta pendente. O slot continua ocupado até o
worker terminar. Resposta tardia não publica revisão nem reabre transporte.
Encerrar Connect nunca encerra o processo Minecraft observado.

A interface usa o seletor48 existente e consome sua seleção antes do await da
reserva. Remontagem, mudança de anúncio/conta, nova busca e expiração exigem nova
seleção; não há replay automático após erro ou perda de IPC. A capacidade aditiva
`ownedGameTcpDiscovery` habilita apenas esse fluxo Windows/connect-lab. Ausência
da capacidade conserva o comportamento89; build normal permanece indisponível.

O descobridor48, estendido pelo incremento91 desta mesma wave, solicita o protocolo
da versão registrada nativamente no lançamento. Isso é uma consulta observada,
não uma validação runtime de compatibilidade dos nove pares. Metadado desconhecido
não recebe fallback para767. O catálogo contém metadados declarados pelo autor e
não atesta versão/loader. Autoentrada73 continua separada e restrita a cópia
diagnóstica1.21.1/Fabric0.16.14/Java21; social90 não cria GuestSource nem relaxa seu
gate. Generalizar runtime exige contratos de argumentos/metadata/JARs distintos.

Provas exigidas: default-off; forma fechada sem porta/PID; owner/epoch/slot;
reserva observável antes de consulta; consumo único; cancelamento e coleta de
consulta; recusa de seleção vencida/trocada; destino derivado e rechecagem; DOM
de seleção/consumo, atraso/conta/anúncio, capacidade antiga e consentimento novo.
Regressão social89 e lifecycle75–77 permanecem necessárias. Nenhum teste sintético
será descrito como jogo real ou homologação dos nove pares.

Rollback: Encerrar Connect e aguardar recursos liberados; reverter somente90 em
revisão local. Sem push/deploy/instalador/billing/configuração pessoal.

## Evidência de implementação

Arquivos90: `engine/connect/social.rs`, `social_profile.rs`,
`social_source_tests.rs`, comando estreito em `connect_commands.rs` e registro em
`main.rs`; frontend em `features/social/{connect-controller,connect-types,
SocialConnectionPanel}` com respectivos testes. O campo novo de capability é
aditivo e não contém bootstrap/URL/pin. A revisão continua com digest e aprovação
novos; portas derivadas nunca vêm de um campo editável no formulário.

Executados em `apps/desktop` em07/11/2026: `node --test
src/features/social/connect-controller.test.mjs` passou **18/18**,561ms;
`node --test src/features/social/SocialView.smoke.mjs` passou **41/41**,1492ms.
Essas rodadas usam o seletor no estado anterior à consolidação91; as fixtures já
incluem os metadados91. Cobrem seleção explícita/consumo antes da espera,
capability antiga, token tardio após conta inválida, fonte IPv4 estrita, seleção
expirada no clique, anúncio alterado, STATUS atrasado e cancelamento visível.
A regressão51 anterior se sobrepõe e não deve ser somada. Após a consolidação do
seletor91, a execução combinada dos dois arquivos passou **59/59**,1622ms, sem
falhas/ignorados. Essa rodada substitui as contagens focais anteriores, sem soma.

Seis testes nativos novos passaram **6/6**,0,40s na rodada focal serializada pelo
root: DTO/default-off/owner/epoch; processo próprio e substituição na mesma
porta recusada sem conexão; prazo original; reserva e coleta após perder waiter
de stop; liberação de Host real enquanto consulta bloqueante continua retida;
STATUS/pong de fixture pela ponte autenticada para porta descoberta. A fixture
não é Minecraft. O comando completo e o log da execução serializada ficam no
registro integrado do root. As falhas de compilação preparatórias
ficam nos logs integrados do root; o acesso entre módulos ao helper de fixture foi
corrigido antes da rodada. A revisão estática também corrigiu uma lease vazia do
teste de liberação antes de executar esse teste, sem mudança de produção.
O teste ignorado de processo social recebeu RPCs somente de
teste para criar/selecionar/recolher um filho próprio, reservar fonte descoberta
e consultar STATUS pela ponte nativa; a integração Auth/Firestore/broker90 deve
ser registrada separadamente após execução, sem reaproveitar o eco89 como prova90.

## Próximos contratos

O ciclo social90 mantém uma vaga e máximo60s. Alterá-los exige revisar a política
assinada Rust e TypeScript, grants, clocks, continuidade e estado do broker88;
os limites não são apenas apresentação. Social90 não publica `GuestSource` para
o launch73. A integração futura precisa verificar também a continuidade social
no gate final de spawn, além da invitation original, guest/ponte fracos,
cancelamento e epoch. `Authority::Session` isolada não confere a lease social.
Generalizar autoentrada aos nove pares exige perfis/argumentos e inspeção de JARs
de Fabric/Forge/legado; o metadado STATUS91 não substitui essa verificação.
