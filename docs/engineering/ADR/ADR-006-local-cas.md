# ADR-006 — CAS local SHA-256 com índice transacional

## Contexto

Na proposta original, downloads já validavam hashes, mas updates/snapshots/rollback ainda não existiam. Atualizações, migração de Companion e ações do Assistente passaram a ter mecanismos próprios de journal/rollback. Cópias completas desperdiçam disco e GC incorreto pode destruir instâncias.

## Opções avaliadas

1. backups de diretório;
2. CAS somente em arquivos/JSON;
3. blobs SHA-256 + manifestos versionados + índice SQLite.

## Vantagens e desvantagens

Backups são simples e caros. JSON reduz dependência, mas torna referência/GC transacional difícil. SQLite oferece ACID local e consultas, adicionando schema/migrações.

## Riscos

Power loss, blob corrompido, hardlink incompatível, GC de objeto vivo e crescimento sem limite.

## Decisão recomendada

**MELHORAR incrementalmente**: primeiro snapshot/journal mínimo por update; depois opção 3. Layout `blobs/sha256/<prefix>/<digest>`, descritor com digest/tamanho/tipo, manifestos imutáveis e SQLite `FULL` durability. GC mark-and-sweep, dry-run e raízes de instâncias/snapshots retidos.

## Consequências

A unificação de Update/Doctor/Tools num executor transacional permanece uma direção, não uma descrição do código atual. Mundos/configs mutáveis não viram CAS integral por padrão.

## Adoção limitada em 29/09/2026

A [ADR-027](ADR-027-manual-instance-recovery.md) aplica CAS SHA-256, manifestos versionados e índice transacional aos pontos manuais de Recuperação. Os backups, snapshots e journals existentes não são migrados nesta entrega. Configurações locais só entram no escopo explícito de um ponto solicitado pelo jogador; mundos continuam excluídos. A exclusão de pontos não executa coleta de blobs: conteúdo sem referências permanece conservado até haver uma coleta verificável. O formato efetivo e os limites estão em [Recuperação de instâncias](../../instance-recovery.md).

## Reversibilidade

Alta: materializar manifesto em diretório convencional; índice pode ser reconstruído dos manifests/blobs.
