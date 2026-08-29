# Supabase Storage para skins e capas

Projeto: `https://pgbzkinjfwbncifurirb.supabase.co`.

O desktop nunca acessa o Supabase com uma chave privilegiada. Ele envia o PNG
e o Firebase ID token ao endpoint `/v1/appearance` do Worker. O Worker valida a
assinatura, o usuário, o limite de 5 MB e as dimensões antes de gravar no bucket
público `aurora-appearance`. O nome do objeto usa SHA-256 para não sobrescrever
o cache de uma imagem anterior.

Configure `SUPABASE_API_KEY` no Cloudflare com uma **Secret key** atual ou a
chave legada **service_role**. Uma chave publishable/anon não serve: a criação
do bucket e o upload serão corretamente bloqueados por RLS. A chave privilegiada
deve permanecer apenas no secret do Worker.

O Worker usa `SUPABASE_API_KEY_SERVICE_ROLE` para criar/verificar o bucket e
fazer uploads. `SUPABASE_API_KEY` permanece somente como fallback de migração.
O launcher tenta Supabase primeiro e preserva uma cópia local da skin para que
uma indisponibilidade temporária não impeça o jogo de iniciar.

## Estado verificado em 28 de agosto de 2026

O endpoint `/health/storage` confirmou o bucket público `aurora-appearance` e o
secret `SUPABASE_API_KEY_SERVICE_ROLE`. O Worker publicado prioriza essa chave
para operações privilegiadas. A skin selecionada é salva localmente e enviada
ao Supabase; se a sincronização falhar, a cópia local continua utilizável no
Companion 1.20.1.
