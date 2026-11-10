# Aurora Connect — implementação

10/11/2026: IMPLEMENTADO MAS NÃO HOMOLOGADO. Nove pares Core/Skins e Assistente preservados. “Online” é o serviço Aurora; servidores autorizados com `online-mode=false` continuam no escopo para perfis originais/offline, com autenticação Connect obrigatória.

| Área | Evidência e limite atual |
|---|---|
| TCP | Dois ConnectManagers neste PC + SDK, Firebase e Worker publicados: convite privado, STATUS/pong da fonte de teste e 1 MiB íntegro por direção passaram. |
| UDP | NativeVoice8 anterior: oito participantes locais a 50 datagramas/s, retorno isolado, cinco ensaios passaram. Não repetido nesta rodada TCP. |
| Direto | IPv6 externo anterior preservado; novo teste usa o mesmo PC. IPv4/NAT e outras redes ainda precisam de validação externa. RTT observado não garante menor latência absoluta. |
| Relay | Serviço existente ativo, limites gratuitos preservados; sem novos recursos/faturamento. Cota não comporta oito vozes no pior caso. Novo teste privado não homologa relay. |
| Amigos / grupos / convites | Busca por nick Aurora, catálogo, Entrar, grupos e pedidos implementados. Privados por conta: receber, entrar, recusar, revogar, reenviar e limpar expirados; oito destinatários por anúncio, prazo original preservado. Online ativo no build experimental; build comum mantém a opção desligada. |
| Minecraft | GUI anterior Fabric 1.21.1 com Core/Skins entrou pelo HostPlayer Social, SVC e Plasmo por 191s cada. Novo teste usa fonte própria de laboratório; não é entrada em mundo. Nove clientes completos não homologados. |
| Simple Voice Chat | Nove servidores oficiais preparados/testados; cliente real autenticou UDP pela ponte. Áudio audível não homologado. |
| Plasmo Voice | Oito servidores oficiais testados; cliente Fabric 1.21.1/Plasmo 2.1.17 entrou e conectou UDP nos dois lados. Áudio não homologado; não há versão oficial Forge 1.12.2. |
| Interface | Guia e painel estão no executável `629cb9ec…` (`ba3c0b0`). Conta de teste autenticada mostrou “Lista atualizada” após ativação. Ingresso privado completo pela GUI ainda precisa de teste. |

Incremento atual: novo runner `scripts/connect-private-online-smoke.mjs`, helper de limpeza restrita e testes; processo nativo somente libtest em `connect/social.rs`, `social_online_process_tests.rs` e `connect_discovery/tests.rs`. SDK real, autoridade compilada, tokens só em memória/stdin e origem própria exclusivamente loopback. 23 testes Node, cinco Rust, Clippy/fmt e revisão independente passaram.

Teste online real: revogação confirmada no SDK servidor; convidado encerrado e removido do grupo em 3925ms, anfitrião original saudável e porta reutilizável. Código terminal genérico `connect_transport_failed`: não prova causalidade exclusiva. Duas contas descartáveis e todos os 12 documentos ficaram ausentes, confirmados por leitura independente. Sem processos próprios remanescentes ou alteração do perfil pessoal.

Comandos executados: `node --test scripts/connect-private-online-smoke.test.mjs scripts/connect-private-online-owned-cleanup.test.mjs` (suítes focais), Cargo test/clippy/fmt e `node scripts/connect-private-online-smoke.mjs --run --binary <libtest revisado> --sha256 5a7f3506964ab29bcb92045651bf5ee2ca8146f12870a0fc6665aa660f81f7ff`. Recibos em `.aurora/connect-validation/social-private-online-process-20261110-result.json` e `private-online-smoke-aeeeac1c-1423-4bf6-ab1b-2757b63558e8/{result.json,cleanup-verification.json}`. Suítes anteriores e lista de arquivos permanecem em `social-private-online-20261110/implementation-result.json`.

Ativação autorizada: `rules-update.mjs apply` e API Cloudflare de versões/deployments. Worker `93bb0871-d139-4dfc-814c-5576f7c69a98` a 100%, bundle `3c7ddd31…`; Firestore `ad9f8fae-5614-4bbb-9f0f-ec75dd9f196a`, SHA `4ba9f923…`. Três namespaces e limites preservados; acrescentada somente a flag privada.

Rollback preparado em `social-private-online-20261110/`: Worker `38d190c1-e163-455a-9b06-069b80f1f5aa` e regras `9810d06c-1a9f-4e57-b4ed-4d61cc0d6ff6`, somente após conferir a versão atual. Revalidação antes do deploy não é CAS; rollback não apaga dados. Limpeza do laboratório usa journal/hash e CAS Firestore; recuperação Auth por UID não tem CAS, e signup sem UID exige revisão restrita.

Pendentes: ingresso privado completo pela GUI, áudio real, duas máquinas/contas mistas, outras redes e estabilidade longa. Matriz/18 JARs preservados não equivalem a homologação. Sem push, release ou instalador.
