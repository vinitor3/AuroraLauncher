# Aurora Connect — vínculo entre conta e chave, incremento 88A

Data: 25/10/2026. Estado: **PROTÓTIPO**, **IMPLEMENTADO MAS NÃO HOMOLOGADO** para publicação. A [ADR-030](engineering/ADR/ADR-030-social-account-key-binding.md) é o contrato; o [registro da tarefa](task-aur-r4-connect-social-88a.md) contém comandos, resultados, falhas e pendências. Complementa a [experiência social](aurora-connect-social-experience.md) e o [incremento 87](aurora-connect-social-87.md), sem fechar R4-005/008.

## O que foi acrescentado

O catálogo sabe qual conta Aurora pode ler uma sessão. O transporte conhece chaves públicas. O incremento 88A acrescenta uma prova limitada de que a conta autenticada possui a chave privada conservada em um preparo nativo específico. Essa prova liga conta, chave, operação, geração, papel, sessão, conteúdo da sessão e prazo original.

O recibo resultante tem `transportAuthorized:false`. Não é um convite nem uma autorização de ingresso, não é aceito pelo transporte e não confirma o laboratório manual. **Entrar permanece desabilitado.** Não há novo fluxo gráfico ou novo teste de jogo neste incremento.

## Fluxo e fronteiras

1. O preparo nativo conserva a chave privada em RAM e fornece somente a identidade pública, operação, geração e vencimento originais.
2. A autoridade recebe uma solicitação fechada com `schemaVersion`, `operationId`, `generation`, `role`, `publicKey`, `sessionId` e `preparedExpiresAtUnixMs`. A conta vem da autenticação do solicitante, nunca de um UID escolhido no corpo.
3. O leitor consulta o perfil Aurora, o registro do nick e a sessão com o token do próprio solicitante, sob as regras Firestore existentes. O papel host exige propriedade da sessão; guest exige outra conta e leitura autorizada.
4. A autoridade retém um desafio efêmero. Seu prazo é o menor entre 60 segundos, o vencimento original do preparo e o vencimento da sessão; espera e retry não renovam esse orçamento.
5. O comando nativo estreito confere o desafio contra seu próprio Draft e emite no máximo uma prova naquela tentativa. Não abre sockets, inicia jogo ou altera a fase.
6. A autoridade consome o desafio antes da primeira espera de verificação, autentica novamente o solicitante, confere a assinatura e repete cadastro, leitura autorizada e digest da sessão.
7. Somente uma prova ainda válida produz o recibo de vínculo. Mudança de conta, conteúdo, visibilidade ou bloqueio, expiração, cancelamento e replay impedem esse resultado.

Uma leitura permitida é evidência da visibilidade naquele instante. Ela não reserva vaga, comprova que o mundo está acessível nem autoriza uma sessão futura. A etapa de ingresso deverá estabelecer seu próprio contrato e reconferir política e autorização atuais.

## Codec e recibo

O desafio é JSON plano, fechado e limitado a 2048 bytes. Campos na ordem canônica:

```text
schemaVersion, purpose, issuerInstance, challengeId, accountId,
operationId, generation, role, publicKey, sessionId, sessionDigest,
issuedAtUnixMs, expiresAtUnixMs
```

`schemaVersion=1`, `purpose=accountKeyBinding`; IDs de operação/sessão são UUID v4 canônicos. Instância da autoridade, desafio, chave pública e digest usam hexadecimal minúsculo de 64 caracteres. UID tem de 1 a 128 caracteres ASCII alfanuméricos, `_` ou `-`. Geração e timestamps são inteiros positivos seguros em JavaScript; o papel é a string literal `host` ou `guest`.

A assinatura Ed25519 cobre o domínio UTF-8 `aurora/connect/social-key-proof/v1`, um byte zero e cada campo com comprimento UTF-8 em u16 big-endian. Números usam decimal canônico. O mesmo codec é implementado em Rust e TypeScript; não existe endpoint para assinar bytes ou digest arbitrários. A prova é o objeto fechado `{schemaVersion, challenge, signature}`, com assinatura de 128 caracteres hexadecimais minúsculos e limite nativo de 2304 bytes.

