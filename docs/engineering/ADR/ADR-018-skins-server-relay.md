# ADR-018 — Retransmissão de skins pela conexão do servidor Minecraft

Data: 22/09/2026. Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**.

## Contexto

Aurora Skins aplicava a aparência no cliente local. A escolha não era enviada aos demais jogadores e consultas de outros mods, como cabeças no chat, podiam continuar obtendo a textura oficial. A API de catálogo existente fornece imagens, mas não identifica a conexão de cada participante de um servidor Minecraft.

## Decisão

Adicionar ao Aurora Skins 0.3.0 um protocolo opcional na conexão Minecraft, implementado nos adaptadores Fabric/Forge já suportados. Instalar no servidor o JAR da mesma versão do Minecraft e do mesmo loader; esse lado não inicializa interface, renderização, Aurora Core ou Companion. O cliente mantém Aurora Core 1.1.3 ou superior como requisito.

O cliente aguarda confirmação do protocolo antes de publicar a seleção. O servidor atribui a publicação exclusivamente ao UUID da conexão, valida o PNG e retransmite a aparência aos participantes. Novos participantes recebem um retrato das aparências anunciadas. Clientes sem Aurora Skins continuam podendo entrar, respeitadas as outras exigências do servidor, e não recebem essa funcionalidade. Em servidor sem o mod, o cliente mantém a aparência local.

Manter até 256 participantes, com uma publicação por jogador a cada dois segundos. Validar estrutura, CRC, dimensões e tamanho antes de decodificar; normalizar para 64x64 e recodificar somente os pixels, até 24 KiB. Armazenar no servidor apenas o estado em memória da sessão, sem URLs fornecidas pelos jogadores, serviço de nuvem ou credenciais. A biblioteca e o catálogo existentes não mudam de finalidade.

Carregamentos no cliente têm filas e quantidades limitadas. Identificadores de sessão impedem envios e callbacks de uma conexão encerrada; cada textura possui identidade própria para que uma liberação atrasada não remova sua substituta. Desconexão, saída e encerramento liberam o estado pertinente.

Resolver a aparência por UUID nos acessos ao personagem e a `PlayerInfo`, ampliando a compatibilidade com chat/tab. Usar também as consultas disponíveis de `SkinManager` em 1.19.2/1.20.1/1.21.1. Em 1.21.1, a função de consulta reutilizável verifica o estado atual em cada chamada; não substituir o futuro interno `getOrLoad`, pois seu resultado poderia permanecer armazenado após a liberação da textura Aurora. O trilho 1.12.2 permanece com seu adaptador de perfil legado. Preservar modelo e fallback vanilla quando não houver substituição, assim como capas/elytra oficiais.

## Consequências e limites

Não é necessário um backend público de skins ou uma conta Aurora para identificar publicações. O administrador precisa instalar o mod no servidor, e cada jogador que quiser enxergar as skins compartilhadas precisa dele no cliente. A skin oficial da conta não é alterada. Servidores ou clientes que imponham outras restrições de mods continuam aplicando suas próprias regras.

Não são adicionadas cabeças ao chat nem redistribuído Chat Heads. Mods que consultam a Mojang diretamente ou armazenam texturas independentemente podem exigir integração específica. Cabeças de itens/blocos e dados produzidos exclusivamente pelo servidor continuam seguindo os mecanismos existentes. Proxy/ViaVersion, travessia entre versões de Minecraft e combinação de transportes Fabric/Forge não fazem parte da compatibilidade homologada; proxy/travessia entre versões não são suportados nesta entrega.

Sete testes do retransmissor e quatro testes do cliente foram aprovados, incluindo dois clientes simulados, identidade, limites, alterações rápidas, PNG inválido e callbacks atrasados. As nove combinações compilam, mas isso não homologa o comportamento dentro do jogo. Boot dedicado, dois clientes reais e compatibilidade visual com Chat Heads devem ser registrados no [roteiro da tarefa](../../task-skins-multiplayer.md). Não gerar instalador ou substituir releases anteriores como efeito desta decisão.
