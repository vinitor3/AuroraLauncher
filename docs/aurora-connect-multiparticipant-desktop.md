# Grupo de participantes no painel — incremento72

Estado: **PROTÓTIPO / PRECISA DE TESTE**. Integra a [biblioteca71](aurora-connect-multiparticipant-session.md) ao slot experimental Connect. Interface e parsers possuem provas locais abaixo; validação nativa e integração final ainda estão em andamento. Não há homologação de GUI, rede externa, jogo ou áudio por esta entrega.

## Escopo

O anfitrião prepara um grupo estático de dois a oito convidados. Cada convidado conserva o envelope individual `SessionInvitation` schema3 e usa o fluxo [TCP+UDP62](aurora-connect-desktop-fragmented-session.md), inclusive os modos diretos66 e a recuperação UDP manual63. Não há novo formato de convite, convite coletivo, inferência de perfil pelo JSON ou mudança nos quatro perfis existentes.

O novo fluxo de anfitrião tem capability e projeção próprias. Game/TCP e aux/UDP são destinos manuais loopback explicitamente revisados. A descoberta64 permanece disponível nos perfis anteriores, mas não é incorporada implicitamente ao grupo. Não há adição de participantes em execução, renovação, reingresso depois de consumo, conta/social, persistência de chave ou configuração automática de voz.

O grupo ocupa o mesmo slot do `ConnectManager`. A janela principal continua sendo a proprietária. Preparação simples, grupo e convidado não podem coexistir nesse slot. Stop, saída e logout usam o fechamento aguardado existente e encerram todos os recursos Connect próprios, sem encerrar Java ou restaurar configurações de voz automaticamente.

## Capability e comandos

Novo campo opcional em `ConnectCapabilities`, omitido quando indisponível:

```ts
sessionFragmentedHostGroups?: {
  profile: "sessionFragmentedGameUdp";
  invitationSchemaVersion: 3;
  minParticipants: 2;
  maxParticipants: 8;
  maxPreparationSeconds: 600;
  maxReviewSeconds: 60;
  maxAdmissionSeconds: 600;
  maxSessionSeconds: 3600;
  ownedGameTcpDiscovery: false;
}
```

Esse campo exige a capability schema3 anterior, com seu contrato UDP completo. Os modos permitidos continuam sendo a lista anunciada em `capabilities.modes`; não existe lista paralela, fallback ou mudança de modo ao interpretar convite. Ausência da nova capability conserva o comportamento62–70, sem botão de grupo.

Comandos novos, todos exclusivos do anfitrião e com `deny_unknown_fields` nos DTOs:

```ts
connect_prepare_session_group_host({ request: {
  mode: "loopback" | "externalIpv6" | "directIpv4";
  bindAddress: string;
  gameTarget: string;
  udpTarget: string;
}}): GroupPreparation;

connect_review_session_group_host({
  operationId: string;
  participants: Array<{
    guestPublicId: string;
    admissionSeconds: number;
    sessionSeconds: number;
  }>;
}): GroupReview;

connect_confirm_session_group_host({
  operationId: string;
  digest: string;
}): ConnectSnapshot;

connect_export_session_group_invitation({
  operationId: string;
  participantId: string;
}): GroupInvitationExport;

connect_revoke_session_group_participant({
  operationId: string;
  participantId: string;
  expectedGeneration: number;
}): ConnectSnapshot;
```

`connect_stop` e `connect_snapshot` existentes continuam sendo os comandos de encerramento e observação. Comandos simples de revisar/confirmar/exportar não aceitam uma operação de grupo, e os novos comandos não aceitam operação simples ou de convidado. Solicitações ausentes de feature não criam identidade, socket ou timer.

Preparar valida os destinos e cria somente a identidade efêmera e o rascunho, sem bind/transporte. O prazo próprio de600s permite receber as identidades públicas dos convidados; não amplia o prazo60s das preparações anteriores. Após o preparo, cada convidado prepara o perfil schema3 usando o peer do anfitrião. O anfitrião então informa a lista completa e a revisa uma única vez. IDs de participante são opacos, gerados nativamente e exclusivos da operação; não são conta, nome Minecraft ou índice de linha.

Tempos são segundos inteiros: ingresso1–600, sessão1–3600 e sessão≥ingresso. O nativo recusa peers repetidos, o peer do próprio anfitrião, lista fora2–8 e qualquer erro parcial sem publicar grupo incompleto. Emissão cria cada autoridade uma vez; polling, exportação e repetição de revisão não reemitem convites nem renovam clocks. Uma preparação já revisada não aceita lista diferente: exige encerrar e preparar outra operação.

