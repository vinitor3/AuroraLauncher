# Orientação de próximos passos no painel Connect

Atualização: 24/10/2026. Estado: **PROTÓTIPO**, com `connect-lab` desligada por padrão. Base de trabalho: `208edc9`. O painel acrescenta orientação em português e contagens de prazos já emitidos. Não acrescenta transporte, autoridade ou comando Tauri. A entrega foi incorporada ao executável de teste e observada nos dois papéis da interface com transporteTCP real local; os limites dessa prova constam abaixo. O contrato continua em [Connect desktop](aurora-connect-desktop.md) e [sessão TCP](aurora-connect-desktop-session-lifetime.md).

## Orientação apresentada

O cartão **Próximo passo** fica antes do estado observado. A ação principal deriva do último snapshot validado, papel e fase. Carregamento, ausência de capability, resposta incerta, observação desatualizada e operação pendente têm mensagens próprias. Copiar um código, editar um campo ou exportar um convite não cria uma fase observada nova.

Quando a disponibilidade do recurso não pôde ser confirmada, o guia oferece somente observação e encerramento da tentativa identificada. Explica que Atualizar estado consulta o snapshot, sem repetir a consulta de capability. O exemplo de preparação/autorização fica oculto e a contagem da revisão usa o título descritivo Revisão observada. Depois da liberação confirmada, o próximo passo é reabrir o aplicativo para verificar a disponibilidade do Connect; o cartão não promete recuperar os controles por atualização de estado.

O exemplo explicitamente identificado de um anfitrião e um convidado explica o caminho completo no launcher:

1. Anfitrião prepara e envia seu código público; convidado prepara com esse código e devolve o próprio código.
2. Anfitrião recebe o código do convidado, revisa, confere a confirmação e autoriza; só depois copia e envia o convite.
3. Convidado cola o convite destinado à sua tentativa, revisa, confere a confirmação e autoriza.
4. Com a ponte e seu endereço observados, convidado copia o endereço para Multiplayer → Conexão direta no Minecraft compatível.

São instruções para ações explícitas nos controles existentes, sem PowerShell. O guia não revisa, autoriza, exporta, copia, consulta STATUS ou dispara conexão automaticamente. Admissão, ponte, STATUS e entrada no mundo permanecem fatos diferentes; o texto não promove nenhum deles a áudio ou jogador no mundo.

Para grupo e perfis com UDP, o próximo passo remete às revisões e controles existentes. O cartão não representa o grupo como um convite único. Preparações próprias, respostas perdidas, recuperação UDP e encerramento tardio conservam as autoridades e proteções anteriores. Atualizar estado e Encerrar Connect permanecem no estado observado, inclusive após resposta incerta quando existe operação identificada; um identificador desconhecido não é inventado para cancelar.

## Relógios e expiração

As contagens vêm exclusivamente dos instantes existentes no snapshot:

| Contagem | Origem |
| --- | --- |
| Preparação | `preparation.expiresAtMs`, preparação do grupo ou prazo observado de `hostPreparing`. |
| Revisão para autorizar | `review.expiresAtMs`, inclusive a revisão própria do grupo. |
| Prazo para entrar | `sessionTiming.admissionExpiresAtMs` dos perfis com prazos separados. |
| Encerramento da sessão | `sessionTiming.sessionExpiresAtMs` dos mesmos perfis. |
| Convite e conexão | Prazo compartilhado real do perfil curto, obtido da revisão ou do snapshot ativo. |

Os relógios individuais dos participantes continuam no painel de grupo existente; o cartão superior não agrega nem inventa um encerramento comum. Os horários civis existentes permanecem exibidos. O timer de apresentação atualiza a cada segundo enquanto o componente está montado e é removido ao desmontar; não produz comandos nativos nem adiciona duração ao relógio atual.

Se o relógio civil local alcança o prazo de preparação ou revisão individual, a interface desabilita somente a ação vencida de revisar/autorizar e limpa a confirmação. Os handlers também conferem `Date.now()` no clique, cobrindo a passagem do prazo antes do próximo tick. Encerramento e observação continuam disponíveis. Recuar o relógio não restaura a confirmação já limpa. O backend continua sendo a autoridade sobre prazos originais, consumo e recursos; a interface pede encerrar e preparar novamente, sem renovar a tentativa.

No fim do ingresso, o convite exportado sai do painel e a exportação fica indisponível, sem republicação. Uma resposta de exportação recebida depois do seu vencimento não é apresentada nem copiada. A expiração de ingresso não é descrita como fim de sessão de um convidado cuja admissão foi observada. O estado nativo e o prazo absoluto de sessão continuam separados.

## Validação local e limites

A validação automatizada inicial usa fixtures DOM sintéticas, reutilizando o mesmo transporte simulado e os builders de snapshot anteriores; elas não acessam Java, conta, rede, clipboard real ou configuração pessoal. Os casos novos cobrem sequência de botões de ambos os papéis, ausência de ações automáticas, confirmação pendente/cancelamento/resposta tardia, observação desatualizada, revisão incerta, contagens distintas e cleanup do timer, revisão/preparação vencidas no clique, timeout/consentimento/rollback do relógio, exportação vencida e perfil de grupo.

