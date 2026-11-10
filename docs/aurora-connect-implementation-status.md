# Aurora Connect — implementação

Estado em 10/11/2026: em andamento, experimental. Base local `2349320`; matriz original de nove pares preservada.

| Área | Estado observado |
|---|---|
| TCP / UDP | NativeVoice8 assinado: oito participantes locais, 50 datagramas/s por pessoa, TCP íntegro de 2 MiB; cinco ensaios consecutivos passaram. Limites antigos preservados. |
| Direto | Evidência externa IPv6 anterior preservada; cobertura IPv4/NAT depende da validação de rede. |
| Relay / online | Worker gratuito ativo; Firebase sem faturamento. Contrato de voz 5 passou em autenticação, horário, política, disponibilidade, handshake e limpeza reais. Cotas do relay inalteradas; não comportam oito vozes no pior caso. |
| Amigos / grupos / pedidos | Busca por nick Aurora, catálogo, entradas autorizadas, grupos e remoção individual implementados. |
| Jogo | Instalação real dos nove perfis e inspeção das cópias passaram, sem abrir Minecraft. Entrada no mundo continua com evidência própria. |
| Simple Voice Chat | Nove servidores oficiais passaram em preparo/STATUS/UDP/encerramento; áudio permanece não homologado. |
| Plasmo Voice | Oito servidores oficiais passaram em preparo/STATUS/UDP/encerramento após corrigir o prazo de salvamento V3 para 30s. Áudio não homologado; não há versão oficial Forge 1.12.2. |
| Interface | Revalidação gráfica preservou Java e prazo original; encerramento, restauração e retirada do anúncio passaram. Corrigidos inventário de recuperação e orientação para selecionar instância vazia. Entrada gráfica completa pendente. |

Verificações: 142 transporte passaram/uma fixture ignorada; 252 DOM, TypeScript, build, fmt e Clippy passaram. Recuperação física passou nas duas ordens EULA/anúncio; revalidação preserva 17 testes anteriores. A GUI revelou fechamento antes do jogo, com autorização válida; idle foi reproduzido e corrigido para o Host jogador, preservando revogação e prazos. O convidado durante downloads longos ainda requer ajuste; entrada gráfica completa segue pendente. HTTP reutilizado passou 48/48 polls em 30s, sem homologar estabilidade. Voz sob carga limita TCP a 192 kB/s até 200 ms sem UDP.

Comandos executados: `cargo test/check/clippy --offline --locked`, `npm run connect:online:frontend`, testes Node e emuladores Firebase; instalação oficial em diretório descartável. Não houve push, release ou instalador.

Produção: `aurora-connect-social.aurora-edge-proxy.workers.dev`, bundle `80f791dc…`, versão `042b42ba-43a0-4253-b82b-79ae8b621e43`; regras `20aeaa01-996b-4040-9acd-8b6cfe4eebb7`. Rollback: bundle anterior em `.aurora/connect-validation/social-online-terminal-build`; regras no snapshot `online-activation-snapshot-9bfa5bad-e1f0-4c2d-923d-c553793732fb`. Código: reverter somente commits desta etapa. Configurações pessoais preservadas; horário autenticado sem alterar Windows. `verify-mod-release-matrix.ps1` passou: 18/18 JARs correspondem ao código, hashes/assinaturas conferidos; isso não homologa mundos nem promove a instalador.