## DTOs e revisão

```ts
type GroupPreparation = {
  operationId: string;
  generation: number;
  publicId: string;
  expiresAtMs: number;
  kind: "staticSessionFragmentedHost";
};

type GroupParticipant = {
  participantId: string;
  guestPublicId: string;
  invitationDigest: string;
  sessionTiming: SessionTiming; // perfil schema3, sem adaptar schema2
  consumed: boolean;
  transportActive: boolean;
  revoked: boolean;
  admissionValid: boolean;
  sessionValid: boolean;
};

type GroupReview = {
  operationId: string;
  generation: number;
  digest: string;
  kind: "staticSessionFragmentedHost";
  mode: ConnectMode;
  hostPublicId: string;
  directAddress: string;
  gameTarget: string;
  targetSource: "manual";
  fragmentedUdp: FragmentedUdpReview; // inclui udpTarget manual
  participants: GroupParticipant[];
  expiresAtMs: number;
};

type GroupInvitationExport = {
  operationId: string;
  participantId: string;
  invitationDigest: string;
  invitation: string;
  hostPublicId: string;
  expiresAtMs: number; // ingresso deste participante
  sessionTiming: SessionTiming;
  fragmentedUdp: FragmentedUdpContract;
};
```

UUIDs, peers/hash64hex e inteiros seguros seguem os validadores existentes. Listas têm2–8 participantes, IDs/peers/digests únicos e ordem preservada após revisão. `SessionTiming` é exatamente a projeção schema3 anterior. `FragmentedUdpReview` e `FragmentedUdpContract` conservam original4096B/frame1264B, serviço aux/revisão1, retorno autorizado16384B/s e20fragmentos/s; somente a revisão local contém `udpTarget`. Game permanece TCP/reliableStream revisão1. Destinos privados não entram nos convites nem na exportação.

O digest de revisão usa domínio próprio e vincula operação, geração, modo, chave/endereço inicial, destinos TCP/UDP, perfil/serviços/orçamentos, ordem e identidade de todos os participantes, digest de cada envelope e ambos os prazos originais. Prazo da revisão é o menor entre60s, o restante dos600s originais do preparo e os ingressos individuais ainda válidos. UI mostra cada peer, ambos os vencimentos, os dois destinos e que a autorização vale para o grupo inteiro; checkbox novo é exigido antes da confirmação.

Confirmar verifica o digest vigente, proprietário, operação, lista inteira e todas as autoridades originais antes de abrir o endpoint. Após qualquer espera também revalida o conjunto. Falha não autoriza subconjunto e não tenta perfil manual. A biblioteca71 conserva uma autoridade por lease e rechecagem após bind. Erro ou perda de resposta IPC não disparam confirmação automática; o painel reconcilia pelo snapshot.

## Snapshot no slot existente

`ConnectSnapshot` recebe somente este campo opcional para operações de grupo:

```ts
hostGroup?: {
  kind: "staticSessionFragmentedHost";
  profile: "sessionFragmentedGameUdp";
  invitationSchemaVersion: 3;
  mode: ConnectMode;
  hostPublicId: string;
  preparation: GroupPreparation | null;
  review: GroupReview | null;
  participants: GroupParticipant[];
};
```

Snapshots antigos omitem `hostGroup`. Em grupo, `role="host"`; os campos simples `preparation`, `review`, `invitationProfile`, `sessionTiming`, `fragmentedUdp`, `udp` e `udpRecovery` permanecem ausentes ou nulos conforme seu contrato anterior. Não se serializa um convite arbitrário do grupo nesses aliases. Os parsers têm um ramo estrutural explícito de grupo; mistura com contexto simples/Guest é recusada. Ações de grupo exigem a nova capability, mas reconhecer a projeção para cleanup não exige outra consulta de capability: stop/logout devem continuar possíveis quando essa consulta falhar.

Se a consulta inicial de capability falhar, a interface ainda consulta e acompanha o snapshot nativo, permite atualizar e aguarda `connect_stop`. Não oferece preparação, revisão, confirmação, exportação, revogação ou consulta STATUS nessa condição. Em `reviewed`, os cinco indicadores individuais externos devem coincidir com a revisão original: não consumido, não revogado, transporte ainda inativo e ambos os prazos válidos. Endereço IPv6 na revisão exige literal canônico, porta1–65535 e a política conservadora nativa; a verificação sintática não consulta a rede nem comprova alcance.

