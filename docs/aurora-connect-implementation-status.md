# Aurora Connect — implementação

Estado em 09/11/2026: em andamento, experimental. Base local `2349320`; matriz original de nove pares preservada.

| Área | Estado observado |
|---|---|
| TCP / UDP | NativeVoice8 assinado: oito participantes locais, 50 datagramas/s por pessoa, TCP íntegro de 2 MiB; cinco ensaios consecutivos passaram. Limites antigos preservados. |
| Direto | Evidência externa IPv6 anterior preservada; cobertura IPv4/NAT depende da validação de rede. |
| Relay / online | Worker gratuito ativo; Firebase sem faturamento. Contrato de voz 5 passou em autenticação, horário, política, disponibilidade, handshake e limpeza reais. Cotas do relay inalteradas; não comportam oito vozes no pior caso. |
| Amigos / grupos / pedidos | Busca por nick Aurora, catálogo, entradas autorizadas, grupos e remoção individual implementados. |
| Jogo | Instalação real dos nove perfis e inspeção das cópias passaram, sem abrir Minecraft. Entrada no mundo continua com evidência própria. |
| Simple Voice Chat | Nove servidores oficiais passaram em preparo/STATUS/UDP/encerramento; áudio permanece não homologado. |
| Plasmo Voice | Oito servidores oficiais passaram em preparo/STATUS/UDP/encerramento após corrigir o prazo de salvamento V3 para 30s. Áudio não homologado; não há versão oficial Forge 1.12.2. |
| Interface | Busca, grupo, dedicado, anúncio para amigos e recuperação observados. HostPlayer chegou a transporte disponível, mas encerrou antes do prazo; causa ainda em investigação. Revisão simplificada e seleção de origem corrigidas. |

Verificações: 134 transporte, 78 backend, 103 frontend, 32 nativas, 11 de horário, sete de bootstrap e dois gates passaram; Clippy passou. A sonda real confirmou 19 polls e perdeu a janela de continuidade no 20º, apesar de a sessão ainda ter prazo; encerramento e restauração confirmados. Falhas preservadas. Controle/handshake não homologam TCP/UDP entre redes, mundo ou áudio. Voz sob carga limita TCP a 192 kB/s até 200 ms sem UDP.

Comandos executados: `cargo test/check/clippy --offline --locked`, `npm run connect:online:frontend`, testes Node e emuladores Firebase; instalação oficial em diretório descartável. Não houve push, release ou instalador.

Produção: `aurora-connect-social.aurora-edge-proxy.workers.dev`, bundle `80f791dc…`, versão `042b42ba-43a0-4253-b82b-79ae8b621e43`; regras `20aeaa01-996b-4040-9acd-8b6cfe4eebb7`. Rollback: bundle anterior em `.aurora/connect-validation/social-online-terminal-build`; regras no snapshot `online-activation-snapshot-9bfa5bad-e1f0-4c2d-923d-c553793732fb`. Código: reverter somente commits desta etapa. Configurações pessoais preservadas; horário autenticado sem alterar Windows. `verify-mod-release-matrix.ps1` passou: 18/18 JARs correspondem ao código, hashes/assinaturas conferidos; isso não homologa mundos nem promove a instalador.
