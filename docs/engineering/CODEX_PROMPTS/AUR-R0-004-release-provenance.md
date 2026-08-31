# Prompt AUR-R0-004

Você é o Release/Docs Implementer do AuroraLauncher. Execute somente **AUR-R0-004 — Proveniência, versão e documentação pública**, baseline `5aa5fe8`, branch `codex/aur-r0-004-release-provenance`, worktree dedicado. Leia `AGENTS.md`, manifests, README, `docs/module-a.md`, release docs e a task correspondente.

Fato histórico que deve ser preservado: `releases/Aurora Smart Launcher_0.1.0_x64-setup.exe` tem 5.029.160 bytes e SHA-256 `95C2708E5898A0E194263D5E9865F0CC6E56D8880E757C209BD4A50A69D9AAF8`; o asset GitHub sob `v0.1.0-alpha` tem 5.028.702 bytes e SHA-256 `582E5BA290620CEB63B8A024C7BAA2CB44487542EF4E1CE15C0D4E1A2FD465A9`. Não decida retroativamente qual “deveria” ser o arquivo, não apague/substitua asset, tag ou binário. Próximo candidato recomendado: `0.2.0-alpha.1`, mas só faça bump se a release tiver sido autorizada; o checker pode ser implementado antes.

Ownership: `scripts/release/**`, fontes de versão/manifests, README fora da seção legal, `docs/module-a.md` e docs/changelog de release. Não toque LICENSE/notices, lógica funcional, regras Firebase ou artefatos históricos. Root `package.json` pertence a você nesta wave; coordene aliases de CI via Integration Agent.

Implemente checker read-only de consistência SemVer entre root/package, desktop/package, Cargo/Tauri e Companion; detecção de tag/versão já usada; manifesto JSON e `SHA256SUMS` reproduzíveis com path, size, SHA-256, plataforma e versão. Fixture com versão/hash divergente deve falhar; caminhos com espaço devem funcionar.

Reconcile README/module-a/status com fatos: alpha é de desenvolvimento privado, Swing ainda é janela externa, só 2 runtimes têm evidência, Microsoft entitlement/updater/multiplayer ainda não estão prontos e o produto não é oficial Minecraft/Mojang. Documente release imutável, SBOM, artifact attestation, assinatura Tauri, publisher Windows não assinado e SignPath como candidatura sem garantia.

Execute checker/testes, build/parsing afetados e `git diff --check`. Faça um único commit `chore(release): enforce immutable provenance [AUR-R0-004]`. Handoff: ID, branch/SHA, arquivos, hashes confirmados, comandos, docs alteradas, decisões D-002/assinatura pendentes, riscos e rollback. Não faça push, merge, tag nem publique release.
