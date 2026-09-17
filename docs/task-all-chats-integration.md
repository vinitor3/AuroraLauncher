# Tarefa — integração das modificações do Aurora

## Integração atual de 17/09/2026

As entregas locais foram reunidas em `codex/aur-release-20260917`, versão
`0.1.1-alpha.6`: catálogos/filtros, Core 1.1.2, Skins 0.2.3, Companion 0.2.3,
busca de skins e atualizador de conteúdo. O preflight confirmou os nove worktrees
sem entregas pendentes de integração. O Worker integrado está publicado e
CurseForge responde HTTP 200 com a chave renovada.

O comando `npm run release:package` gera uma única release e seu manifesto;
`npm run release:status` consulta pendências. Consulte `release-workflow.md`.
Os detalhes abaixo preservam a release histórica alpha.4. Compilação não
substitui homologação dentro do Minecraft; atualização automática de modpacks
CurseForge permanece bloqueada, conforme `task-content-updates.md`.

## Estado

**CONCLUÍDO no código e compilado em 31 de agosto de 2026.**

O instalador desktop foi gerado. A homologação dentro de todas as combinações
de Minecraft, loader e Java continua sendo uma etapa manual separada.

## Escopo integrado

- Redesign Aurora com navegação e componentes mais compactos, editor de
  instância reorganizado, visualização de skins e assistente com estado visual
  de microfone.
- Avatar da conta derivado da cabeça da skin equipada, mantendo apenas essa
  skin no perfil público e as demais na biblioteca local.
- Cards de instância com capa de modpack, início direto, estado **Rodando** e
  renomeação no editor, sem o antigo botão global de início.
- Gerenciador de Java com descoberta e instalação de JRE/JDK Temurin, incluindo
  Java 8, 16, 17, 21, 25 e requisitos detectados dinamicamente.
- Criação de instâncias somente com Forge ou Fabric, atualização automática de
  Fabric Loader legado e correção do comando do instalador oficial do Forge.
- Catálogos Modrinth e CurseForge para modpacks, mods, shaders e resource packs,
  com filtros, ordenação, paginação, imagens, detalhes sanitizados e listagem
  dos mais baixados sem exigir busca.
- Instalação de modpacks CurseForge, overrides e dependências obrigatórias. Os
  arquivos bloqueados para distribuição automática seguem o fluxo oficial de
  download em página, sem contornar a política do catálogo.
- Aurora Core 1.1.0, Aurora Skins 0.1.0 e Aurora Companion 0.2.2 para a matriz
  suportada de Forge/Fabric, incluindo a correção de multiplayer offline 1.16.5
  e as melhorias de conexão, desligamento e feedback do Companion.

## Evidência de validação

- Build React/TypeScript/Vite concluído.
- `cargo fmt --check` e `cargo clippy --all-targets -- -D warnings` concluídos.
- Testes Rust: 45 aprovados e 1 teste online opcional ignorado.
- Matriz de release: 9/9 JARs Core, 9/9 Skins e 9/9 Companion correspondem ao
  código integrado.
- O Worker desse fluxo recebeu uma publicação posterior em 1º de setembro de
  2026, com categorias e gateway autenticado de downloads CurseForge. A
  credencial CurseForge de produção ainda precisa ser renovada.
- Lint das Firebase Functions concluído.
- Preflight oficial de release aprovado no branch
  `codex/aurora-all-chats-integration`.
- `npm run desktop:build` concluiu e gerou o bundle NSIS.

## Artefato

- Arquivo: `releases/Aurora Smart Launcher_0.1.1-alpha.4_x64-setup.exe`
- Tamanho: 6.333.385 bytes
- SHA-256: `DC714FB83CF6D3388EFC5B420765A4A18620B219C8F5961412A5E8CF13046B15`
- Assinatura Authenticode: ausente (`NotSigned`).

## Limites restantes

- O build confirma compilação e consistência dos artefatos, mas não substitui
  testes manuais de inicialização e multiplayer para cada versão do Minecraft.
- O Worker está implantado, mas a busca CurseForge permanece indisponível até o
  proprietário substituir a chave recusada pela API oficial.
- O bundle web mantém o aviso conhecido de chunk acima de 500 kB.
- Os builds de mods mantêm avisos conhecidos do Architectury Loom/Gradle legado.
- A instalação das Functions em Node 24 avisou que o projeto declara Node 20 e
  o `npm audit` informou oito vulnerabilidades moderadas nas dependências; o
  lint passou e nenhuma atualização forçada foi aplicada durante esta tarefa.
