# ADR-030 — Prova limitada entre conta Aurora e chave do Connect

Data: 25/10/2026. Estado: **PROTÓTIPO**, implementação e ensaio integrado locais em [88A](https://github.com/vinitor3/AuroraLauncher/blob/088bf5b435b1b63e842c11ff1c01648b5e38bced/docs/task-aur-r4-connect-social-88a.md). Complementa a ADR-029 e os IDs R4-005/008. Não altera a matriz, prioridades ou gates.

## Problema e decisão

O catálogo social já possui regras de amizade, bloqueio e visibilidade. O transporte existente conhece uma chave pública e uma autorização individual, mas a janela nativa não autentica um UID Aurora. Portanto, ler um anúncio ou informar um nick não pode autorizar a conexão.

O primeiro incremento 88A prova posse da chave privada conservada no preparo nativo e associa essa prova à conta autenticada no serviço social. O resultado é exclusivamente um vínculo temporário de conta/chave/operação/sessão. **Não é autorização de ingresso, não permite confirmar automaticamente o laboratório manual e não habilita Entrar.** Não há consumidor nativo de um recibo como passe de transporte neste incremento.

## Desafio e codec fechado

O desafio é um objeto JSON plano, sem campos adicionais e de até 2048 bytes. Campos, nesta ordem canônica:

`schemaVersion`, `purpose`, `issuerInstance`, `challengeId`, `accountId`, `operationId`, `generation`, `role`, `publicKey`, `sessionId`, `sessionDigest`, `issuedAtUnixMs`, `expiresAtUnixMs`.

- schemaVersion é 1; purpose é `accountKeyBinding`.
- issuerInstance e challengeId são 32 bytes aleatórios codificados em 64 caracteres hexadecimais minúsculos. O primeiro identifica uma instância efêmera da autoridade; o segundo identifica um único desafio.
- accountId corresponde a `[A-Za-z0-9_-]{1,128}` e provém exclusivamente da autenticação verificada no backend.
- operationId e sessionId são UUID v4 canônicos; generation é inteiro positivo representável exatamente em JavaScript; role é host ou guest.
- publicKey é Ed25519, 32 bytes hexadecimais minúsculos; sessionDigest é SHA-256 da projeção fechada normalizada lida pelo serviço, incluindo prazo e política declarada.
- Timestamps são inteiros positivos representáveis exatamente em JavaScript. Prazo máximo de 60 segundos, sem renovação por retry; verificação por relógio civil e monotônico no owner da autoridade.

A preimagem é o domínio UTF-8 `aurora/connect/social-key-proof/v1` seguido de um byte zero. Em seguida, para cada campo na ordem acima, entram seu comprimento UTF-8 em u16 big-endian e seus bytes; números usam decimal canônico sem sinal, expoente ou zeros à esquerda. A assinatura é Ed25519. Não há API para assinar digest arbitrário, payload genérico ou bytes escolhidos sem o schema fechado.

A prova é o objeto fechado `{schemaVersion:1, challenge:<objeto>, signature:<128 hex minúsculos>}`. O backend compara todo o desafio com o original retido; não reconstrói autoridade somente a partir da prova recebida. Um desafio é consumido antes de trabalho assíncrono de verificação e não pode ser usado simultaneamente, repetido, transferido entre UIDs ou reutilizado após reiniciar a autoridade.

## Autoridade e fronteiras locais

O núcleo do serviço recebe autenticador e leitor confiáveis explicitamente. Não altera o verificador RS256 existente do Worker e não adiciona fallback para tokens sem assinatura. A integração local tem adaptador separado, fixo no projeto `demo-aurora-social`, Auth Emulator `127.0.0.1:9187`, Firestore Emulator `127.0.0.1:8187` e servidor HTTP somente loopback. Tokens do Auth Emulator são próprios de teste e não demonstram validação criptográfica de conta em produção. Nenhum endpoint remoto é publicado.

Referências oficiais verificadas: [Firestore REST e identidade do solicitante](https://firebase.google.com/docs/firestore/use-rest-api) e [Auth Emulator e seus tokens de teste](https://firebase.google.com/docs/emulator-suite/connect_auth). O adaptador local também recusa `Origin` e `Host` diferentes do listener fixado; futura integração gráfica exige ponte estreita deliberada, não abertura genérica de CORS.

O leitor usa o token do próprio solicitante para consultar perfil Aurora, registro do nick e projeção da sessão pelo Firestore REST, sob as regras existentes. Não usa credencial administrativa nem lê o grafo social de terceiros. Consulta permitida comprova apenas visibilidade naquele instante. O backend confere que host possui a sessão e guest é outra conta. Após a prova, repete cadastro, leitura autorizada e digest; bloqueio, restrição ou alteração do anúncio invalidam o preparo.

Operações e respostas têm tamanho e prazo limitados. A autoridade mantém no máximo 128 pendências globais e quatro por UID, com controle de frequência por processo; recusa saturação sem expulsar pendência viva para permitir repetição. Esses limites são de uma instância local e **não** constituem rate limit distribuído, orçamento de produção ou garantia de cobrança zero. Nenhum token, senha, chave privada ou resposta privada integral entra em log, JVM, anúncio ou recibo.

O recibo contém identificador opaco, UID, chave pública, operação/sessão/contexto e prazo original, com `transportAuthorized:false`. Não é uma credencial assinada aceita pelo transporte. Uma integração futura exige cadeia de confiança própria, consumo e política atual; não pode promover esse objeto por conveniência.

## Nativo e ciclo de vida

O método estreito pertence ao mesmo ConnectManager/Attempt já coberto por stop, logout e saída. Somente preparo manual `sessionGameTcp` em loopback, ainda com Draft Host/Guest e chave RAM, pode produzir a prova inicial. Compara papel, chave, operação e geração com o próprio estado; respeita o deadline original de preparo e recusa desafio cujo prazo o ultrapasse. Cada tentativa emite no máximo uma prova. A função não altera a fase, abre sockets, inicia jogo ou renova a preparação.

O comando Tauri aceita somente a janela main. Continua indisponível no build sem `connect-lab`. O helper CLI separado usa a mesma biblioteca/codec, gera uma chave efêmera, imprime somente a identidade pública, lê um desafio limitado e emite uma prova; serve à interoperabilidade local e não substitui o teste do manager.

## Verificação e sequência

Testes obrigatórios: codec interoperável Rust/TypeScript; adulteração de UID/chave/papel/operação/geração/sessão/prazo/digest; objeto/limites; chave errada; expiração civil/monotônica; replay e concorrência; autoridade reiniciada; cadastro Aurora ausente; bloqueio/restrição após desafio; consulta atrasada; cancelamento/stop e feature desligada. Executar também Auth/Firestore Emulator reais com o helper Rust real, sem tratar mocks como prova dessa integração.

88B deve acrescentar uma política de hospedagem revisada, fonte nativa, autorização individual atual e troca interna da autorização de transporte; somente então habilitar Entrar para o recorte testado. Aprovação de pedidos, participantes dinâmicos, revogação ativa, seleção de caminho, voz e homologação externa permanecem etapas distintas. Este primeiro vínculo não fecha R4-005/008.

Rollback: reverter somente este incremento; interromper os próprios helpers e serviço local. Nenhuma regra remota, configuração pessoal, firewall, conta de produção, release ou instalador foi alterado. Documentar comandos e resultados efetivos no relatório de execução antes de promover o estado.
