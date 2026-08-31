# ADR-003 — Firebase para identidade; aparência sob quota explícita

## Contexto

Firebase Auth/Firestore guarda identidade/perfil. Supabase público guarda skin/capa por Worker. O objetivo é custo zero, mas Supabase Free limita storage/egress e pode pausar/restringir.

## Opções avaliadas

1. manter split atual;
2. mover aparência a Firebase Storage;
3. mover aparência a Cloudflare R2;
4. aparência apenas local.

## Vantagens e desvantagens

Manter evita migração e já funciona. R2 oferece 10 GB-mês e egress sem tarifa, mas exige nova operação/conta e política. Local é mais privado/barato, porém o jogo/remoto precisa receber bytes.

## Riscos

Bucket público, retenção indefinida, quota inesperada e duas autoridades de dados. Rules de role eram vulneráveis e foram endurecidas localmente.

## Decisão recomendada

**MANTER temporariamente** Firebase + Supabase com limites, remoção e modo local. **PESQUISAR MIGRAÇÃO** de objetos públicos para R2 apenas quando a quota real justificar. Firestore é a autoridade de identidade; storage nunca decide autorização.

## Consequências

Instrumentar contagem/bytes sem PII e impedir cobrança silenciosa. URL pública é dado, não credencial.

## Reversibilidade

Alta com interface `AppearanceStorage` e migração por hash.

