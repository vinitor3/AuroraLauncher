# 0.2.0-beta.1

A beta reúne a alpha.15 e a entrega do chat **Melhorar UI e recursos do launcher**. A conta Aurora continua identificando o perfil e as amizades. O mantenedor prevê a autenticação Microsoft e a verificação de licença para o fim do projeto.

| Área | Mudança para quem testa |
| --- | --- |
| Interface e Java | Navegação visível em 960 × 620, listas com rolagem interna e RAM global ou por modpack. |
| Amigos | Contador de solicitações, convites e compartilhamento de um mundo aberto para LAN. |
| Diagnóstico | O Doctor continua local. Chat, microfone, voz sintética e atalho do assistente foram retirados. **Preparar para IA** gera um texto editável para erros do launcher, Minecraft ou servidor. É possível incluir um trecho filtrado antes de copiar. |
| Skins | Catálogo da comunidade, navegação por página, retrato 2D e prévia 3D com giro, caminhada e modelo Clássico/Slim. Os seis visuais do Aurora Studio saíram do launcher, do mod, do serviço e dos resultados antigos em cache. |
| Temas | **Alto contraste** e **Âmbar compacto** entram na biblioteca de temas. Teste por 10 segundos e confirme para aplicar. |
| Inicialização | O leitor de descrições e o visualizador 3D são carregados quando usados. Mudar a aparência deixa de disparar uma nova leitura da lista de instâncias. |
| Mods Aurora | Core 1.1.7 e Skins 0.3.3 compartilham lógica por módulo e mantêm adaptadores para cada versão. A seleção de Java reconhece os perfis modernos; o launcher preserva arquivos de mod já idênticos. |

## Minecraft e loaders

| Minecraft | Fabric | Forge | Java do jogo |
| --- | :---: | :---: | --- |
| 1.12.2 | — | Sim | 8 |
| 1.16.5 | Sim | Sim | 8 |
| 1.19.2 | Sim | Sim | 17 |
| **1.20.1** | **Sim** | **Sim** | 17 |
| **1.21.1** | **Sim** | **Sim** | 21 |
| **1.21.11** | **Sim** | — | 21 |
| **26.3** | **Sim** | — | 25 |

Em 24/11/2026, entre os 100 modpacks mais baixados do Modrinth, 51 dos 70 projetos que declaravam Fabric listavam 1.20.1, 47 listavam 1.21.1 e 40 listavam 1.21.11. Dos 32 que declaravam Forge, 23 listavam 1.20.1. São versões anunciadas e downloads acumulados, sem contagem de jogadores ativos. No CurseForge, RLCraft e Better MC também justificam manter Forge 1.12.2 e 1.20.1. [Consulta Modrinth](https://api.modrinth.com/v2/search?facets=%5B%5B%22project_type%3Amodpack%22%5D%5D&index=downloads&limit=100) · [Modpacks Forge no CurseForge](https://www.curseforge.com/minecraft/search?class=modpacks&gameVersionTypeId=1&page=1&pageSize=20&sortBy=total+downloads)

Abrimos o Minecraft nas 11 combinações da tabela com os dois JARs distribuídos. Em todas, o Core respondeu, a janela do jogo abriu e o processo encerrou normalmente. No Forge 1.20.1, verificamos os builds 47.4.23 e 47.4.28. No Fabric 1.21.11 e 26.3 também navegamos pelo catálogo e aplicamos uma skin Slim. Partida entre dois computadores e voz ainda precisam de homologação.

[Guia rápido](README.md) · [Matriz com versões exatas](../scripts/active-mod-matrix.json) · [Como portar os mods](portar-mods.md) · [Instalador, SHA-256 e manifesto](https://github.com/vinitor3/AuroraLauncher/releases/tag/v0.2.0-beta.1)
