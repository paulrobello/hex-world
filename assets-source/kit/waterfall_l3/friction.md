# waterfall fall faces: friction (Task 18)

- `export.write_export_report` listed only `socket-lane-`, `anchor-prop-` and `anchor-park-` sockets; it now lists `socket-lip-` too, and writes array extras (an IDProperty array such as a lip's `normal`) as lists instead of failing in `json.dumps`.
- `mathutils.geometry.delaunay_2d_cdt` returns single-precision coordinates: a perimeter matched back to its inputs by coordinate misses vertices at 1e-7. The generator maps each output vertex to its input through the CDT's `orig` lists instead.
- The controller's tier contract (WF-Q3) arrived mid-build: lips at the upper water level less 4 m per tier, set out 1.5 m per tier. The first layout (lip 0 on the channel bed, ledges about 1.35 m out on a dished profile) was rebuilt to it, and the gate's reach limit became 1.5 (drop - 1) + 1 m.
- A 14 m face 4 to 15 m tall cannot reach the kit's 256 px/m on a 1024 atlas at the piece KTX budget: the fall faces carry scoped floors (kit_gate `WATERFALL_TEXEL_MIN_PX_PER_M`), an owner question.
