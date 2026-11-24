# ADR-032 — Resolução autenticada do contexto para Entrar

25/10/2026. Estado: **PROTÓTIPO**, implementado e testado localmente conforme [tarefa88C](https://github.com/vinitor3/AuroraLauncher/blob/088bf5b435b1b63e842c11ff1c01648b5e38bced/docs/task-aur-r4-connect-social-88c.md), que preserva também a primeira falha de cleanup e suas limitações. Complementa ADR-030/031 e R4-005/008, sem publicar serviço ou habilitar o botão gráfico por inferência.

## Problema e recorte

O teste88B construía a expectativa do convidado diretamente dos objetos que seu runner possuía. Um cliente real conhece somente o identificador público da sessão e precisa obter o contexto por uma autoridade autenticada. Acrescentar essa consulta ao mesmo backend e verificar seu resultado no cliente nativo existente elimina essa dependência do runner. Não acrescentar outro CLI ou lifecycle em SocialView.

`POST /v1/social/context` recebe somente `{sessionId}` no adaptador local explicitamente habilitado com `admission:true`. Autenticação, cadastro e leitura atual da sessão pelo token do chamador seguem as regras reais88A/B, sem Admin. Só uma política88B aberta, atual, do mesmo anfitrião/sessão/digest pode produzir contexto; conta anfitriã não consulta como convidada. Falta de política, mudança da projeção, bloqueio, sessão privada/inacessível e política indisponível retornam HTTP403 com somente `{code:"context_unavailable"}`, sem distinguir existência. Erros de autenticação e limites continuam próprios.

A consulta não reserva vaga, consome desafio ou autoriza transporte. `join` continua exigindo a prova88A e revalidação atual; obter contexto antes de um bloqueio não permite ingresso depois dele. O contrato continua uma vaga/public ou friends/open. Não há IP, fonte loopback, convite, token, perfil privado ou grafo social no resultado. Autenticar primeiro, ler a sessão autorizada antes de procurar a política RAM; capturar frescor antes dessa leitura e conferir a mesma referência de política ainda aberta após cada espera e assinatura. Devolver cópias, sem alias mutável do estado interno.

## Objeto fechado

Resposta `{body,signature}`, assinada pela mesma chave Ed25519 de admissão88B, com domínio `aurora/connect/social-join-context/v1` terminado por byte zero e codificação canônica u16BE por campo, na ordem:

`schemaVersion`, `purpose`, `issuerInstance`, `recipientAccountId`, `sessionId`, `sessionDigest`, `hostAccountId`, `hostPublicKey`, `hostOperationId`, `hostGeneration`, `policyId`, `policyDigest`, `issuedAtUnixMs`, `expiresAtUnixMs`.

Valores fixos: schema1, purpose=`socialJoinContext`; UIDs/UUIDs/hex/inteiros usam os mesmos validadores88B. Destinatário precisa ser diferente do anfitrião. Máximo8192bytes. Vencimento de no máximo10000ms, limitado também pelo menor prazo civil/monotônico original da política. Leitura, digest e assinatura não reancoram prazos; falha, cancelamento, reserva concorrente ou mudança de política antes da resposta impedem publicação do contexto. Limites de entrada/conta e consulta de5s existentes88B se aplicam. Não manter token/Request após a operação.

O convidado verifica assinatura/pin provisionado fora do renderer, conta própria autenticada e sessão selecionada independentes do objeto, esquema fechado e prazos. Congela a expectativa antes de produzir a prova. O protocolo do helper existente passa a aceitar `{context:<objeto assinado>}`; o antigo `{expect:...}` cru é recusado. Contexto não é renovável nem transfere mais tempo ao preparo. Sua validade é rechecável até o ingresso; após admissão válida, a continuidade segue a lease88B e o prazo original da sessão, sem depender de renovar contexto.

Enquanto aguarda desafio ou grant, o cliente usa o menor deadline original entre preparo e contexto e encerra sozinho se vencer, inclusive com stdin aberto. Não espera os60s de preparo para reconhecer um contexto de10s vencido. A assinatura da prova é seguida de nova checagem antes da publicação.

Após revogação, shutdown aguardado e drop dos recursos próprios, o harness confirma liberação de todas as portas em um mesmo ciclo de bind, mantendo os binds de teste até terminar esse ciclo. Somente `AddrInUse` permite nova sondagem, cada20ms, sob um único deadline de até1s; outros erros falham imediatamente. Não usa reuseaddr, outra porta ou reabertura de transporte. Preserva primeira/última observação e número de tentativas; timeout continua `cleanup=false`/falha. Essa janela de confirmação é separada da validade da lease e não é garantia de startup ou SLA. O runner termina sua própria extremidade TCP half-open depois de observar independentemente a retirada, antes de aguardar a confirmação de rebind do peer.

## Verificação e limites

Testes do backend cobrem contexto público/amigos sob regras locais, assinatura/adulteração, conta errada, bloqueio/restrição/ausência, política reservada, concorrência, rollback civil/cancelamento/prazos/limites. Testes nativos cobrem pin/conta/sessão, expiração, campos extras e recusa do protocolo cru anterior. A integração real reutiliza Auth/Firestore/SDK e dois processos88B, obtém contexto por HTTP e continua até TCP e encerramento; o runner não constrói a expectativa com campos do anfitrião.

Essa consulta ainda não fornece entrega host↔guest de grant/convite/lease, bootstrap nativo da autoridade, worker social no mesmo ConnectManager, troca de conta ou UI final. **Entrar gráfico permanece desabilitado** até essa integração. Sem nova prova externa, Minecraft, voz, relay ou nove pares. Sem push/deploy/instalador/configuração pessoal/gasto. Rollback reverte somente88C e recolhe seus próprios testes.
