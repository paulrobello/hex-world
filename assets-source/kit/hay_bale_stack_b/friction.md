# hay_bale_stack_b friction (2026-10-20, built with hay_bale_round_b from kit/_hay_var.py)

1. **A family-file edit after the build stales the sibling piece.** Both variants import
   `_hay_var.py`, so the round_b fixes after stack_b's first build made `make kit-provenance
   PIECE=hay_bale_stack_b` refuse ("_hay_var.py changed since the build"). Fix: rebuild and
   recompress stack_b (its geometry was unchanged). 6 min. Proposed SKILL.md text, appended to the
   "Helpers" rule:
   > When two new pieces share one family file, finish the family file before either piece's
   > provenance, or rebuild the first after the last edit: provenance refuses a helper changed since
   > the build.
2. **A dark spot in the textured still was the contact groove, not a hole.** Between the lower pair,
   seen past the top bale's bevel, faces bake AO 0.03, as on `hay_bale_stack` (AO p01 0.09). Checked
   on the reimported LODs (no faces missing). 3 min. No skill change.
