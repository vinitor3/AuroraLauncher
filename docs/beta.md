# Aurora Smart Launcher 0.2.0-beta.3

| Área | Como usar |
| --- | --- |
| Modpacks | Em **Descobrir**, abra o projeto para ver a descrição do autor e o destino da instalação. Use Tab para percorrer as ações e Escape para voltar ao resultado. |
| Conta | Ao vincular o e-mail, confira o endereço na tela, abra o link recebido e volte ao Aurora para concluir. O Firebase envia a mensagem pelo domínio do projeto. |
| Logs e crashes | No console da instância, alterne entre **Log do jogo** e **Último crash**. Para copiar um arquivo de servidor, use **Copiar log ou crash**. O tratamento do texto é local. O assistente, os prompts de tema e a tradução por Gemini foram retirados. |
| Aurora Connect | Ao entrar em uma sessão com a instância compatível selecionada e uma única cópia pronta, o launcher localiza e verifica a cópia. **Jogar agora** abre o Minecraft. Instalações pendentes e mundos com aprovação indicam o próximo passo na tela. |
| Discord | Em **Amigos e comunidade → Convites → Presença no Discord**, ative a integração. A escolha por instância permite usar a presença do Aurora ou a de um mod. O modo automático reconhece CraftPresence e Simple RPC. O anfitrião pode responder individualmente aos pedidos de entrada. |

A presença nativa depende do Discord desktop e do ID público de aplicativo do Aurora. O instalador pode obter esse ID pela [configuração oficial da edição](../scripts/discord-application.json). Em **Copiar convite Aurora**, copie o link da sessão pública e compartilhe-o pelo Discord. Quem recebe abre o link e escolhe **Abrir convite no Aurora**. A página também oferece o código para colar em **Convite recebido**. A política do mundo determina se o anfitrião precisa aprovar a entrada.

## Minecraft e loaders

| Minecraft | Fabric | Forge | Java do jogo |
| --- | :---: | :---: | --- |
| 1.12.2 | — | Sim | 8 |
| 1.16.5 | Sim | Sim | 8 |
| 1.19.2 | Sim | Sim | 17 |
| 1.20.1 | Sim | Sim | 17 |
| 1.21.1 | Sim | Sim | 21 |
| 1.21.11 | Sim | — | 21 |
| 26.3 | Sim | — | 25 |

A matriz distribui Core 1.1.7 e Skins 0.3.4. A autenticação Microsoft está prevista para o fim do projeto.

**Desempenho observado.** No teste com o Additive 26.5.1 e Fabric 1.21.11, executado pelo motor local antes do empacotamento da beta.3, o jogo levou 21,5 s da criação do processo até a tela inicial com os caches aquecidos. Do título até um mundo novo foram mais 11,1 s. Em 30 amostras, a mediana foi 60 FPS com VSync, RX 6650 XT, janela de 854 × 480 e distância de 12 chunks. Outros modpacks, configurações e computadores podem apresentar resultados diferentes.

[Guia rápido](README.md) · [Matriz exata](../scripts/active-mod-matrix.json) · [Download e integridade](https://github.com/vinitor3/AuroraLauncher/releases/tag/v0.2.0-beta.3) · [Beta anterior](https://github.com/vinitor3/AuroraLauncher/releases/tag/v0.2.0-beta.2)