O digest da sessão usa o domínio `aurora/connect/social-session/v1`, um byte zero e a projeção normalizada em ordem fixa. Inclui ID, dono, título, modpack, versão, loader, imagem permitida, população declarada, visibilidade, política de entrada e vencimento. A população declarada continua distinta de jogadores realmente conectados.

O recibo contém `bindingId`, `accountId`, `operationId`, `generation`, `publicKey`, `sessionId`, `sessionDigest`, `role`, `expiresAtUnixMs` e `transportAuthorized:false`. Não contém senha, token, chave privada, endereço de transporte ou grafo social. Não é uma credencial assinada de ingresso e não possui consumidor nativo de admissão.

## Ciclo de vida nativo

`connect_prove_social_key_binding` aceita apenas a janela `main` e permanece indisponível no build sem `connect-lab`. A chave pertence ao mesmo `ConnectManager/Attempt` já coberto pelo encerramento da tentativa, troca de conta e saída. Só um Draft manual `sessionGameTcp` em loopback, ainda em `hostPrepared` ou `guestPrepared`, é elegível. Preparo já revisado, outra operação/dono/geração/chave/papel, perfil legado, fragmentado, grupo ou modo externo é recusado.

A assinatura ocorre sob o lock do manager, serializada com stop/revisão. O prazo civil do desafio não pode superar o preparo; o prazo monotônico original também é reconferido. A prova não renova a preparação nem consome uma autorização manual de transporte.

O helper `aurora-connect-key-proof` gera uma chave efêmera, imprime somente sua identidade pública, recebe uma linha limitada e emite uma única prova. Tem orçamento original de 60 segundos, sem rede ou persistência própria. Ele prova interoperabilidade do codec; não substitui o teste do manager nem cria uma sessão Connect.

## Autoridade e laboratório local

O núcleo em `apps/edge-proxy/src/connectSocialBinding.ts` recebe explicitamente autenticação e leitura confiáveis. Não foi adicionado ao roteador de produção e não altera a verificação RS256 existente do Worker.

O adaptador separado em `firebase/tests/social-binding-server.mjs` exige ativação explícita e fixa:

| Item | Limite |
| --- | --- |
| Projeto | `demo-aurora-social` |
| Auth Emulator | `127.0.0.1:9187` |
| Firestore Emulator | `127.0.0.1:8187` |
| HTTP da autoridade | Somente `127.0.0.1`, porta 42888 ou porta efêmera no teste |
| Rotas | POST `/v1/social/key/challenge` e `/v1/social/key/proof` |
| Corpo HTTP | JSON, até 4096 bytes, sem compressão |
| Resposta de upstream | Até 65536 bytes, sem seguir redirecionamentos |
| Pendências | Até 128 globais e quatro por UID; não expulsa desafio vivo para aceitar outro |
| Recibos retidos | Até 128, expiração original |
| Frequência | 512 tentativas globais/minuto e 20 por UID/minuto, por processo |
| Consulta/autenticação | Até 5 segundos por operação, com cancelamento |

O servidor confere o header Host contra seu endereço local e rejeita Origin; não há CORS genérico nem fetch direto liberado para uma WebView. Encerramento prematuro da conexão propaga cancelamento ao núcleo. Respostas atrasadas não publicam desafio/recibo após o cancelamento; desafios em verificação são consumidos também quando a verificação falha.

O Auth Emulator usa tokens sem assinatura (`alg=none`) por definição do ambiente de teste. Esse comportamento está isolado no adaptador local, que também consulta a conta no emulador e confere projeto, UID e prazos. **Não demonstra autenticação criptográfica de produção e não cria fallback para tokens sem assinatura em produção.** Os limites por processo não são um orçamento distribuído, proteção de faturamento ou homologação de serviço público.

