# Correção focal de dependências do desktop durante Connect46

Data08/10/2026, base `791b77b`, branch `codex/aur-r4-connect`. Estado **CONCLUÍDO** para as duas correções locais descritas; não significa homologação de Connect ou eliminação de todos os avisos. O trabalho Connect em andamento permaneceu no checkout e não integra este commit de dependências.

`npm ci --prefix apps/desktop` e auditoria somente leitura identificaram seis pacotes sinalizados: DOMPurify, source-map-js e quatro entradas do mesmo grafo Firebase/gRPC. A inspeção de ranges e fontes primárias selecionou apenas duas atualizações compatíveis:

| Pacote | Lock anterior → novo | Origem e alcance observado |
| --- | --- | --- |
| DOMPurify | 3.4.14 → 3.4.16 | Range direto `^3.4.14`; [release upstream](https://github.com/cure53/DOMPurify/releases/tag/3.4.16). Sanitização de descrições no desktop usa string/ADD_ATTR, sem os modos IN_PLACE/hooks relacionados aos dois avisos; isso não prova ausência geral de risco. |
| source-map-js | 1.2.1 → 1.2.2 | PostCSS8.5.26 admite `^1.2.1`; [release upstream](https://github.com/7rulnik/source-map-js/releases/tag/v1.2.2). Dependência de desenvolvimento na árvore Vite/PostCSS. |

Mudança exata: somente `version`, `resolved` e `integrity` dessas duas entradas em `apps/desktop/package-lock.json`; manifest e demais pacotes preservados. Os valores foram consultados no registro npm e aplicados por patch, sem `npm update`, `audit fix`, override, downgrade de Firebase ou autorização automática de scripts novos.

Comandos efetivamente executados:

```powershell
npm view dompurify@3.4.16 version dist.integrity dist.tarball dependencies optionalDependencies license --json
npm view source-map-js@1.2.2 version dist.integrity dist.tarball dependencies engines license --json
npm ci --prefix apps/desktop
node --test --test-concurrency=1 apps/desktop/src/lib/projectTranslation.test.mjs apps/desktop/src/features/connect/ConnectView.smoke.mjs
npm run build --prefix apps/desktop
npm audit --prefix apps/desktop --json
npm ls --prefix apps/desktop dompurify source-map-js @grpc/grpc-js firebase @firebase/firestore postcss --all
git diff --check
```

Instalação282pacotes e build TypeScript/Vite355módulos passaram; permanece o aviso existente de chunk maior que500kB. Rodada de testes **21PASS/0skip**,3958,667ms:12 testes existentes de tradução/links e9 do painel Connect ainda em implementação46. Esses nove são fixtures DOM e não prova de transporte ou Tauri; os12 não constituem teste browser específico do sanitizador. Asserção estrutural Node comparou o lock contra `git show HEAD:apps/desktop/package-lock.json`, exigiu somente os três campos em duas entradas e igualdade do restante; manifest sem diff. Nenhum teste Java, novo mod ou matriz foi executado por essa correção.

Auditoria posterior retornou exit1, com **quatro pacotes high** e nenhum low/critical no resumo por pacote, todos referentes ao mesmo grafo `firebase → firestore/firestore-compat → @grpc/grpc-js1.9.16`. Não são quatro falhas independentes. Os dois avisos fonte são [validação de certificado no servidor gRPC](https://github.com/grpc/grpc-node/security/advisories/GHSA-m9gg-hp2v-232j) e [exposição de mensagem de erro de handler](https://github.com/grpc/grpc-node/security/advisories/GHSA-f596-whhp-79r4), com correção1.13.6 fora de `~1.9.0` exigido pelo Firestore fixado. O uso atual é cliente browser/WebChannel, não servidor Node gRPC; exports e bundle foram inspecionados, mas busca de strings não comprova inacessibilidade absoluta. Não suprimir esses avisos nem aplicar o downgrade Firebase9.14 sugerido automaticamente; qualquer override exige decisão e testes próprios.

Evidências locais ignoradas: `.aurora/connect-validation/connect-desktop-security-lock-{ci,tests,build,diff}.log` e `connect-desktop-security-lock-npm-audit.json`. Auditoria anterior preservada em `connect-desktop-forty-six-npm-audit.json`. Sem credenciais, API paga, push, publicação ou instalador. Nenhuma configuração pessoal foi alterada. Rollback: revert local revisado deste commit e reinstalação pelo lock correspondente; não apagar checkout, caches ou dados do usuário. Nenhuma ação humana é necessária.
