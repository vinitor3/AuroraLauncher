# Executive Summary

## Resultado

O Aurora é um **alpha funcional avançado do launcher**, não um produto multiplayer pronto. A fundação desktop é real e passou novamente por build/testes em 2026-08-30. O trilho crítico imediato é fechar segurança de identidade/dados, licença, CI e proveniência de release; modularização e encerramento verificável do Companion vêm logo depois.

## Baseline confirmado

- Git ativo, `main` sincronizada com `origin/main`, cinco commits e tag `v0.1.0-alpha`.
- CI pública existente; último run do commit `5aa5fe8` concluído com sucesso.
- Frontend: 309 módulos, bundle principal de 1.528,34 kB (gzip 431,78 kB), com alerta de chunk >500 kB.
- Rust: `fmt` e `clippy -D warnings` verdes; 26 testes aprovados e um teste online de TTS ignorado.
- Worker e Functions: validação TypeScript/dry-run verdes.
- Companion 1.20.1: build Gradle verde, mas Loom 1.3 está obsoleto e há APIs incompatíveis com Gradle 9.
- Nove JARs versionados presentes; sete combinações permanecem sem evidência runtime registrada.

## Correções ao relatório de 29/08

O relatório dizia que Git, licença e CI não existiam. Isso ficou desatualizado: os três já existem. Ainda não estão encerrados:

- a CI não exige `rustfmt`, `clippy`, build do Companion nem matriz runtime;
- `LICENSE.md` não licencia claramente todo o monorepo e não contém o texto integral da GPL;
- o release publicado e o instalador rastreado no tag têm bytes diferentes: Git `95C2708E…D9AAF8` (5.029.160 bytes) versus GitHub `582E5BA2…465A9` (5.028.702 bytes);
- todos os manifests ainda declaram `0.1.0`, portanto a próxima build precisa de bump coordenado e não pode reutilizar o release histórico.

## Bloqueador de segurança encontrado

A regra original de criação de `/users/{uid}` aceitava um `role` escolhido pelo próprio cliente. Um usuário autenticado podia criar o perfil como `ADMIN`. O patch local agora exige `PLAYER`, UID/e-mail sintético coerentes, allowlist exata de campos e estatísticas iniciais. Sete testes comportamentais positivos/negativos passam no Firestore Emulator. Produção continua potencialmente vulnerável até a auditoria dos documentos existentes e o deployment explícito das regras.

## Estado por área

| Área | Estado | Evidência / lacuna |
| --- | --- | --- |
| Desktop e engine | PARCIAL | build/testes verdes; E2E e modularização ausentes |
| Downloads | IMPLEMENTADO MAS NÃO HOMOLOGADO | concorrência/hash/rename testados; falhas extensas e CAS futuros |
| Companion | IMPLEMENTADO MAS NÃO HOMOLOGADO | 9 JARs, somente 2 runtimes comprovados |
| HUD nativa | NÃO IMPLEMENTADO | Swing externa permanece |
| Updater de conteúdo | NÃO IMPLEMENTADO | inventário/resolve/rollback ausentes |
| Crash/Doctor/CAS | NÃO IMPLEMENTADO | contratos ainda precisam preceder Tools |
| Gemini Tools | PROTÓTIPO | existe chat, não executor seguro |
| Master/Creator | IMPLEMENTAR FUTURAMENTE | depende de manifesto e autorização |
| Multiplayer/social | IMPLEMENTAR FUTURAMENTE | primeira versão aceita PoC com conexão direta e providers externos opcionais |

## Decisões recomendadas

- **MANTER:** engine Rust própria; Prism, MultiMC e launchers maduros como referências e fontes licenciadas avaliadas por arquivo.
- **MELHORAR:** Firebase Auth/Firestore + Worker enquanto couberem nos limites gratuitos; impor quotas e degradação explícita.
- **SUBSTITUIR:** Swing por UI nativa com core de estado/protocolo compartilhado e adaptadores por versão/loader.
- **REMOVER:** Firebase Functions somente após busca de chamadas e período de depreciação documentado.
- **IMPLEMENTAR FUTURAMENTE:** Master e Social, começando pelo Master Panel mínimo e por um PoC multiplayer/social medido.
- **ADIAR:**  relay próprio, CAS remoto e Tools mutáveis.

## Próxima wave segura

Quatro trilhos com ownership exclusivo: fechar regras Firestore; regularizar licença do monorepo; endurecer CI; congelar proveniência/versionamento e reconciliar documentação pública. Só após esse gate entram timeout/limites de rede, harness runtime, substituição atômica e primeira decomposição do `App.tsx`. HUD nativa começa sobre esse baseline observável.
