# Owner review queue, 2026-11-16

Requests waiting on you, filed by the /dbl loop. Nothing here is approved unless it says answered. Board cards carry the same questions with the `approval` tag. Wave-1 agents are running tonight on branches off `main` `740d8c8`; their verified results land per the standing push rule.

## EQ1. ENH-001 Phase 0: replace the `integration.patch` convention (card `01a1dfe2bc0a`, blocked)

ENH-001 moves every per-piece runtime table (`walk/tables.rs`, `scatter/tables.rs`, `HERO_VARIANTS`, the showcase dispositions) into `data/pieces.ron`, embedded like the tile definitions, so adding a kit piece is one data entry instead of Rust edits in three crates. Plan: `docs/fable/ENH-001-data-driven-piece-metadata.md`. The change you are asked to approve: **asset agents return a `data/pieces.ron` entry (plus any non-table Rust change) instead of a verified `integration.patch` over the tables** — the convention recorded in `CLAUDE.md` and the blender-asset dispatch brief. Phases 1-4 stay shippable per phase behind green transitional equality tests and byte-identical `make thumbs`. Reply "approved" (or with changes) to unblock the L-sized card.

## EQ2. ENH-006: Git LFS for checkpoint evidence, option O1 (card `01a1dfe2cdcd`, blocked)

Plan: `docs/fable/ENH-006-checkpoint-evidence-storage.md`. `docs/checkpoints/` is 469 MiB of the 635 MiB pack and every re-shot JPEG grows it forever. Recommendation O1: `.gitattributes` LFS-tracks `docs/checkpoints/**` + `docs/perf/**` jpg/png/mp4 going forward (no history rewrite). Needs from you: (a) the option choice, (b) a check of the GitHub LFS storage/bandwidth quota against ~470 MiB existing media, (c) a specific go-ahead for the first push, which uploads that ~470 MiB and is metered. Work stops before the push until you answer; a scratch-clone `make checkall` with `GIT_LFS_SKIP_SMUDGE=1` is part of the verification.

## EQ3. rail_station_shelter measured footprint (card `01a1e39c6f92`, blocked)

QA-001 gave the shelter a measured `ground: 4.8` footprint row; it legitimately moves the wildflower scatter around it (10-28 px per rotation, all wildflower blades) and re-renders the six `rail_station` thumbnails. The fix-audit run reverted the row to keep thumbs byte-identical for merge and left a `DEFAULT_FOOTPRINT_KEPT` exemption. Landing it means: restore the row, delete the exemption, `make thumbs`, commit the regenerated `rail_station-r0..r5` + contact sheet, with in-engine shots for you before the merge per the visual standard. Say "land it" and the prep + booked-window captures happen next session.

## EQ4. ENH-002 one-frame lag acceptance (card `01a1dfe2bf74`, in progress — pending tonight's spike)

The additive spike (engine-published `play.fits`, refusal codes; the `rules.ts` mirror stays authoritative) is being built tonight. The follow-up that deletes the mirror waits on: zero mirror-vs-engine disagreements across the parity worlds plus your acceptance of the one-frame lag by eye. Measurement lands here when the spike reports.
