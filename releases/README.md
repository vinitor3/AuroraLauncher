# Releases do Aurora

Os artefatos desta pasta são imutáveis. Um nome de versão já publicado nunca
deve ser sobrescrito nem removido.

## Fluxo obrigatório do instalador desktop

1. Integre e faça commit de todas as mudanças de produto.
2. Garanta que os outros worktrees estejam limpos e que seus branches relevantes
   façam parte do histórico do branch de integração.
3. Incremente a mesma versão em `package.json`, no pacote desktop, em
   `Cargo.toml` e em `tauri.conf.json`.
4. Execute `npm run desktop:build` na raiz. O preflight bloqueia checkout sujo,
   branch principal, versões divergentes, conflitos, worktrees pendentes e
   tentativa de reutilizar um release existente.
5. Copie o bundle NSIS para `releases` sem `-Force`, gere SHA-256 e registre a
   evidência no documento da tarefa.
6. Faça commit do artefato e crie a tag SemVer correspondente.

Não use `npm --prefix apps/desktop run tauri build` para um release oficial,
pois esse comando ignora o preflight da raiz.

## Instaladores preservados

- `0.1.0`: `95C2708E5898A0E194263D5E9865F0CC6E56D8880E757C209BD4A50A69D9AAF8`
- `0.1.1-alpha.2`: `64CED79E0E0A335F5EC595ED0B9A22B34AD01BF9D1692E32A99ACCF07D21A160`
- `0.1.1-alpha.3`: `E535918AD05E26F057BA5415E93BCFB30C778D30B705A6B123F22F739364CEFB`
- `0.1.1-alpha.4`: `DC714FB83CF6D3388EFC5B420765A4A18620B219C8F5961412A5E8CF13046B15`

A `alpha.3` é o release integrado anterior e contém o redesign do desktop,
Aurora Core 1.0.0 e Companion 0.2.0.

A `alpha.4` integra os trabalhos posteriores de perfil, instâncias, Java,
catálogos Modrinth/CurseForge, instalação de dependências, Aurora Core 1.1.0,
Aurora Skins 0.1.0 e Aurora Companion 0.2.2.

## Matrizes de módulos preservadas

- Aurora Core 1.1.0: nove JARs Fabric/Forge versionados em `releases/core/1.1.0`.
- Aurora Skins 0.1.0: nove JARs versionados em `releases/skins/0.1.0`.
- Aurora Companion 0.2.2: nove JARs versionados; os hashes estão em
  [`companion/0.2.2-SHA256SUMS.txt`](companion/0.2.2-SHA256SUMS.txt).

O Companion 0.2.2 preserva os artefatos 0.2.0/0.2.1 e incorpora as correções
de inicialização, desligamento, feedback do Assistente e limite de captura
integradas depois da matriz 0.2.1.

## Histórico do Companion 0.2.0

Os artefatos continuam preservados byte a byte, mas a auditoria posterior
encontrou faixas Forge incorretas nos JARs 1.16.5/1.19.2 e uma corrida de
inicialização nas linhas Fabric observadas. Esses problemas foram corrigidos
na matriz 0.2.2; a `alpha.3`, que ainda incorpora o Companion 0.2.0, não deve
ser descrita como matriz runtime homologada. Consulte
[`docs/engineering/COMPANION_AUDIT.md`](../docs/engineering/COMPANION_AUDIT.md).
