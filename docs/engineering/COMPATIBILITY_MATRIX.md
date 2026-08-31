# Compatibility Matrix

Atualizado em 2026-08-30. `BUILD` significa JAR válido/compilável; `RUNTIME` exige roteiro completo. A documentação existente confirma runtime apenas nas linhas marcadas.

| Minecraft | Loader | Java | Build | Runtime | HUD nativa | Aparência visual | Estado |
| --- | --- | ---: | :---: | :---: | :---: | :---: | --- |
| 1.12.2 | Forge | 8 | sim | sim | não | parcial | PARCIAL / legado isolado |
| 1.16.5 | Fabric | 8 | sim | não | não | não | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.16.5 | Forge | 8 | sim | não | não | não | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.19.2 | Fabric | 17 | sim | não | não | não | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.19.2 | Forge | 17 | sim | não | não | não | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.20.1 | Fabric | 17 | sim | sim | não | parcial | PARCIAL / alvo de referência |
| 1.20.1 | Forge | 17 | sim | não | não | não | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.21.1 | Fabric | 21 | sim | não | não | não | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.21.1 | Forge | 21 | sim | não | não | não | IMPLEMENTADO MAS NÃO HOMOLOGADO |

## Roteiro runtime obrigatório

1. Instância limpa e hashes de instalador/JAR.
2. Java/loader corretos e launch sem argumento sensível.
3. Handshake IPC e rejeição de nonce inválido.
4. Keybind configurável; abrir/fechar repetido; ESC.
5. Mundo não pausa; teclado/mouse voltam ao jogo sem travar.
6. Texto, voz, resposta, legenda e mute/TTS.
7. Screenshot somente após confirmação e sem telemetria.
8. Skin 64x64 classic, slim, 64x32 convertida e capa, com inspeção visual.
9. Falha de Worker/rede e encerramento do Minecraft/launcher.
10. Evidência redigida anexada à execução.

## Estratégia de portes

- Referência: Fabric 1.20.1.
- Segundo alvo: Forge 1.20.1.
- Depois: 1.19.2, 1.16.5, 1.21.1 conforme adaptadores reais.
- Último: Forge 1.12.2, preservando Java 8/LWJGL2 e sem bloquear o moderno.
- NeoForge: fora do escopo por decisão atual.

