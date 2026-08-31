# Compatibility Matrix

Atualizado em 2026-08-31. `BUILD` significa JAR válido/compilável; `RUNTIME` exige o roteiro completo. Aurora Core 1.0.0 compila nos nove alvos, mas ainda não possui homologação runtime.

| Minecraft | Loader | Java | Core build | Core runtime | Companion build | Companion runtime | Estado do Core |
| --- | --- | ---: | :---: | :---: | :---: | :---: | --- |
| 1.12.2 | Forge | 8 | sim | não | sim | sim | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.16.5 | Fabric | 8 | sim | não | sim | não | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.16.5 | Forge | 8 | sim | não | sim | não | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.19.2 | Fabric | 17 | sim | não | sim | não | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.19.2 | Forge | 17 | sim | não | sim | não | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.20.1 | Fabric | 17 | sim | não | sim | sim | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.20.1 | Forge | 17 | sim | não | sim | não | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.21.1 | Fabric | 21 | sim | não | sim | não | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.21.1 | Forge | 21 | sim | não | sim | não | IMPLEMENTADO MAS NÃO HOMOLOGADO |

As duas marcações runtime do Companion são evidências anteriores e não homologam automaticamente o novo Core. O menu Aurora, o avatar 3D e a conexão única precisam ser reexecutados em cada linha.

## Roteiro runtime obrigatório

1. Instância limpa e hashes de instalador/JAR.
2. Java/loader corretos e launch sem argumento sensível.
3. Handshake IPC e rejeição de nonce inválido.
4. Core instalado antes do Companion; somente uma conexão WebSocket.
5. Options → Aurora Options; abrir/fechar repetido; ESC.
6. Avatar 3D com classic/slim, camadas externas, mouse e fallback de render.
7. Mundo não pausa; teclado/mouse voltam ao jogo sem travar.
8. Texto, voz, resposta, legenda e mute/TTS.
9. Screenshot somente após confirmação e sem telemetria.
10. Skin 64x64 classic, slim, 64x32 convertida e capa, com inspeção visual.
11. Falha de módulo/IPC/Worker e encerramento do Minecraft/launcher.
12. Migração/backup de configuração e evidência redigida anexada à execução.

## Estratégia de portes

- Referência: Core/Companion Fabric 1.20.1.
- Segundo alvo: Forge 1.20.1.
- Depois: 1.19.2, 1.16.5, 1.21.1 conforme adaptadores reais.
- Último: Forge 1.12.2, preservando Java 8/LWJGL2 e sem bloquear o moderno.
- NeoForge: fora do escopo por decisão atual.
