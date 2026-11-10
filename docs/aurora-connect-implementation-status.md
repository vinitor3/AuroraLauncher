# Aurora Connect — implementação

Estado em 10/11/2026: IMPLEMENTADO MAS NÃO HOMOLOGADO, com incrementos em andamento. Código local `dfb7c15`; matriz original de nove pares preservada.

| Área | Estado observado |
|---|---|
| TCP / UDP | NativeVoice8 assinado: oito participantes locais, 50 datagramas/s por pessoa, TCP íntegro de 2 MiB; cinco ensaios consecutivos passaram. Limites antigos preservados. |
| Direto | Evidência externa IPv6 anterior preservada; IPv4/NAT depende de teste externo. Observação local schema2 acrescenta RTT estimado do caminho selecionado, sem sondas, nova autoridade ou mudança no protocolo; substitui a descrição schema1 sem métricas. Não é ping do jogo nem menor latência absoluta. |
| Relay / online | Worker gratuito ativo; Firebase sem faturamento. Contrato de voz 5 passou em autenticação, horário, política, disponibilidade, handshake e limpeza reais. Cotas do relay inalteradas; não comportam oito vozes no pior caso. |
| Amigos / grupos / pedidos | Busca por nick Aurora, catálogo, entradas autorizadas, grupos e remoção individual. Seleção inválida de convite é descartada. Cancelar pedido pendente libera atomicamente a própria vaga, sem cancelar aprovação; regras online atualizadas com autorização específica. Privado tem convite manual; direcionamento por conta Aurora ainda não implementado. |
| Jogo | Nove perfis instalados/inspecionados. GUI Fabric 1.21.1 entrou com Core/Skins pelo HostPlayer Social: SVC reutilizado e Plasmo em cópia nova, 191s cada até saída deliberada, exit0. Não homologa duração longa nem nove clientes. |
| Simple Voice Chat | Nove servidores oficiais passaram em preparo/STATUS/UDP/encerramento. Cliente real autenticou e validou UDP pela ponte; áudio permanece não homologado. |
| Plasmo Voice | Oito servidores oficiais passaram em preparo/STATUS/UDP/encerramento. Novo cliente Fabric 1.21.1/Plasmo 2.1.17 conectou UDP nos dois lados pela ponte e entrou no mundo. Áudio não homologado; não há versão oficial Forge 1.12.2. |
| Interface | Guia de voz, cópia, revisão, entrada, Stop, retirada do anúncio e recuperação passaram pela GUI. Progresso termina sem limpar outra instalação. Guia privado incorporado ao executável; botão abre convites sem iniciar conexão. Fluxo manual volta somente após limpeza Social confirmada. |

Verificações anteriores: 337 DOM, 107 backend, sete focais/regressões nativas e QA passaram. Incrementos atuais: guia 39 DOM; progresso 6 DOM + 2 nativos; revisão oficial de assets 3 nativos/reais; grupos 6 DOM; cancelamento 10 DOM + 80 regressões + 6 SDK + 7 emulador. TypeScript, fmt/Clippy e revisões independentes passaram nos escopos alterados. Registro terminal limitado a 4 KiB; não cobre saída abrupta.

Falhas preservadas e corrigidas: HostPoll de 3.009ms (limite de 3s mantido; novo probe 52/52, HTTP máximo 1.051ms); reutilização JNA com verificação do JAR oficial (`ecc3dd7`); índice oficial de assets revisado, aceito somente pelo novo par de hashes (`878b586`); progresso preso em 98% (`6854b2f`). GUI Plasmo no exe `56c07200…` passou; processos/portas liberados, EULA e chaves de anúncio recuperadas. Áudio e rede externa não testados nesta rodada.

Comandos: Cargo test/clippy/fmt, Node --test, TypeScript, emulador Firestore e `connect:online:build`. RTT: 6 unitários, 5 integrações reais (1 IPv4 opt-in não executado), 19 nativos desktop; guia/RTT: 312 DOM. Evidências em `.aurora/connect-validation/`: `gui-plasmo-fixed-20261110-result.json`, `gui-jna-reuse-20261110-readonly.json`, `path-rtt-20261110-result.json`, `gui-private-navigation-20261110-result.json`, `social-request-cancel-20261110/`. Build `29074655…` e GUI passaram; processos encerrados, sem push/release/instalador.

Produção: Worker `aurora-connect-social.aurora-edge-proxy.workers.dev`, versão `38d190c1-e163-455a-9b06-069b80f1f5aa`, bundle `c86db950…`; regras Firestore `9810d06c-1a9f-4e57-b4ed-4d61cc0d6ff6`, SHA `570946a9…`. Atualizações especificamente autorizadas, sem alterar planos/limites/bindings. Rollback preparado: Worker `042b42ba-43a0-4253-b82b-79ae8b621e43`; regras `20aeaa01-996b-4040-9acd-8b6cfe4eebb7`; snapshots nos pacotes `social-locator-build-20261110` e `social-request-cancel-20261110`. Reverter somente commits próprios; configurações pessoais preservadas. Matriz: 18/18 JARs conferidos anteriormente; isso não homologa mundos/áudio nem libera instalador.
