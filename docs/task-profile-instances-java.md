# Tarefa — perfil, instâncias e Java

## Estado

**CONCLUÍDO no código e validado em 31 de agosto de 2026.**

## Escopo entregue

- O perfil público referencia somente a skin equipada. O launcher recorta a
  cabeça e a camada externa do rosto para exibi-la como avatar; `skinUrl` e
  `avatarUrl` apontam para a mesma imagem pública. As demais skins continuam
  somente na biblioteca local, sem duplicar arquivos no armazenamento online.
  Um documento mínimo em `publicProfiles/{uid}` permite que outros usuários
  autenticados vejam nome e avatar sem expor o documento privado da conta.
- Instâncias de modpack exibem nome e capa obtidos do Modrinth. Metadados antigos
  sem capa são hidratados e gravados localmente quando a API está disponível.
- O card seleciona a instância ao clique e possui ação direta **Iniciar**. O
  cabeçalho global de instância/início foi removido.
- Processos iniciados são acompanhados pelo backend. Cards mostram **Rodando** e
  bloqueiam novo início, renomeação ou exclusão até o encerramento.
- O editor permite renomear a pasta/ID da instância sem sobrescrever outra.
- Java & Engine lista tanto os runtimes do sistema quanto os gerenciados pelo
  Aurora, permite escolher o ativo e consulta as versões atualmente disponíveis
  no catálogo oficial do Eclipse Temurin. A instalação aceita JRE ou JDK, usa
  SHA-256 e inclui versões intermediárias como Java 16.
- Antes de iniciar uma instância Fabric, o launcher inspeciona também os
  `fabric.mod.json` de dependências internas dos mods. Assim, requisitos como o
  Java 16 do CaffeineConfig empacotado pelo Sodium Extra são detectados antes do
  jogo abrir, e o runtime adequado é instalado/selecionado automaticamente.
- Perfis Fabric antigos são reparados antes da execução: loaders inferiores ao
  mínimo aceito pelo Aurora Companion são atualizados e a nova versão fica
  gravada nos metadados locais da instância.
- Descrições Modrinth com HTML legado são convertidas para Markdown seguro;
  `iframe` vira link HTTPS e tags executáveis são descartadas.
- O instalador oficial do Forge recebe a pasta como valor de `--installClient`
  em todas as versões suportadas. Falhas agora mostram a exceção útil em vez da
  última linha da pilha Java.

## Limites e confiança

- Conteúdo e URLs vindos dos catálogos são tratados como não confiáveis.
- Somente URLs HTTPS são persistidas como imagens e avatar público.
- E-mail, função, estatísticas e demais dados da conta não entram no perfil público.
- O launcher não publica a biblioteca completa de skins.
- A indicação de execução representa processos iniciados na sessão atual do
  launcher; ela não tenta adivinhar processos Minecraft iniciados por terceiros.
- A inspeção de requisitos adicionais de Java usa metadados Fabric. Instâncias
  Forge continuam usando a versão-base de Java definida para cada Minecraft.

## Evidência

- Build React/Vite concluído para esta correção. O instalador existente não foi
  substituído e nenhum novo instalador foi gerado, conforme solicitado.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` e
  `cargo test` concluídos: 34 testes aprovados e 1 teste online opcional ignorado.
- `wrangler deploy --dry-run` e o lint TypeScript das Functions concluídos.
- Regras de `publicProfiles` compiladas em dry-run e publicadas somente no
  Firestore do projeto `auroralauncher` em 30 de agosto de 2026.
- Inspeção visual no aplicativo desktop confirmou cards com capa, início direto,
  renomeação no editor e o gerenciador de Java.
- A página do Cobblemon foi aberta no catálogo e não exibiu tags HTML cruas.
- Instalador `Aurora Smart Launcher_0.1.1-alpha.2_x64-setup.exe` gerado com
  SHA-256 `64CED79E0E0A335F5EC595ED0B9A22B34AD01BF9D1692E32A99ACCF07D21A160`.
- A correção posterior do comando Forge foi validada no código, sem gerar um
  novo instalador, conforme solicitado.
- Em 31 de agosto de 2026, este conjunto foi integrado ao Aurora Core 1.0.0 e
  ao Companion 0.2.0 no build `0.1.1-alpha.3`. A suíte combinada encontrou 34
  testes Rust: 33 aprovados e 1 teste online opcional ignorado.
- A inspeção visual do executável de release confirmou avatar pela cabeça da
  skin, capas e nomes dos modpacks, início direto por card, gerenciador de Java
  e catálogo Modrinth/CurseForge, sem o antigo botão global de início.
- Instalador imutável `Aurora Smart Launcher_0.1.1-alpha.3_x64-setup.exe`
  gerado com SHA-256
  `E535918AD05E26F057BA5415E93BCFB30C778D30B705A6B123F22F739364CEFB`.
- As correções posteriores de Forge, Fabric Loader e seleção de Java foram
  integradas e compiladas no instalador imutável
  `Aurora Smart Launcher_0.1.1-alpha.4_x64-setup.exe`, com SHA-256
  `DC714FB83CF6D3388EFC5B420765A4A18620B219C8F5961412A5E8CF13046B15`.
