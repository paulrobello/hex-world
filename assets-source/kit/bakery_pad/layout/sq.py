"""Square-kerb mouth layout: the through road's north kerb/verge/sink is cut between x = a and b
(end caps), a flat forecourt runs north from the asphalt edge. Obstacles in runtime (x, z): shop,
kerb strips outside [a, b], south kerb, forecourt east/north edges, hex border."""
import json
import sys

sys.path.insert(0, __import__("os").path.dirname(__import__("os").path.abspath(__file__)))
import check  # noqa: E402


def build(c):
    a, b = c["a"], c["b"]
    extra = {
        "kerb w": [(-14.0, -4.6), (a, -4.6), (a, -3.0), (-14.0, -3.0)],
        "kerb e": [(b, -4.6), (14.0, -4.6), (14.0, -3.0), (b, -3.0)],
        "south kerb": [(-14.0, 3.0), (14.0, 3.0), (14.0, 5.0), (-14.0, 5.0)],
        "fore e": [(c["xe"], -16.0), (16.0, -16.0), (16.0, -4.6), (c["xe"], -4.6)],
        "fore n": [(-16.0, -18.0), (16.0, -18.0), (16.0, c["zn"]), (-16.0, c["zn"])],
    }
    for k, v in c.get("extra", {}).items():
        extra[k] = [tuple(p) for p in v]
    return {"shop": c["shop"], "xs": c["xs"], "zb": c["zb"], "extra": extra, "spots": c["spots"]}


if __name__ == "__main__":
    c = json.load(open(sys.argv[1]))
    w, _ = check.run(build(c))
    print("WORST", round(w, 3))
