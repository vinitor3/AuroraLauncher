# ADR-006 — CAS local SHA-256 com índice transacional

## Contexto

Downloads já validam hashes, mas updates/snapshots/rollback não existem. Cópias completas desperdiçam disco e GC incorreto pode destruir instâncias.

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

Update/Doctor/Tools usam o mesmo executor transacional. Mundos/configs mutáveis não viram CAS integral por padrão.

## Reversibilidade

Alta: materializar manifesto em diretório convencional; índice pode ser reconstruído dos manifests/blobs.