Uma primeira rodada focal teve 9 PASS/1 falha por seletor DOM `[readonly]`: Linkedom não reflete esse atributo no textarea de React. Corrigiu-se o oráculo para selecionar o textarea realmente exibido. A rodada seguinte de dez testes passou; dois casos adicionais de incerteza e timeout foram acrescentados antes do fechamento. Resultados finais e freeze constam no registro desta entrega e em `.aurora/connect-validation/desktop-ui-guidance-source-freeze.json`, fora do Git.

Primeiro fechamento local: `node --test apps/desktop/src/features/connect/ConnectView.smoke.mjs` passou **203 PASS/0 falhas/0 ignorados**, 4,034s, com 12 casos novos naquela revisão. O arquivo anterior tinha 191 testes; o total histórico de 241 incluía outros arquivos. `node node_modules/typescript/bin/tsc -p tsconfig.app.json --noEmit --incremental false`, executado em `apps/desktop`, passou em 6,375s, sem escrever assets. Logs preservados: `desktop-ui-guidance-full-final.log` e `desktop-ui-guidance-ts-final.log` em `.aurora/connect-validation`. A execução anterior do mesmo smoke passou 201 testes antes dos dois casos finais; não somar rodadas como provas distintas.

A revisão independente encontrou a orientação incorreta de repetir atualização de estado após falha de capability. O ajuste acima acrescentou um único teste, reutilizando a fixture existente de erro de capability com snapshot revisado. Ele comprova ausência de instruções e controles para autorizar/copiar, preservação da barreira após refresh sem retry de capability, uma invocação de stop e cleanup confirmado. Foco **1 PASS/0 falhas**, 1,034s; novo fechamento do smoke **204 PASS/0 falhas/0 ignorados**, 3,070s, incluindo **13 casos novos**; TypeScript sem emissão passou em 5,620s. Logs próprios preservados: `desktop-ui-guidance-capability-{focused,full,ts}.log`. Freeze atual: `.aurora/connect-validation/desktop-ui-guidance-capability-source-freeze.json`; o freeze inicial permanece como registro da revisão anterior.

O checkpoint de fixtures acima, sozinho, não executou GUI real, Minecraft ou nova prova de transporte/voz/rede externa. Defaults, papéis iniciais, modos, perfis, DTOs, comandos, gates, prazos e nove pares Core/Skins permanecem sob os contratos atuais. Build e integração nativos posteriores foram executados separadamente após o freeze das fontes.

## Executável e observação gráfica reais24/10

O novo build do perfil isolado passou com Vite373módulos/1,54s e nativo `dev`21,21s, sem bundle ou feature de voz. O guia foi observado no executável próprio64692224B, SHA256 `8a535bd65131ce93c2a1df9990a7b10067019ec05ec36dda8cddaad3a108ba2c`, em `.aurora/connect-validation/connect-ui-guide-24/AuroraConnectLab.exe`. Não é instalador ou release. Revisão independente final do guia/helper sem bloqueadores.

Na interface real, o responsável exerceu explicitamente os controles existentes, sem aprovação automática: anfitriãoGUI/convidadoCLI e, separadamente, anfitriãoCLI/convidadoGUI. Ambos observaram o cartão, revisão, confirmação e admissão originais; cada caso transferiu1048576B em cada direção com conteúdo/EOF conferidos e encerrou Connect até **Recursos liberados**. O primeiro registrou admissão74ms; o segundo observou caminho direto e portaTCP efêmera53190. Os processosCLI próprios terminaram exit0/recolhidos/sem força, echo/probe foram fechados e o helper retornou `clean=true`. Seus três rebinds verificam somente **UDP42000, TCP42010 e TCP43001**; a ponteGUI53190 do segundo caso tem encerramento observado no estado nativo, **sem rebind explícito de53190 registrado**. O rebind fixo42010 não comprova essa porta efêmera e não foi acrescentado postcheck retrospectivo aos relatórios.

Resultados privados `gui-host-case3.json` e `gui-guest-case4.json`, no diretório do executável, conservam os hashes e limites descritos no [registroGUI](aurora-connect-ui-lab.md). As falhas preparatórias1/2 permanecem falhas. A troca de executável exigiu encerrar forçadamente duas instâncias antigas próprias já sem Connect ativo; saída normal do launcher não foi comprovada. A nova janela continua aberta com Connect encerrado, não processo completo encerrado.

O comando combinado de três arquivosDOM, `node --test src/features/connect/ConnectView.smoke.mjs src/features/connect-game-launch/ReadyGameLaunch.smoke.mjs src/features/connect/account-transition.smoke.mjs`, na pasta desktop, passou **254PASS/0falhas/0skip**,3742,3289ms, log `desktop-ui-guidance-combined.log`. Não somar204+254 como testes distintos:204 pertencem ao painel, incluindo13 novos, e50 aos outros arquivos existentes. TypeScript sem emissão e os checks anteriores mantêm suas próprias rodadas registradas acima.

O guia deste painel explica o laboratório; a [experiência social exigida](aurora-connect-social-experience.md) prevê **Entrar** para amigos/sessões públicas sem convite manual conforme a política, ainda **NÃO IMPLEMENTADO / PRECISA DE TESTE**. Os casos3/4 descritos nesta seção não executaram MinecraftSTATUS; o ensaio posterior separado está no [domínioGUI](aurora-connect-ui-lab.md). Eles também não executaram mundo, UDP de aplicação, áudio, outra internet ou fluxo social e não homologam essas capacidades.
