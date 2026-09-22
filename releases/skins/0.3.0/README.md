# Aurora Skins 0.3.0 — pacote para testes

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. O pacote contém nove alternativas; instale apenas o JAR correspondente ao Minecraft e ao loader da sua instância.

## Instalação

1. Cliente: Aurora Core 1.1.3 ou superior e Aurora Skins 0.3.0 na pasta `mods`.
2. Servidor: Aurora Skins 0.3.0 na pasta `mods`, usando a mesma combinação de Minecraft e loader. O servidor não precisa de Aurora Core.
3. Todos que desejarem ver as skins Aurora dos outros precisam do mod no cliente. O servidor também precisa dele para compartilhar as skins.
4. Escolha a skin normalmente nas opções do Aurora. A sincronização ocorre pela conexão do jogo.

Mantenha uma cópia da versão anterior e evite deixar dois JARs do Aurora Skins juntos na pasta `mods`. Clientes sem o mod seguem usando as skins vanilla. Em servidores sem o mod, a skin escolhida no Aurora continua somente local.

| Minecraft | Loader | Java |
| --- | --- | --- |
| 1.12.2 | Forge | 8 |
| 1.16.5 | Fabric ou Forge | 8 |
| 1.19.2 | Fabric ou Forge | 17 |
| 1.20.1 | Fabric ou Forge | 17 |
| 1.21.1 | Fabric ou Forge | 21 |

## Chat Heads e compatibilidade

O Aurora fornece a textura às consultas usadas por chat/lista de jogadores e outros mods. **Não adiciona cabeças ao chat e não inclui o Chat Heads.** Instale o Chat Heads separadamente se quiser esse recurso. Mods que baixam skins diretamente da Mojang ou mantêm caches próprios podem precisar de integração específica.

## O que foi verificado

- Nove JARs compilados e conferidos; hashes no `manifest.json`.
- 27 testes Java aprovados, incluindo troca de skins entre dois clientes simulados, reconexão e validação de imagens.
- Dois testes Rust de empacotamento aprovados.
- Fabric 1.21.1 iniciou o retransmissor dedicado sem Core, antes da EULA. Forge 1.21.1 reconheceu o mod sem Core; não chegou a executar o adaptador antes da EULA.

**Ainda requer teste em jogo com dois clientes e Chat Heads.** Nenhum instalador do Launcher foi gerado. Este candidato parte da base de desenvolvimento 0.2.4; a integração com a interface posterior 0.2.5, incluindo a grade de resultados, permanece separada. Não é uma atualização homologada do instalador atual.

Limites: até 256 participantes do protocolo; atualização de skin espaçada; mesma versão de Minecraft/loader; proxy/ViaVersion e combinações entre loaders não homologados. Skins compartilhadas ficam apenas em memória no servidor.
