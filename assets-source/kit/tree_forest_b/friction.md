# tree_forest_b friction

- 2026-10-14: the approved original's generator and its `_*.py` helpers are hashed by its provenance, so a variant cannot import a monolithic generator or edit its helper. The variant pipeline lives in a new family helper (`_spruce.py`, `_rock.py`, `_flowers.py`, `_single.py`) that reuses the frozen helpers' functions and copies only the monolithic steps. 5 min. Folded into project.md (Props, variants).
- 2026-10-14: a `json.dump` of assets/kit/manifest.json reflowed the hand-formatted file (2,000 changed lines); the entries went in as text instead. 3 min. Folded into pitfalls.md (Coordinate frame and export).
