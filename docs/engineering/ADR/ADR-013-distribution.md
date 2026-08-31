# ADR-013 — Releases GitHub imutáveis e updater assinado

## Contexto

O release atual possui dois instaladores diferentes sob `0.1.0-alpha`. GitHub Actions é gratuito para runners padrão em repositório público; assinatura Windows tradicional pode custar.

## Opções avaliadas

1. builds manuais sobrescritas;
2. GitHub tag/release por versão com hashes;
3. infraestrutura própria de artefatos.

## Vantagens e desvantagens

GitHub resolve hosting/CI sem custo e liga fonte a tag. Não substitui code signing. Infra própria adiciona custo/operação.

## Riscos

Tag mutável, secret de assinatura vazado, artifact/action comprometida, downgrade e SmartScreen.

## Decisão recomendada

**SUBSTITUIR** fluxo manual pela opção 2: tag anotada, build limpo, `SHA256SUMS`, SBOM, provenance e release imutável. Próxima versão `0.2.0-alpha.1`. Updater Tauri somente com assinatura fail-closed. Enquanto não houver certificado Windows gratuito confiável, declarar instalador não assinado.

## Consequências

Nunca corrigir asset in-place; publicar patch/prerelease novo. Actions pinadas e permissões mínimas.

## Reversibilidade

Alta; artefatos podem migrar mantendo manifestos/hashes.

