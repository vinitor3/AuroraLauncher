# Armazenamento de skins e capas

O launcher salva a skin e o modelo no computador antes de sincronizar. Para usar uma skin localmente, abra **Guarda-roupa → Importar**, selecione o PNG e clique em **Aplicar skin**. O Aurora Skins recebe a cópia local ao iniciar o jogo.

O Worker recebe o PNG e o token Firebase em `POST /v1/appearance`, valida a conta e a imagem e envia o arquivo ao bucket público `aurora-appearance` do Supabase. A chave privilegiada fica no secret `SUPABASE_API_KEY_SERVICE_ROLE` do Worker. Se o envio falhar, o launcher tenta o Firebase Storage.

Quando ambos os armazenamentos falham, uma skin selecionada em **Explorar** pode usar o endereço público da imagem no perfil. O launcher confere a origem e o SHA-256 da cópia equipada e do PNG servido pelo Worker. Arquivos pessoais e links colados manualmente continuam equipados no computador. **Aplicar skin** repete a sincronização.

## Situação observada em 24 de novembro de 2026

O `/health/storage` respondeu HTTP 503. O DNS público retornou `NXDOMAIN` para `pgbzkinjfwbncifurirb.supabase.co`. A API do Google Cloud Storage informou que o bucket alternativo `auroralauncher.firebasestorage.app` não existe. O catálogo público de skins usa um serviço independente.

O responsável pelo projeto pode conferir a referência e o estado do projeto no painel do Supabase. Após restabelecer o serviço, consulte `/health/storage` e execute `node scripts/supabase-appearance-smoke.mjs` com `AURORA_SMOKE_FIREBASE_ID_TOKEN` de uma conta de teste. O teste verifica o upload autenticado e compara o PNG baixado com o original. [Diagnóstico de NXDOMAIN no Supabase](https://supabase.com/kb/troubleshooting/nxdomain-error-connecting-to-a-supabase-project)
