# Configuração Firebase — Fase 0

1. Crie um projeto no Firebase e habilite **Authentication > Email/Password**.
2. Crie um banco **Cloud Firestore** em modo de produção.
3. Registre um aplicativo Web. Ao abrir o Aurora pela primeira vez, cole o objeto de configuração pública mostrado pelo Firebase na tela **Conecte o Firebase**. Ele fica em `firebase-public-config.json` dentro dos dados locais do aplicativo, fora do instalador e do código-fonte.
4. Publique `firebase/firestore.rules` no Firestore. As regras permitem que cada pessoa leia e atualize apenas seu próprio perfil e tornam o registro de nick imutável. A biblioteca de skins fica no IndexedDB local do launcher; apenas a skin equipada permanece no perfil online.
5. O destino principal de skins/capas é o Supabase via Worker. O Firebase
   Storage é apenas um fallback de migração opcional; a cópia local da skin
   continua disponível mesmo sem ele.
6. Para ativar o assistente e o catálogo protegido, configure `VITE_AURORA_API_URL` no ambiente de build do desktop. Essa URL pública é empacotada no launcher e não aparece para o jogador. As chaves `GEMINI_API_KEY`, `CURSEFORGE_API_KEY` e `SUPABASE_API_KEY` ficam somente no Worker e nunca no aplicativo desktop.

O Aurora usa `nick@aurora.internal` somente como identificador interno do Firebase Auth. A pessoa entra sempre com nick e senha; o e-mail sintético não é mostrado na interface.

As chaves públicas identificam o projeto e podem estar no aplicativo. Nunca adicione chaves de conta de serviço, credenciais administrativas ou tokens privados ao cliente.
