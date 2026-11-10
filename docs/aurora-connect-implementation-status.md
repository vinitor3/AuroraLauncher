# Aurora Connect — implementação

Estado em 10/11/2026: em andamento, experimental. Código local `1febd36`; matriz original de nove pares preservada.

| Área | Estado observado |
|---|---|
| TCP / UDP | NativeVoice8 assinado: oito participantes locais, 50 datagramas/s por pessoa, TCP íntegro de 2 MiB; cinco ensaios consecutivos passaram. Limites antigos preservados. |
| Direto | Evidência externa IPv6 anterior preservada; cobertura IPv4/NAT depende da validação de rede. |
| Relay / online | Worker gratuito ativo; Firebase sem faturamento. Contrato de voz 5 passou em autenticação, horário, política, disponibilidade, handshake e limpeza reais. Cotas do relay inalteradas; não comportam oito vozes no pior caso. |
| Amigos / grupos / pedidos | Busca por nick Aurora, catálogo, entradas autorizadas, grupos e remoção individual. Pedido pendente mostra Verificar pedido; consulta não reenvia nem entra automaticamente. |
| Jogo | Nove perfis instalados/inspecionados. GUI Fabric 1.21.1 entrou no mundo com Core/Skins pelo HostPlayer Social; desconectou após 18s, estabilidade pendente. |
| Simple Voice Chat | Nove servidores oficiais passaram em preparo/STATUS/UDP/encerramento. Cliente real autenticou e validou UDP pela ponte; áudio permanece não homologado. |
| Plasmo Voice | Oito servidores oficiais passaram em preparo/STATUS/UDP/encerramento após corrigir o prazo de salvamento V3 para 30s. Áudio não homologado; não há versão oficial Forge 1.12.2. |
| Interface | Abrir Minecraft focou o preparo, reutilizou a cópia e iniciou Java em 9s. Novo exe `c7e8f18e…`: preparo/Stop pela GUI e registro terminal no Doctor passaram; detalhes da queda também aparecem no painel de jogo. |

Verificações: 337 testes DOM, 107 backend e sete focais/regressões nativas, TypeScript, fmt/Clippy e revisões independentes passaram. Hash otimizado: inspeção de 41s para 7s, 4.290 arquivos intactos e debug_assertions preservado. `connect:online:build` passou, sem instalador. Registro terminal limitado a 4 KiB, sem identificadores/segredos, fora do mutex; cobre `finish`, não expiração anterior à sessão/JoinError nem saída abrupta.

Queda GUI anterior: HostPoll com autorização de 3.009ms, última consulta 1.756ms, incerteza 352ms; limite de 3s preservado. Correção `6758a55` paraleliza leituras frescas e rejeita expiração inválida; bundle `c86db950…` revisado, publicação aguardando autorização específica. Ganho real/estabilidade ainda não medidos. Voz sob carga limita TCP a 192 kB/s até 200 ms sem UDP. Dedicado salvo/encerrado, EULA/anúncios recuperados, processos/portas liberados; aviso OpenJDK cancelado pelo usuário.

Comandos executados: `cargo test/check/clippy --offline --locked`, `npm --prefix apps/desktop run connect:online:build`, Wrangler `deploy --dry-run`, testes Node e emuladores Firebase. Comandos completos/evidência local em `.aurora/connect-validation/connect-terminal-diagnostic-20261110.json` e `gui-terminal-diagnostic-20261110-result.json`. Não houve push, novo deploy, release ou instalador nesta etapa.

Produção: `aurora-connect-social.aurora-edge-proxy.workers.dev`, bundle `80f791dc…`, versão `042b42ba-43a0-4253-b82b-79ae8b621e43`; regras `20aeaa01-996b-4040-9acd-8b6cfe4eebb7`. Recuperação do update proposto: mesma versão e bundle em `.aurora/connect-validation/social-online-nativevoice8-build`; snapshot/plano em `social-locator-build-20261110`. Código: reverter somente commits desta etapa. Configurações pessoais preservadas; horário autenticado sem alterar Windows. `verify-mod-release-matrix.ps1` passou: 18/18 JARs correspondem ao código, hashes/assinaturas conferidos; isso não homologa mundos nem promove a instalador.