As fases do slot continuam `hostPrepared`, `reviewed`, `starting`, `hostListening`, `closing`, `closed` e `failed`. `hostGroup.preparation` existe apenas em hostPrepared; `hostGroup.review` apenas em reviewed. Lista vazia identifica o rascunho ainda não revisado, inclusive ao encerrar ou falhar; após revisão conserva2–8 entradas também no fechamento. Durante starting não há exportação. HostListening/closing/closed conservam as observações individuais, sem reciclar revisão antiga. `expiresAtMs` externo é o prazo de preparo/revisão nessas fases, e nulo quando o grupo está ativo ou encerrando: não representa um prazo coletivo que possa encerrar B ao expirar A.

Em closing/closed/failed, indicadores individuais de validade e transporte ativo são false; consumo e revogação permanecem históricos. Falha normal concluída é `failed + cleanupState=complete + transportState=closed`. Se aguardar o worker falhar, `failed + cleanupState=failed` conserva o transporte observado (`notStarted`, `listening` ou `closed`), sem afirmar liberação de recursos. A interface explica essa incerteza, conserva a ação de encerramento e não libera o formulário de uma nova operação.

`consumed` vem da decisão cumulativa de ingresso do Host. Não indica conexão ativa ou jogador no mundo. `revoked` é a decisão individual retida pelo caller. `admissionValid` e `sessionValid` rechecados na autoridade original significam validade atual, inclusive clocks civil/monotônico; ambos são falsos quando revogado. Consumo não renova validade e não é apagado ao desconectar. No review os participantes estão não consumidos, não revogados e válidos. Na execução não há atributo de jogador, número de contas ou áudio disponível.

Extensão explícita72 da biblioteca: `transportActive` consulta a conexão por lease, retida sob o mesmo mutex de admissão, e exige autoridade válida, cancelamento ausente e conexão sem motivo de fechamento. Não deriva apenas de consumo ou do contador global. Só pode ser true para participante consumido, não revogado e com sessão válida em hostListening; no review, fechamento e estado encerrado é false. A interface o chama de conexão autenticada observada, nunca jogador conectado. A consulta é uma observação instantânea, não garante que a próxima operação de rede terá sucesso. Essa extensão não integra o commit71 nem altera sua prova histórica.

Servidor publicado indica somente listener autenticado disponível. Estado STATUS continua separado e não demonstra ingresso; a primeira entrega não acrescenta consulta STATUS de grupo nem usa um participante como proxy. `tcpAddress`/`statusSummary` ficam nulos, `statusState=notChecked`, mundo `notVerified`, ambos mods `disabled` e relay false. O Host não ganha bridge UDP de convidado, capacidade UDP negociada ou mecanismo de retry63.

## Exportação, revogação e encerramento

Exportação é permitida somente em hostListening para o participante selecionado ainda não consumido, não revogado e com ingresso/sessão atuais válidos. Retorna o envelope schema3 original, sem reemissão. A UI confere operação, participante, digest e metadados contra a linha selecionada antes de oferecer cópia. Mudança da seleção, geração, validade, fechamento ou perda de resposta limpa o texto exportado; resposta tardia nunca recoloca outro convite na seleção atual.

Revogação exige operação/participante vigentes e geração atual do snapshot. O comando chama `Host::revoke` somente para o lease correspondente, registra a decisão e limpa sua exportação. Repetição de revogação já concluída pode retornar a observação atual sem repetir efeitos, mas nunca ignorar proprietário/operação/participante. A UI pede essa ação por participante e informa que ela encerra seu transporte; não reaproveita o consentimento de confirmação para outro grupo. Nenhuma falha provoca revogação ou retry automáticos.

Ingressos e sessões são independentes. O monitor não encerra o grupo porque o primeiro prazo venceu: termina normalmente somente quando cada participante estiver revogado, com sessão inválida, ou nunca admitido e com ingresso inválido. Convidado consumido e desconectado não pode reutilizar o convite. Pode haver Host sem conexões até o último prazo autorizado, dentro do teto3600s, sem renovação.

Stop/logout/saída encerram e aguardam o Host compartilhado e todos os recursos próprios antes de liberar o slot. Respostas de confirmar/exportar/revogar iniciadas antes do fechamento não recriam estado ativo. Perda de snapshot válida mantém ações bloqueadas até observação consistente; não vale assumir sucesso, recriar identidades ou reaplicar comandos. A UI conserva seleção/epoch/operação/geração por ação, inclusive ao trocar de perfil, página ou participante.

