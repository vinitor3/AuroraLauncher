# Integração Skins e sessões independentes

Data: 23/09/2026. Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**.

## Base e escopo

Base atual do launcher: `4c58a3b`, alpha.10, Core 1.1.5 e Skins 0.2.5. A entrega multiplayer `e45e2b5` partiu de `269d282` e não incluía `1d4f8c8`, responsável pela grade. A integração é um merge sobre a base atual, preservando ambos os históricos; não é substituição pela árvore antiga. Skins 0.3.1 reúne as duas entregas e passa a ser selecionado na matriz ativa e nos nove JARs embutidos. As releases 0.2.5 e 0.3.0 permanecem imutáveis.

Preservados da 0.2.5: grade de 12 prévias de corpo inteiro, layout responsivo 4×3/6×2, seleção por identidade, aplicação explícita, paginação, estados por slot, descarte de respostas antigas, cache de 96 prévias e downloads simultâneos deduplicados, validação/reparo de cache em disco. Biblioteca, importação, favoritos, histórico, busca, modelos slim/wide e prévia de duas camadas continuam disponíveis.

Integrados da 0.3.0: protocolo opcional cliente/servidor, retransmissão pelo UUID da conexão, snapshot para novos participantes, atualização/remoção/desconexão, validação e limites de PNG, filas limitadas, descarte de callbacks antigos e resolução de skin por UUID nos pontos usados por mods como Chat Heads. Não há cabeças novas no chat nem redistribuição de Chat Heads. Capas/elytra e fallback oficial são preservados. Ver [ADR-022](engineering/ADR/ADR-022-skins-server-relay.md).

Mantidos da base atual: Aurora Doctor, Assistente desktop/voz, retirada de Companion, Core 1.1.5, controles de janela/bandeja e matriz de 18 JARs. Não houve deploy de backend.

## Proteção contra repetição

O contrato cumulativo `scripts/skins-feature-contract.json` é consumido tanto pelos testes Rust dos artefatos embutidos quanto pelo verificador oficial de release. Exige as classes da grade e da sincronização, ligações da UI às prévias e marcadores do cache. Não depende do número declarado na versão do JAR. Fixtures reais da 0.2.5 sem multiplayer e da 0.3.0 sem grade devem ser recusadas; mutações somente em memória simulam a perda da ligação entre UI/cache e suas implementações.

O comparador de release continua recompilando e comparando todo o conteúdo normalizado dos 18 artefatos. A regra de base atual foi acrescentada ao AGENTS.md. Esses testes protegem os cenários conhecidos e não substituem a revisão ou a homologação no jogo.

## Sessões do launcher

A primeira janela mantém o login lembrado existente. Janelas/processos adicionais recebem um escopo independente e temporário, permitindo entrar com outra conta sem propagar login/logout. Encerrar a principal não troca a conta das janelas que já estavam abertas. X continua ocultando para a bandeja; usar **Fechar o launcher** encerra aquele processo. Decisão e limites em [ADR-023](engineering/ADR/ADR-023-independent-launcher-sessions.md).

Para dois Minecrafts, abrir duas instâncias separadas do jogo, uma por launcher/conta. Enquanto o launcher responsável estiver aberto, o bloqueio mantido durante o jogo impede inicialização e alterações concorrentes naquela instância. No Windows, iniciar, renomear e excluir também verificam se o jogo sobreviveu ao encerramento do launcher. As demais mutações manuais não ganham essa verificação adicional nesta entrega. O Assistente continua acessível pelo botão em todas as janelas; o atalho global pertence à principal.

## Validação

Resultados executados:

- Core: build dos nove pares e nove testes Java aprovados.
- Skins: build dos nove pares e 41 testes Java aprovados (grade/cache e sincronização na mesma suíte).
- Legado 1.12.2: três suítes focadas aprovadas, compilando contra Forge/Core reais e Netty 4.1.9; verificam 12 modelos/texturas distintos, seleção/aplicação, paginação, resize e limpeza. São fixtures, não homologação gráfica.
- Frontend: 123 testes e build TypeScript/Vite aprovados; incluem três escopos de autenticação, migração do login principal, erros/timeouts e restauração de perfil.
- Rust: 146 testes aprovados e cinco testes de serviços externos ignorados conforme suas marcações existentes; `cargo fmt --check` e `cargo clippy --all-targets -- -D warnings` aprovados. Cobrem eleição/isolamento, recuperação após crash real de processo auxiliar, bloqueio entre launchers, alias de caixa no Windows, recusa de junction e renomear/excluir instância parada. Uma falha encontrada no guard interno foi corrigida movendo a trava para fora da pasta da instância; os testes passaram novamente.
- Revisão independente final: antecipada a detecção de JVM órfã para antes de preparar o loader/Doctor; após esse ajuste, fmt/clippy e os 11 testes de comandos passaram novamente.
- Matriz: 18/18 artefatos correspondem às fontes recompiladas; Core 9/9 assinaturas aprovadas. Quatro cenários do verificador continuam aprovados.
- Contrato cumulativo: nove JARs 0.3.1 aceitos; 0.2.5 e 0.3.0 incompletas recusadas; três mutações em memória de UI/cache/mixin recusadas. Hashes, loaders de build/seleção e Java em [manifesto 0.3.1](../releases/skins/0.3.1/manifest.json).
- Smoke dedicado final: Fabric Minecraft 1.21.1, Loader 0.16.14, Temurin 21.0.12.1+1-LTS. JAR SHA-256 `fdfe82f248204e3a0de3bc0c0886c3b98efab2c59bfae140f686411cdb4a2258`. Inicializou `SkinSyncFabricNetwork`/`SkinSyncServer` sem Core, encerrou com código 0 antes da EULA, sem mundo/jogadores ou classes de cliente/Core. EULA permaneceu falsa. Evidências locais em `.aurora/qa/fabric-1211-server-no-core-031/`; não comprova handshake entre jogadores, UI, skin ou capa.

O usuário confirmou multiplayer e Chat Heads funcionando na 0.3.0 em Minecraft 1.20.1 Forge e Fabric. As versões exatas dos loaders e os hashes usados nesse teste não foram informados. Esse relato não homologa os nove pares nem os novos hashes da 0.3.1.

Homologação real com duas contas em múltiplos launchers e dois clientes Minecraft por versão/loader: **PRECISA DE TESTE**. A evidência da grade 0.2.5 e do multiplayer 0.3.0 permanece nos registros históricos e não é transferida para novos binários.
