"""Which hero oak the oak tile's slot 0 draws at each cell near the origin: a port of
`slot_hash` / `pick_piece` in crates/hex-core/src/variant.rs (PerSlot, VARIANT_ONE_IN 3), with the
slot's row (tree_hero_oak -> [tree_hero_oak_b, tree_hero_oak_c]). Picks the shot cells for run.sh.
Usage: python3 docs/checkpoints/tree_hero_oak_c/cells.py"""
M = (1 << 64) - 1
VARIANTS = ["tree_hero_oak_b", "tree_hero_oak_c"]


def slot_hash(q: int, r: int, slot: int) -> int:
    packed = ((q & 0xFFFFFFFF) << 32) | (r & 0xFFFFFFFF)
    z = packed ^ (((slot + 0x5EED4E50) & M) * 0x9E3779B97F4A7C15 & M)
    z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & M
    z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & M
    return z ^ (z >> 31)


def pick(q: int, r: int) -> str:
    h = slot_hash(q, r, 0)
    return VARIANTS[(h // 3) % len(VARIANTS)] if h % 3 == 0 else "tree_hero_oak"


for q in range(-3, 4):
    print(" ".join(f"({q:+d},{r:+d}) {pick(q, r)[13:] or 'hero':>4}" for r in range(-3, 4)))
