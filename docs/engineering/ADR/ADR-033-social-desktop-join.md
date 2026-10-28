# ADR-033 — Entrar social no mesmo proprietário nativo do launcher

28/10/2026. Estado: **PROTÓTIPO** implementado e testado localmente, sem homologação de produção; o registro de execução em
`docs/task-aur-r4-connect-social-89.md` é a autoridade sobre testes efetivos.
Complementa ADR-029/030/031/032 e R4-005/008, sem liberar produção ou ampliar a matriz.

## Decisão e recorte

Integrar public/friends + open ao botão Entrar, reaproveitando a mesma chave RAM,
prova, política, contexto, grant, SessionInvitation e continuidade existentes.
Uma vaga, game/TCP loopback, limite original de preparação/política de60s e
continuidade de até3s. Pedidos de entrada sujeitos a aprovação, múltiplos convidados,
descoberta de portas, autoentrada no mundo, UDP/voz, relay e internet não são
consequências deste incremento. Anúncio não comprova servidor ou jogador no mundo.

Não criar outro clienteCLI ou lifecycle em SocialView. O worker pertence ao mesmo
ConnectManager/Attempt/slot, owner main, cancelamento, stop, logout e saída já
existentes. Os caminhos manuais não são confirmados automaticamente e não passam
a aceitar objetos sociais como aprovação.

## Bootstrap e identidade

Somente o executável de laboratório social com connect-lab pode receber, no
startup nativo, um bootstrap explícito do processo local que criou a autoridade:
origem fixa http://127.0.0.1:42888, chave pública Ed25519 e issuerInstance.
Não aceitar origem/pin/projeto por comando Tauri, armazenamento do renderer,
anúncio, grant ou consulta HTTP de descoberta. Manager normal não tem bootstrap.
Ausência, formato incorreto ou perfil normal deixa a capability desligada.
Isso é confiança de laboratório no mesmo PC, não provisionamento de produção.

Reservar o slot e sua operação/geração antes de buscar o token no SDK. A conta
esperada é somente uma comparação: a autoridade autentica o token e retorna UID;
divergência encerra a tentativa. O token do SDK já autenticado entra exclusivamente
em comando nativo estreito, fica em RAM durante a operação e só alcança a origem
fixada. Não entra em argv, snapshot, log, Firestore, outro peer, JVM ou resposta.
Não há credencial administrativa ou fallback novo no Worker de produção.

Uma epoch nativa de conta é incrementada ANTES de limpar o slot na barreira de
troca/logout, mesmo se estiver idle. Reserva com epoch antiga é recusada. Entrega
de token/aprovação exige o mesmo owner/operação/geração e tentativa ainda viva.
O controlador da aplicação também descarta respostas SDK tardias e aguarda cleanup;
navegar ou desmontar SocialView não é dono da sessão.

## Entrega interna local

Extensões somente no adaptador local admission:true, POST JSON limitado, Host
pinado e Origin recusado, sob autenticação e prazos existentes:

| Rota | Corpo | Resultado |
| --- | --- | --- |
| /v1/social/identity | {} | accountId e issuerInstance autenticados |
| /v1/social/host/poll | policyId | waiting ou admitted com grant, initialLease e lease atual |
| /v1/social/host/publish | grantId, invitation string | accepted após validar dono e contexto |
| /v1/social/guest/receive | grantId | waiting ou ready com invitation e lease |
| /v1/social/host/stop | policyId | stopped; torna a política/grant terminal |
| /v1/social/guest/stop | grantId | stopped; torna o grant terminal |

O broker mantém apenas objetos públicos assinados e envelope de transporte
limitado, nunca token/Request. A invitation é dado opaco não confiável até a
verificação nativa completa de assinatura, host, destinatário, serviço e prazos.
Não é publicada no catálogo. Somente o host autenticado correspondente publica;
somente o guest correspondente recebe. A política/slot/prazos originais limitam
retenção. Falha de autorização, bloqueio, alteração da projeção, stop ou expiração
não permitem reabrir a vaga consumida. O cliente não renova lease por consultar
uma caixa postal.

A primeira lease de sequência1 é retida separadamente da mais recente. O host
inicia a verificação pela primeira ainda válida e só depois aplica a atual se
sua sequência for maior. Não relaxar SocialContinuity::start para aceitar uma
sequência arbitrária. O convidado conserva a continuidade original desde join,
sem reconstruí-la a partir do último valor de um canal watch. Uma primeira
resposta recebida tarde não ressuscita a janela original já vencida.

Host consulta em500ms; guest aguarda entrega em250ms, dentro do prazo de ingresso.
Guest começa continuidade em750ms logo após join, inclusive enquanto aguarda o
host; o host recebe a última lease assinada. Revalidar relógios civil/monotônico
e a referência do registro após operações assíncronas. Não guardar conexão por
causa de uma resposta atrasada. Falha do backend fecha recursos nativos próprios;
notify-stop é melhor esforço limitado e não condiciona cleanup local.

## Interface e consentimento

No anúncio próprio elegível, o anfitrião prepara compartilhamento da fonte TCP
loopback selecionada, vê fonte/visibilidade/serviço/uma vaga/prazo e confirma a
política exata. Guardar anúncio não abre transporte. No convidado, clicar Entrar
é a ação explícita de ingresso; o aplicativo resolve contexto, prova chave,
obtém grant e troca invitation internamente, sem copiar JSON/chaves/convite.

Mostrar preparação, revisão anfitriã, aguardando convidado, conectando, transporte
conectado, erro redigido e recursos liberados a partir do snapshot nativo. Endereço
TCP local pode ser mostrado/copied para o jogo; isso não é autojoin Minecraft.
Não permitir botões manuais de revisar/exportar convite sobre uma tentativa social.
Pedir para entrar continua indisponível enquanto sua política não for implementada.
O build normal segue desligado; social-lab só habilita Entrar com capability nativa.

## Critérios de verificação e rollback

Exercitar backend positivo/negativo e concorrência; manager/DTO/epoch/default-off;
controlador com token atrasado, navegação, logout e clique duplicado; dois managers
reais + Auth/Firestore/broker, TCP exato nos dois sentidos/EOF e cleanup; bloqueio,
perda de backend, não amigo e mudança de conta. Executar interface Tauri real
separadamente, sem atribuir fixtures DOM à GUI ou eco a Minecraft. Registrar
falhas, retestes, hashes, comandos e limites no task file.

Rollback: parar somente processos de laboratório próprios e reverter o incremento.
Sem push/deploy/regras remotas/instalador/billing/firewall/configuração pessoal.
Preservar outras worktrees, proposta de voz não rastreada, nove pares e Assistente.