## Provas e fronteiras

- DTO/capability/schema estritos, default desligado e snapshots simples antigos preservados; nenhuma mistura grupo/Guest ou grupo/perfis anteriores.
- Preparo/revisão sem destino I/O; confirmar abre um Host e dois Guests reais com autoridades/destinos originais. Cada Guest conserva fluxo62, incluindo falha UDP sem derrubar TCP.
- Exportação por participante sem cruzamento, recusa após consumo/expiração/revogação e atraso IPC/cópia/seleção. Alterar um peer/destino/prazo/digest invalida o consentimento do conjunto.
- Revogar A mantém B e sua autoridade original; expiração individual, duplicação, limites globais e encerramento/rebind. Prazo de A nunca vira timer do Host inteiro.
- DOM com duas linhas distintas, revisão conjunta, ações explícitas, stale responses, troca de página, stop/logout durante confirmação e revogação, resultado IPC perdido sem reenvio automático.
- Regressões dos perfis anteriores, descoberta64, UDP recovery63, voz60/68/70 e build; testes Tauri reais continuam separados de DOM controlado.

Interface: **117 PASS,0 falhas/ignorados**,2367,4747ms, em dois arquivos de testes. Inclui95 regressões anteriores e22 casos do grupo: preparo/revisão próprios, duas linhas/limite8, destinos e modos explícitos, consentimento, exportação individual e recusas cruzadas, revogação A mantendo B, consumo, ausência ou falha da consulta de capability com cleanup aguardado, respostas perdidas/atrasadas, encerramento incerto, expiração no clique, remount, flags divergentes recusadas, IPv6 inválido recusado, snapshots idênticos conservados e geração alterada invalidando consent/export. Fonte testa DOM/IPC controlados; não abre sockets, Java, conta ou clipboard real.

Build TypeScript/Vite: **CONCLUÍDO**,366 módulos, com aviso anterior de chunk>500kB. Logs privados finais `group-72-frontend-third.log` e `group-72-frontend-build-third.log`. A primeira rodada teve108/109 PASS: uma asserção usava atributo `readonly` que o DOM sintético não refletia; a verificação passou a conferir o convite realmente exibido. A segunda passou112/112; a terceira inclui cinco regressões da revisão independente. Não somar rodadas.

Comandos realmente executados pelo autor da interface:

```powershell
node --test apps/desktop/src/features/connect/ConnectView.smoke.mjs apps/desktop/src/features/connect/fragmented-types.test.mjs
npm --prefix apps/desktop run build
```

Arquivos da interface: `GroupHostPanel.tsx`, `group-types.ts`, `ConnectView.tsx`, `types.ts` e `ConnectView.smoke.mjs`, em `apps/desktop/src/features/connect`. Nenhum ajuste em App, estilos globais, manifests, lockfiles, matriz ou configuração pessoal. Uma UI controlada não comprova o getter de transporte nem a autoridade dos recursos reais.

Validação nativa final: foco grupo10 PASS e getter1 PASS; regressão desktop combinada399 PASS/61 ignorados, padrão279 PASS/5 ignorados; biblioteca padrão247 PASS/7 ignorados e relay local257 PASS/8 ignorados. Todas as rodadas finais sem falhas. Clippy desktop padrão/combinado PASS. O teste padrão foi corrigido após falha de compilação por uso indevido de Tokio opcional, sem mudar dependências. Comandos, tempos, logs e limites estão no [registro72](task-aur-r4-connect.md#incremento72--grupo-estático-no-launcher-com-observações-individuais). As provas nativas usam três agentes reais no mesmo PC, sem executar Java ou alterar o relógio Windows.

## Limites e rollback

As provas da interface são locais e controladas. Mesmo após testes de transporte, dois Guests sintéticos não homologam dois jogadores Minecraft, contas originais/offline juntas, Core/Skins, microfone, áudio SVC/Plasmo, redes externas, NAT/CGNAT ou relay. Nove pares existentes, Assistente desktop e roadmap permanecem preservados.

Rollback operacional: encerrar o grupo e aguardar cleanup completo, ocultar a capability nova e usar o fluxo individual em outra operação. Não converter grupo vivo em autorização manual. Código: revert local revisado do incremento após encerramento. Não há mudança de disco pessoal, instalador, push, deploy, billing ou release neste incremento.
