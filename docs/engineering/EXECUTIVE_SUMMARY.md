# Executive Summary

## Resultado

O Aurora é um **alpha funcional avançado do launcher**, não um produto multiplayer pronto. A fundação desktop é real e a Fase 1.3 de atualizações de conteúdo foi implementada no código em 2026-09-02. Segurança de identidade/dados, CI, proveniência, homologação runtime do Companion e homologação do updater continuam sendo gates antes de distribuição pública.

## Baseline confirmado

- Git ativo, CI pública existente e releases locais imutáveis com SHA-256.
- Frontend, Rust, Worker, Functions e as matrizes Core/Skins/Companion fazem
  parte do preflight obrigatório do instalador.
- Core 1.1.2, Skins 0.2.1 e Companion 0.2.3 possuem nove JARs versionados cada;
  build não equivale à homologação runtime das nove combinações.
- O updater separa descoberta e aplicação, revalida estado remoto/local,
  verifica hash/tamanho e usa journal, staging, snapshot mínimo e rollback.

## Correções ao relatório de 29/08

O relatório dizia que Git, licença e CI não existiam. Isso ficou desatualizado: os três já existem. Ainda não estão encerrados:

- a CI não exige `rustfmt`, `clippy`, build do Companion nem matriz runtime;
- `LICENSE.md` não licencia claramente todo o monorepo e não contém o texto integral da GPL;
- o release publicado e o instalador rastreado no tag têm bytes diferentes: Git `95C2708E…D9AAF8` (5.029.160 bytes) versus GitHub `582E5BA2…465A9` (5.028.702 bytes);
- versões do launcher são incrementadas em conjunto e um nome de release já
  utilizado é bloqueado pelo preflight.

## Bloqueador de segurança encontrado

A regra original de criação de `/users/{uid}` aceitava um `role` escolhido pelo próprio cliente. Um usuário autenticado podia criar o perfil como `ADMIN`. O patch local agora exige `PLAYER`, UID/e-mail sintético coerentes, allowlist exata de campos e estatísticas iniciais. Sete testes comportamentais positivos/negativos passam no Firestore Emulator. Produção continua potencialmente vulnerável até a auditoria dos documentos existentes e o deployment explícito das regras.

## Estado por área

| Área | Estado | Evidência / lacuna |
| --- | --- | --- |
| Desktop e engine | PARCIAL | build/testes verdes; E2E e modularização ausentes |
| Downloads | IMPLEMENTADO MAS NÃO HOMOLOGADO | concorrência/hash/rename testados; falhas extensas e CAS futuros |
| Companion | IMPLEMENTADO MAS NÃO HOMOLOGADO | 9 JARs 0.2.0; 0/9 runtimes completos, com correções apenas na fonte seguinte |
| HUD nativa | NÃO IMPLEMENTADO | Swing externa permanece |
| Updater de conteúdo | IMPLEMENTADO MAS NÃO HOMOLOGADO | inventário, resolução compatível, staging, integridade, snapshot e rollback; teste em instância real pendente |
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

Fechar regras Firestore, licença/proveniência e CI; homologar o conjunto
Core/Skins/Companion e executar atualização/rollback em instâncias reais. A HUD
nativa e o updater assinado do próprio launcher continuam separados da Fase
1.3.