## Evidências e limites

| Camada | Resultado registrado |
| --- | --- |
| Autoridade TypeScript | 19 testes PASS, 232,6318 ms |
| Integração local real | 8 testes PASS, 5197,8999 ms; Auth/Firestore reais no emulador, duas contas Aurora descartáveis, uma conta Auth sem cadastro e helper Rust real |
| Biblioteca Rust | Quatro testes de prova PASS; compilação inicial do lote em 4 min 19 s |
| Helper CLI Rust | Dois testes PASS; processo e entrada limitada, sem transporte |
| Manager desktop | Quatro testes PASS após correção de visibilidade Rust; compilação em 2 min 08 s, execução focal em 0,02 s |
| Regressões da interface | 262 testes DOM/lifecycle PASS, 6714,6165 ms; não é novo ensaio GUI |
| Formatação | PASS após ajuste de ordem de imports/módulos |
| Worker dry-run | PASS, Wrangler 4.127.0, 152,42 KiB; sem deploy |
| Suíte completa edge | 174 PASS, 9743,1044 ms; inclui os 19 focais, sem soma duplicada |
| Clippy desktop | Todos os targets/features PASS, 31,61 s; perfil default com todos os targets PASS, 9,84 s |
| Build frontend | PASS, 378 módulos, 2,76 s; aviso de chunk maior que 500 kB preservado |

A integração cobre prova host/guest, UID diferente, replay, alteração de conteúdo, restrição público→privado, bloqueio posterior, ausência de cadastro Aurora e recusas HTTP. A revisão independente exercitou cancelamento em memória: solicitação já abortada e abort durante a última leitura retornaram `binding_cancelled`; no segundo caso havia zero pendências e zero recibos.

A rodada final após o ajuste de estilo repetiu os quatro testes da biblioteca e dois CLI (PASS), além dos oito testes da integração real (PASS, 1269,6721 ms). Clippy connect-lab passou em 13,90 s; os recursos próprios foram encerrados e as portas verificadas livres. Pins e falhas anteriores permanecem separados no registro da tarefa. A compilação bem-sucedida não promove transporte, jogo, voz, GUI nova ou build de produção a homologados.

## Situação por componente

| Componente | Efeito de 88A |
| --- | --- |
| TCP | Nenhuma nova prova de transporte; implementação/evidências anteriores conservadas |
| UDP | Nenhuma alteração ou nova prova de datagramas |
| Conexão direta | Nenhum novo caminho ou diagnóstico de rede; loopback aqui limita a prova nativa |
| Relay | Não usado, habilitado ou contratado; nenhuma nova validação de termos |
| Minecraft | Não iniciado nem conectado por 88A; nenhuma nova evidência de mundo/jogadores |
| Convites/interface | Prova estreita disponível só ao laboratório; Entrar continua desabilitado e os fluxos manuais não são autoaprovados |
| Amigos/catálogo | Funcionalidades de 87 preservadas; regressões locais executadas |
| Simple Voice Chat | Sem novo teste de áudio, endereçamento ou recuperação |
| Plasmo Voice | Sem novo teste de áudio, endereçamento ou recuperação |

O próximo contrato deverá ligar política de hospedagem revisada, fonte nativa, host da sessão, autorização individual atual e troca interna da autorização de transporte. Isso ainda não foi implementado por 88A. Pedidos de entrada, participantes dinâmicos, revogação ativa, escolha de caminho e homologação entre redes permanecem separados, sem alterar a prioridade do roadmap.

## Rollback

Reverter apenas as alterações do incremento 88A e interromper seus próprios helpers/autoridade local. Preservar os dados e worktrees anteriores, o documento de proposta de voz não rastreado e os registros privados de falhas. Nenhuma configuração pessoal, regra remota, firewall, credencial de produção, push, deploy, instalador ou release faz parte deste incremento.
