# Aurora Connect — implementação

Estado em 10/11/2026: em andamento, experimental. Código local `5028a01`; matriz original de nove pares preservada.

| Área | Estado observado |
|---|---|
| TCP / UDP | NativeVoice8 assinado: oito participantes locais, 50 datagramas/s por pessoa, TCP íntegro de 2 MiB; cinco ensaios consecutivos passaram. Limites antigos preservados. |
| Direto | Evidência externa IPv6 anterior preservada; cobertura IPv4/NAT depende da validação de rede. |
| Relay / online | Worker gratuito ativo; Firebase sem faturamento. Contrato de voz 5 passou em autenticação, horário, política, disponibilidade, handshake e limpeza reais. Cotas do relay inalteradas; não comportam oito vozes no pior caso. |
| Amigos / grupos / pedidos | Busca por nick Aurora, catálogo, entradas autorizadas, grupos e remoção individual. Pedido pendente mostra Verificar pedido; consulta não reenvia nem entra automaticamente. |
| Jogo | Nove perfis instalados/inspecionados. GUI Fabric 1.21.1 reutilizou a cópia, entrou com Core/Skins pelo HostPlayer Social e permaneceu 191s até saída deliberada, exit0. Não homologa duração longa nem nove clientes. |
| Simple Voice Chat | Nove servidores oficiais passaram em preparo/STATUS/UDP/encerramento. Cliente real autenticou e validou UDP pela ponte; áudio permanece não homologado. |
| Plasmo Voice | Oito servidores oficiais passaram em preparo/STATUS/UDP/encerramento após corrigir o prazo de salvamento V3 para 30s. Áudio não homologado; não há versão oficial Forge 1.12.2. |
| Interface | Preparo, revisão, entrada, Stop, retirada do anúncio e recuperação passaram pela GUI. UDP reservado ao Social agora indica onde acompanhar a conexão; 17 testes DOM e TypeScript passaram. |

Verificações: 337 testes DOM, 107 backend e sete focais/regressões nativas, TypeScript, fmt/Clippy e revisões independentes passaram. Hash otimizado: inspeção de 41s para 7s, 4.290 arquivos intactos e debug_assertions preservado. `connect:online:build` passou, sem instalador. Registro terminal limitado a 4 KiB, sem identificadores/segredos, fora do mutex; cobre `finish`, não expiração anterior à sessão/JoinError nem saída abrupta.

Queda anterior preservada: HostPoll 3.009ms, limite de 3s mantido. Update autorizado `6758a55`: leituras frescas paralelas, expiração inválida recusada; probe online 52/52 em 30,3s, máximo HTTP 1.051ms/idade 2.204ms. `ecc3dd7` corrige reutilização após JNA somente com DLL/marcador correspondentes ao JAR oficial fixado: seis focais, 24 regressões, inspeção real, Clippy/fmt/QA e GUI passaram. No exe `34d99fba…`, mundo e controle SVC permaneceram até saída normal; processos/portas liberados, EULA/anúncio restaurados. Áudio e rede externa não foram testados nesta rodada. Voz sob carga mantém TCP a 192 kB/s até 200ms sem UDP.

Comandos: Cargo test/check/clippy, `connect:online:build`, Wrangler dry-run, Node/emuladores/probe HTTP nativo. Update único via API Cloudflare com hash aprovado. Evidências em `.aurora/connect-validation/`: `social-locator-build-20261110/deployment.json`, `native-host-poll-248eadc1-d020-459c-8384-a0992d399eb6.json`, `jna-reuse-fix-20261110.json`, `gui-jna-reuse-20261110-readonly.json`; falhas preservadas. Build posterior do texto UDP `4d662b30…` passou; sem push/release/instalador.

Produção: `aurora-connect-social.aurora-edge-proxy.workers.dev`, bundle `c86db950…`, versão `38d190c1-e163-455a-9b06-069b80f1f5aa`; regras `20aeaa01-996b-4040-9acd-8b6cfe4eebb7` inalteradas, bindings/limites/plano preservados. Rollback: versão `042b42ba-43a0-4253-b82b-79ae8b621e43`, bundle `80f791dc…`; snapshots antes/depois em `social-locator-build-20261110`. Código: reverter somente commits próprios. Configurações pessoais preservadas. Matriz: 18/18 JARs e assinaturas conferidos anteriormente; isso não homologa mundos/áudio nem promove a instalador.
