# Configuração Firebase — Fase 0

1. Crie um projeto no Firebase e habilite **Authentication > Email/Password**.
2. Crie um banco **Cloud Firestore** em modo de produção.
3. Registre um aplicativo Web. A configuração pública do projeto oficial já é
   empacotada no Aurora, então jogadores não precisam configurá-la na primeira
   abertura. Para usar outro projeto em desenvolvimento, defina as variáveis
   `VITE_FIREBASE_*` de `apps/desktop/.env.example` em um `.env.local`. Um
   `firebase-public-config.json` antigo nos dados locais continua sendo aceito
   como substituição para compatibilidade.
4. Publique `firebase/firestore.rules` no Firestore. As regras permitem que cada pessoa leia e atualize apenas seu próprio perfil e tornam o registro de nick imutável. A biblioteca de skins fica no IndexedDB local do launcher; apenas a skin equipada permanece no perfil online.
5. O destino principal de skins/capas é o Supabase via Worker. O Firebase
   Storage é apenas um fallback de migração opcional; a cópia local da skin
   continua disponível mesmo sem ele.
6. Configure `VITE_AURORA_API_URL` no ambiente de build para os serviços de catálogo e aparências. A URL pública é empacotada no launcher. As credenciais dos provedores ficam no Worker.

## Recuperar a conta Aurora

No primeiro cadastro, o Firebase Auth usa `nick@aurora.internal` como identificador interno. Contas antigas e novas podem abrir **Recuperação da conta**, digitar a senha atual e confirmar um endereço pessoal pelo link recebido. Até confirmar, a pessoa entra com o nick. Após confirmar, entra pelo e-mail e conserva o mesmo UID, nick no jogo, amizades e aparências. O endereço pessoal fica no Firebase Auth.

Na tela de entrada, **Esqueci minha senha** envia um link de uso único ao e-mail vinculado. O aplicativo exibe a mesma confirmação para um endereço desconhecido. Uma conta que só tem o identificador sintético precisa da senha atual para vincular um endereço.

Use o provedor **E-mail/senha**, a proteção contra enumeração de e-mails, um domínio autorizado para as ações e os modelos de confirmação de troca de e-mail e redefinição de senha. O manipulador padrão do Firebase conclui as ações no navegador. [Documentação do Firebase Auth](https://firebase.google.com/docs/auth/custom-email-handler)

No projeto oficial, o nome de remetente publicado no modelo de verificação é **Aurora Smart Launcher**. A página dos links de conta fica em **auroralauncher.firebaseapp.com**. O Firebase fornece o assunto e o corpo desse modelo. O vínculo de um endereço a um nick usa a operação `VERIFY_AND_CHANGE_EMAIL`, que a API administrativa não expõe como modelo separado. [Modelos disponíveis na Identity Platform](https://docs.cloud.google.com/identity-platform/docs/reference/rest/v2/Config#SendEmail)

As chaves públicas identificam o projeto e podem estar no aplicativo. Nunca adicione chaves de conta de serviço, credenciais administrativas ou tokens privados ao cliente.
