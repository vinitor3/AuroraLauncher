# Executive Summary

## Resultado

O Aurora é um **alpha funcional avançado do launcher**. A base documental desta revisão é `0.1.1-alpha.14`, commit `6a107b87adc4882bd464909e22650185f862ea5e`, conferida em 29/09/2026. A Fase 1.3 de atualizações de conteúdo, Doctor, ações locais confirmadas, recuperação manual e cópias de diagnóstico já possuem implementação. Segurança de identidade/dados, licença, entitlement, CI, proveniência e homologação runtime continuam sendo gates antes de distribuição pública. Aurora Connect é uma proposta R4, sem implementação ou homologação demonstrada nesta revisão.

## Baseline confirmado

- Git ativo, CI pública existente e releases locais imutáveis com SHA-256.
- Frontend, Rust, Worker, Functions e as matrizes Core/Skins fazem
  parte do preflight obrigatório do instalador.
- A matriz ativa tem nove Core e nove Skins (18 JARs);
  build não equivale à homologação runtime das nove combinações.
- O updater separa descoberta e aplicação, revalida estado remoto/local,
  verifica hash/tamanho e usa journal, staging, snapshot mínimo e rollback.
- Doctor mantém análise local somente leitura; ações confirmadas possuem broker,
  prévia, autorização, histórico e restauração próprios.
- Recuperação manual usa CAS SHA-256/SQLite limitado ao seu domínio. A alpha.14
  acrescenta histórico verificável e cópias de teste com aplicação seletiva,
  proteção e desfazer. Pontos automáticos gerais de updates e coleta de blobs
  continuam pendentes; mundos não pertencem ao escopo de restauração.

Fontes atuais: [Doctor](../aurora-doctor.md), [Tools](../aurora-ai-tools.md),
[recuperação](../instance-recovery.md), [cópias de teste](../diagnostic-lab.md),
[evidências alpha.14](../task-aur-1167-doctor-lab.md) e [releases preservadas](../../releases/README.md).
Os resultados citados foram executados nas entregas registradas; esta revisão
documental não os repetiu nem homologou Minecraft, UI nativa ou serviços online.

## Correções ao relatório de 29/08

O relatório dizia que Git, licença e CI não existiam. Isso ficou desatualizado: os três já existem. Ainda não estão encerrados:

- a CI não exige `rustfmt`, `clippy`, build dos módulos Java nem matriz runtime;
- `LICENSE.md` não licencia claramente todo o monorepo e não contém o texto integral da GPL;
- o release histórico `v0.1.0-alpha` publicado e o instalador rastreado no tag tinham bytes diferentes: Git `95C2708E…D9AAF8` (5.029.160 bytes) versus GitHub `582E5BA2…465A9` (5.028.702 bytes); esse achado não descreve o instalador alpha.14;
- versões do launcher são incrementadas em conjunto e um nome de release já
  utilizado é bloqueado pelo preflight.

## Bloqueador de segurança encontrado

O registro original identificou criação de `/users/{uid}` com `role` escolhido pelo cliente e documentou correção local para `PLAYER`, UID/e-mail sintético coerentes, allowlist de campos e estatísticas iniciais, com sete testes no Firestore Emulator. Preserva-se o gate de auditoria dos documentos existentes e implantação explícita das regras. O estado atual de produção não foi consultado nesta revisão; o resultado histórico local não comprova a correção do ambiente publicado.

## Estado por área

| Área | Estado | Evidência / lacuna |
| --- | --- | --- |
| Desktop e engine | PARCIAL | builds/testes registrados e features extraídas; cobertura E2E completa e modularização adicional pendentes |
| Downloads | IMPLEMENTADO MAS NÃO HOMOLOGADO | concorrência/hash/rename testados; adoção de CAS pelo downloader e falhas extensas continuam etapas próprias |
| Assistente desktop | IMPLEMENTADO MAS NÃO HOMOLOGADO | painel, Gemini, voz e atalho preservados; serviços online/runtime precisam de evidência própria |
| Core/Skins | PARCIAL | matriz de 18 JARs; homologação completa nas nove combinações continua pendente |
| Updater de conteúdo | IMPLEMENTADO MAS NÃO HOMOLOGADO | inventário, resolução compatível, staging, integridade, snapshot e rollback; teste em instância real pendente |
| Doctor e histórico | IMPLEMENTADO MAS NÃO HOMOLOGADO | diagnóstico local, orientação opcional e observações por tentativa; associação temporal não comprova causa |
| Recuperação/CAS/cópias de teste | IMPLEMENTADO MAS NÃO HOMOLOGADO | pontos manuais, CAS restrito e aplicação seletiva com desfazer; sem GC ou proteção automática geral antes de updates |
| Gemini Tools | IMPLEMENTADO MAS NÃO HOMOLOGADO | `remove_mod` e `set_setting` restritos, plano/diff, confirmação e journals; sem shell, escrita arbitrária ou restauração por IA |
| Master/Creator | NÃO IMPLEMENTADO | evolução futura depende de manifesto e autorização |
| Aurora Connect/multiplayer/social | NÃO IMPLEMENTADO | proposta TCP/UDP e voz por mods existentes; conexão direta preferida e relay gratuito condicionado |

## Decisões recomendadas

- **MANTER:** engine Rust própria; Prism, MultiMC e launchers maduros como referências e fontes licenciadas avaliadas por arquivo.
- **MELHORAR:** Firebase Auth/Firestore + Worker enquanto couberem nos limites gratuitos; impor quotas e degradação explícita.
- **RETIRADA DECIDIDA:** mod Companion/Swing e ponte in-game, preservando integralmente o Assistente desktop, conforme ADR-018.
- **REMOVER:** Firebase Functions somente após busca de chamadas e período de depreciação documentado.
- **IMPLEMENTAR FUTURAMENTE:** Master e Social conforme seus gates; R4 segue a [especificação Connect V2](AURORA_CONNECT_SPEC.md) e a [ADR candidata](ADR/PROPOSAL-aurora-connect.md), incluindo UDP, Simple Voice Chat e Plasmo Voice separadamente.
- **PRESERVAR E HOMOLOGAR:** Tools confirmadas, recuperação/CAS limitado e cópias de teste já implementadas, sem ampliar silenciosamente seus escopos.
- **ADIAR:** infraestrutura relay própria obrigatória, CAS remoto, GC não validado e Tools amplas/nível 3. Relay gratuito só pode ser habilitado após verificação de termos, quotas, segurança e custo para o uso pretendido; software livre não assegura infraestrutura gratuita.

## Próxima wave segura

Fechar regras Firestore, licença/proveniência e CI; homologar o conjunto
Core/Skins e executar atualização/rollback em instâncias reais. O Assistente desktop precisa também de validação independente de Minecraft. O updater assinado do próprio launcher continua separado da Fase 1.3.

Preservar e testar os incrementos alpha.14 antes de ampliar recovery/Tools.
Uma PoC Connect isolada deve respeitar ownership e gates, sem substituir essa
homologação nem habilitar faturamento, publicação ou capacidades experimentais
ao usuário por padrão. A aprovação de TCP não aprova UDP, voz, relay ou R4 inteira.
