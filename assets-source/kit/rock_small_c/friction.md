# rock_small_c friction

- 2026-10-14: the approved original's generator and its `_*.py` helpers are hashed by its provenance, so a variant cannot import a monolithic generator or edit its helper. The variant pipeline lives in a new family helper (`_spruce.py`, `_rock.py`, `_flowers.py`, `_single.py`) that reuses the frozen helpers' functions and copies only the monolithic steps. 5 min. Folded into project.md (Props, variants).
- 2026-10-14: a `json.dump` of assets/kit/manifest.json reflowed the hand-formatted file (2,000 changed lines); the entries went in as text instead. 3 min. Folded into pitfalls.md (Coordinate frame and export).
- 2026-10-14: four stones carry about 1.3x rock_small's surface; at a 512² albedo the median density was 243 px/m (gate 256). Shipped a 1024² albedo with normal and ORM box-filtered to 512² (as the boulder), 1.19 MB BC7. 1 min. Folded into project.md (Props): a larger or multi-stone variant takes a 1024² albedo and 512² normal/ORM.
