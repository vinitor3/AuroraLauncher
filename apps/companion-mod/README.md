# Aurora Companion Mod

Projeto Architectury Loom do mod in-game Aurora. Os dois artefatos são gerados por loader:

- `fabric`: Fabric;
- `forge`: Forge;

O alvo de referência é Minecraft 1.20.1 com Forge 47.4.23, a versão 1.20.1 mais recente publicada no repositório oficial durante a implementação. A referência `47.1.84` da especificação não existe nesse repositório.

## Compatibilidade planejada

O Companion 0.2.1 é entregue apenas para Fabric e Forge e requer Aurora Core 1.1.0 ou superior. A aparência agora pertence exclusivamente ao Aurora Skins; o Companion permanece responsável pelo Assistente e pela telemetria local. O módulo usa a API IPC do Core e não abre uma segunda conexão.

| Minecraft | Fabric | Forge | Java |
| --- | --- | --- | --- |
| 1.12.2 | — | Sim | 8 |
| 1.16.5 | Sim | Sim | 8 |
| 1.19.2 | Sim | Sim | 17 |
| 1.20.1 | Sim | Sim | 17 |
| 1.21.1 | Sim | Sim | 21 |

NeoForge não está no escopo atual. O módulo 1.12.2 fica isolado dos módulos modernos para preservar a compatibilidade com Java 8.

## Organização dos builds

- `.`: build Architectury Loom configurado para a referência 1.20.1; também utilizado para as linhas 1.16.5 e 1.19.2 ao trocar as propriedades de versão.
- `legacy/forge-1.12.2`: build Forge independente, com bytecode Java 8 e `mcmod.info`.
- `modern/1.21.1`: build independente para Fabric e Forge 52, sem remapeamento de classes internas do Minecraft.
- `../../releases/companion/<versão>/<loader>`: JARs prontos para o Launcher instalar na pasta `mods` da instância.

O contrato do socket está em [`docs/companion-ipc.md`](../../docs/companion-ipc.md).

O Launcher instala primeiro o Core e depois o JAR correspondente do Companion na pasta `mods` da instância. As propriedades JVM pertencem ao Core:

- `-Daurora.ipc.port=45882`
- `-Daurora.session.nonce=<nonce-efêmero>`

O mod nunca recebe JWT Firebase, senha ou chave de serviço pela linha de comando.

## Assistente

O Companion registra o módulo `aurora_companion` no Core e conversa com o launcher pelo IPC autenticado compartilhado. O protótipo atual do
Assistente é uma janela Swing externa sobre o jogo; a substituição por uma tela
e um HUD nativos do Minecraft ainda está pendente. `AltGr + /`, pedidos de voz,
respostas e legendas do Edge TTS já usam o mesmo canal IPC.

O Companion não intercepta mais perfil, skin ou capa. Toda aparência pertence ao módulo independente Aurora Skins, evitando dois mixins concorrentes sobre o jogador. Evidências runtime anteriores em 1.20.1 Fabric e 1.12.2 Forge continuam históricas; os JARs 0.2.1 precisam ser homologados novamente junto do Core 1.1.0 e Skins 0.1.0.
