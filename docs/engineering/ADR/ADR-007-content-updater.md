# ADR-007 — Updater planificado, compatível e reversível

## Contexto

Instalação existe, mas não há detecção/troca segura de versões. Modrinth fornece IDs/hashes/dependências; CurseForge pode negar download de terceiros.

## Opções avaliadas

1. comparar filename/versão;
2. substituir automaticamente pelo “latest”;
3. inventário normalizado + resolver puro + executor transacional.

## Vantagens e desvantagens

Automação simples é rápida e perigosa. A opção 3 exige metadados persistidos, mas distingue seguro/ambíguo/bloqueado e suporta rollback.

## Riscos

Trocar loader/game, dependência incompatível, downgrade, perda de config e arquivo bloqueado pelo autor.

## Decisão recomendada

**IMPLEMENTAR** opção 3, Modrinth primeiro e mod individual antes de modpack. `UpdatePlan` não toca disco. Usuário confirma diff/changelog; executor faz snapshot, staging, hash e commit. CurseForge mantém fluxo autorizado/manual quando `downloadUrl` não estiver disponível.

## Consequências

CAS pode entrar depois sem alterar contrato do resolver. Nenhum “update all” antes de falsos positivos medidos.

## Reversibilidade

Alta via manifesto anterior e snapshot.

