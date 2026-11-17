# ENH-006: Checkpoint evidence images out of plain git

| Field | Value |
|---|---|
| **Source** | Audit 2026-11-15, architecture finding ARC-007 (second step) |
| **Impact** | Low now, growing: clone, worktree and CI checkout cost rises with every capture run |
| **Effort** | S for the switch, L only if history is ever rewritten (not proposed) |
| **Depends on** | ARC-007 (sparse checkout), SEC-006 (perf evidence cleanup) |
| **Owner decision needed** | Yes: which option, and the hosting quota |

## Goal

New review evidence stops growing the git object store that every clone, worktree and CI job pays for, while owner review pages and the approval workflow keep working from a normal checkout.

## Current state (at `edbc7ad`)

- Tracked tree about 1.15 GiB; pack 635 MiB after about five weeks of history. `docs/` is 558 MiB, of which `docs/checkpoints/` is 469 MiB.
- `docs/checkpoints/` by tracked file type: 2,126 `.jpg`, 659 `.json`, 228 `.py`, 77 `.sh`, 44 `.png`, 44 `.patch`, 28 `.mp4`, 26 `.md`.
- Committing checkpoint evidence is a documented convention (`CLAUDE.md`: "Checkpoint evidence (owner review images) goes to `docs/checkpoints/<feature>/` via the `make shots-*` targets"). The owner reviews frames from the working tree (`docs/review/approvals.py` generates `approvals.html`, served on 8095).
- Every re-shot JPEG is a new blob kept forever. There is no `.gitattributes`.
- Git LFS 3.8.1 is installed on the dev host.
- The Pages artifact is built from `dist/` only, so `docs/` never ships.

## Options

| | Option | What changes | Cost |
|---|---|---|---|
| O1 | **Git LFS for evidence media, going forward** (recommended) | `.gitattributes` tracks `docs/checkpoints/**` and `docs/perf/**` `.jpg`, `.png`, `.mp4`. Existing files convert to pointers in one new commit. | LFS storage and bandwidth quota on the hosting account; fresh clones need `git lfs` installed |
| O2 | External evidence store with a hash manifest | Images go to a store outside the repo (a release asset or an artifact host); git keeps `<feature>/frames.json` with SHA-256 and URL | A fetch step before review; review pages need the store reachable |
| O3 | Status quo with pruning rules | Keep images in git; delete superseded checkpoint folders on a schedule | No tooling change; history still holds every blob, so nothing shrinks |

Recommendation: O1. It keeps the working tree identical for the owner and for every script that reads frames, needs no new service, and is reversible. It does not shrink existing history (old blobs stay reachable); it stops the growth and lets CI and shallow clones skip the media.

## Steps (O1)

1. **Decide and check quota.** Confirm the option with the owner. Check the repository host's current LFS storage and bandwidth allowance against about 470 MiB of existing media plus growth; record the numbers on the card. Stop here if the quota does not fit.
2. **Inventory readers.** `git grep -nE 'docs/(checkpoints|perf)/.*\.(jpg|png|mp4)' -- scripts e2e crates web docs/review Makefile` and read each hit. Anything that reads these files at build or test time (`include_bytes!`, a unit test fixture) must keep working when the file is an LFS pointer in CI. Move such fixtures out of the tracked patterns (for example to `scripts/assets/test-fixtures/`) before step 3.
3. **Track.** Add `.gitattributes`:
   ```
   docs/checkpoints/**/*.jpg filter=lfs diff=lfs merge=lfs -text
   docs/checkpoints/**/*.png filter=lfs diff=lfs merge=lfs -text
   docs/checkpoints/**/*.mp4 filter=lfs diff=lfs merge=lfs -text
   docs/perf/**/*.jpg        filter=lfs diff=lfs merge=lfs -text
   docs/perf/**/*.png        filter=lfs diff=lfs merge=lfs -text
   ```
   Do not track `assets/` (the game loads those files; Trunk copies them into `dist/`) or `assets-source/refs/` (approval references read by the asset pipeline).
4. **Convert current files without rewriting history.** `git lfs migrate import --no-rewrite --include="docs/checkpoints/**/*.jpg,docs/checkpoints/**/*.png,docs/checkpoints/**/*.mp4,docs/perf/**/*.jpg,docs/perf/**/*.png"` creates one new commit that replaces the files with pointers. Verify `git lfs ls-files | wc -l` equals the count from `git ls-files` for those patterns before the commit.
5. **CI.** `actions/checkout` does not fetch LFS objects unless `lfs: true`, so every job gets pointers and the checkout shrinks with no workflow change. Confirm no job reads the media (step 2), and that ARC-007's sparse checkout still applies.
6. **Worktrees and agents.** `scripts/worktree-bootstrap.sh` must not run git (agent guards refuse it): confirm a new worktree checks out the media through the LFS smudge filter from the shared `.git/lfs` store with no network fetch. Add `git lfs install --local` to the setup notes in `README.md` and `CONTRIBUTING.md`. Asset agents commit frames as before; the filter handles them.
7. **Push.** The first push uploads about 470 MiB to LFS. This is outward-facing and metered: get the owner's go-ahead for that push specifically, then watch the CI run.
8. **Docs.** `CLAUDE.md` (checkpoint evidence convention: media is LFS-tracked), `docs/DOCUMENTATION_STYLE_GUIDE.md` if it names evidence handling, `PRD.md` §14 with a dated decision line.

## Files to touch

`.gitattributes` (new), `README.md`, `CONTRIBUTING.md`, `CLAUDE.md`, `PRD.md`, possibly test fixtures that move out of the tracked patterns, and one conversion commit touching about 2,200 media files under `docs/checkpoints/` plus those under `docs/perf/`.

## Verify

- `git lfs ls-files | wc -l` equals the number of tracked media files under the five patterns; `git lfs fsck` is clean.
- `git check-attr filter -- docs/checkpoints/open_tunnels/<a frame>.jpg` reports `lfs`; a file under `assets/` reports unspecified.
- `python3 docs/review/approvals.py` still generates the page and the frames render from `python3 scripts/dev/serve.py 8095 .`.
- A scratch clone with `GIT_LFS_SKIP_SMUDGE=1` passes `make checkall` (proves no gate reads the media), and its `du -sh` is recorded against a full clone.
- A new worktree shows real images, not pointer text (`file docs/checkpoints/<feature>/<frame>.jpg` says JPEG).
- The pushed CI run is green and its checkout steps are faster than the previous run's.

## Rollback

`git lfs migrate export --no-rewrite --include="<same patterns>"` creates a commit turning pointers back into plain files, then remove `.gitattributes`. LFS objects already uploaded stay on the host until pruned there.

## Out of scope

Rewriting history to shrink the existing pack (it would change every commit hash that `handoff.md`, `PRD.md` and the board cite), and moving `assets/` or `assets-source/`.
