# Armazenamento de skins e capas

O launcher salva a skin e o modelo no computador antes de sincronizar. Para usar uma skin localmente, abra **Guarda-roupa → Importar**, selecione o PNG e clique em **Aplicar skin**. O Aurora Skins recebe a cópia local ao iniciar o jogo.

O Worker recebe o PNG e o token Firebase em `POST /v1/appearance`, valida a conta e a imagem e envia o arquivo ao bucket público `aurora-appearance` do Supabase. A chave privilegiada fica no secret `SUPABASE_API_KEY_SERVICE_ROLE` do Worker. Se o envio falhar, o launcher tenta o Firebase Storage.

Quando ambos os armazenamentos falham, uma skin selecionada em **Explorar** pode usar o endereço público da imagem no perfil. O launcher confere a origem e o SHA-256 da cópia equipada e do PNG servido pelo Worker. Arquivos pessoais e links colados manualmente continuam equipados no computador. **Aplicar skin** repete a sincronização.

## Usar outro projeto Supabase

1. No painel Supabase, abra **Connect** e copie a referência do projeto. É o identificador de 20 caracteres antes de `.supabase.co`.
2. Em `apps/edge-proxy/wrangler.toml`, defina `SUPABASE_PROJECT_REF` sob `[vars]` com essa referência. Cadastre a chave `sb_secret_…` do mesmo projeto no secret `SUPABASE_API_KEY_SERVICE_ROLE` do Worker. `SUPABASE_API_KEY` é o nome de secret aceito por instalações anteriores. [Chaves do Supabase](https://supabase.com/docs/guides/getting-started/migrating-to-new-api-keys)
3. Implante o Worker e consulte `/health/storage`. O estado saudável confirma que `aurora-appearance` é um bucket público. O primeiro upload autenticado pode preparar esse bucket quando ele ainda não existe. A troca de projeto afeta uploads futuros. Os arquivos já publicados continuam nos endereços originais.

Um `service_role` JWT legado pode fornecer a referência no próprio secret. Se ela divergir da configuração, o Worker responde 503 antes de enviar a chave. Sem referência explícita nem referência no secret, a implantação conserva o projeto histórico `pgbzkinjfwbncifurirb`. O desktop recebe apenas o endereço público do PNG.

## Verificar a sincronização

Abra `/health/storage` para conferir o bucket. Com uma conta de teste, execute `node scripts/supabase-appearance-smoke.mjs` com o token em `AURORA_SMOKE_FIREBASE_ID_TOKEN`. O teste envia um PNG e compara o arquivo recuperado com o original. Se a referência deixar de resolver, confira o estado do projeto no painel Supabase e aguarde a propagação do DNS após reativá-lo. [Diagnóstico de NXDOMAIN no Supabase](https://supabase.com/kb/troubleshooting/nxdomain-error-connecting-to-a-supabase-project)
