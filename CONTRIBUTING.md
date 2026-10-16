# Contributing

See [README.md](README.md) for prerequisites and the development server. [PRD.md](PRD.md) records product scope and phase checklists. Keep changes focused on one issue.

## Work in an isolated checkout

Use a Git worktree rather than editing the main checkout:

```sh
git worktree add -b my-change ../hex-world-my-change main
cd ../hex-world-my-change
```

Gitignored dependencies, local configuration and build artifacts do not follow the worktree. Restore any required local configuration without committing secrets. The UI targets install their npm dependencies automatically. Browser tests have a separate `e2e/package-lock.json`: install with `npm ci --prefix e2e` and `npx --prefix e2e playwright install chromium` before running them.

## Change both sides of the bridge

The HUD communicates with Rust through the typed `window.hexWorld` bridge. After changing Rust bridge command/event enums, run `make ui-schema`, update the matching TypeScript contract/handling in `web/ui/src/bridge.ts`, and include regenerated `web/ui/src/bridge-schema.json` and `web/ui/test/rules-parity.json` in the change. Rust contract tests reject stale generated files. Do not hand-edit the generated JSON.

## Verify and capture evidence

- Run `make checkall` while iterating: formatting, Clippy, native and WASM type checks, workspace tests and UI checks.
- Run `make checkall-ci` before pushing: the fast gate plus a release web build and download-size budgets.
- For visual changes, use the relevant `make shots-*` or `make web-shots-*` target and include owner-review evidence under `docs/checkpoints/<feature>/`. Targets and driving interfaces are documented in [docs/agent-operability.md](docs/agent-operability.md). Native capture needs an awake display. Browser/e2e tests run locally on GPU Chromium, not in CI.
- Asset changes also need the gates in [.claude/skills/blender-asset/SKILL.md](.claude/skills/blender-asset/SKILL.md). Do not commit `.blend` files or accidentally regenerated thumbnails.

## Commit and review

Use small, descriptive commits. Existing subjects use `docs: …`, `fix(bridge): …`, `test(water): …`, `perf(grass): …` and `ci: …`. Describe what changed rather than just naming a file. Include the commands run and any limitations in the pull request. Do not push without repository-owner approval. Rebase onto the current target before integrating, then squash each unit of work into one commit. Every merge to `main` is followed by `make build-web` to refresh the local play build.
