# Segurança

## Relatando uma vulnerabilidade

Não publique detalhes exploráveis em issues. Use contato privado do mantenedor e informe componente, impacto, versão/hash, passos mínimos e prova de conceito segura. Não acesse contas, instâncias ou dados de terceiros e não envie senha, token, cookie, chave, mundo ou arquivo pessoal.

## Escopo e modelo de ameaça

Aurora processa código/mods de terceiros, arquivos compactados, manifestos, logs, imagens, conteúdo web, tokens de identidade e saída de IA. Todos são não confiáveis. Atacantes relevantes:

- mod/modpack ou manifesto malicioso;
- página/API/CDN comprometida;
- processo local no mesmo usuário;
- conta autenticada tentando elevar papel/acessar outro perfil;
- prompt injection em log, nome, página ou resposta remota;
- release/action/dependência comprometida;
- erro operacional que publica secret ou artefato errado.

Ativos protegidos: contas, tokens, mundos/instâncias, filesystem fora da instância, identidade de release, chaves backend, screenshots e disponibilidade/cota gratuita.

## Controles obrigatórios

### Identidade e backend

- Segredos Gemini, CurseForge e Supabase ficam somente no Worker/secrets.
- Regras Firestore validam campos/tipos e forçam novos usuários a `PLAYER`; promoção de role nunca é escrita pelo cliente.
- Toda rota privilegiada valida JWT, audiência/projeto, expiração e autorização própria; role do documento não substitui autorização server-side.
- Rate limit em memória é apenas mitigação alpha, não controle distribuído confiável.

### Desktop, arquivos e downloads

- Tauri expõe comandos estreitos; não há comando de shell genérico.
- IDs/nomes são normalizados, paths são confinados e ZIPs usam caminhos fechados contra traversal.
- Download deve exigir HTTPS quando aplicável, tamanho/hash conhecido, staging e commit que preserve o destino anterior em qualquer falha. A implementação atual ainda não satisfaz integralmente esse requisito.
- Updater futuro exige manifesto assinado, hash, canal explícito e rejeição fail-closed.

### Aurora Core e IPC

- JWT/senha/chave nunca entram na JVM.
- Socket apenas em `127.0.0.1`, porta efêmera, nonce novo e handshake antes de mensagens.
- O nonce atual na linha de comando é observável por processos locais do mesmo usuário: não tratá-lo como segredo de alto valor; estudar canal herdado/arquivo ACL e autenticação de processo antes de ampliar capabilities.
- IPC remoto, túnel ou convite usa protocolo e credenciais separados.

### Diagnóstico

- Trate logs, nomes de arquivos e mensagens de mods como dados externos. A ajuda para IA gera texto local para o usuário revisar e copiar.
- Proteja dados de conta e caminhos pessoais. O filtro auxilia a revisão e a pessoa escolhe o conteúdo e o destinatário.

### Privacidade e custo

- Coletar o mínimo; telemetria/crash remoto somente opt-in.
- Aparência pública exige chave não enumerável, remoção/retenção e aviso ao usuário.
- Serviços gratuitos devem falhar em modo degradado quando a cota acabar; nunca habilitar cobrança automática silenciosa.

## Gates de segurança

- Master Panel bloqueado até teste de rules/roles no Emulator e administração server-side.
- Multiplayer social bloqueado até autenticação de sessão, bloqueio/denúncia e threat model.
- Auto-updater bloqueado até assinatura end-to-end e rotação/recuperação de chave documentadas.

## Versões suportadas

Durante a beta, correções chegam à `main`; builds anteriores podem ficar sem correção. Confirme tag, SHA-256 e canal. Compilação de Core/Skins não equivale a suporte runtime; consulte `docs/engineering/COMPATIBILITY_MATRIX.md`.
