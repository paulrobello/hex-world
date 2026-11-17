# proptest regressions

A failing property persists its shrunk case so it replays on every later run: by default proptest
writes `tests/<target>.proptest-regressions` next to the test (`tests/save_props.proptest-regressions`
for the decoder's properties), not here. Commit whatever file appears: a non-empty regressions file
means a found case must be fixed in the decoder or the operation (never by relaxing the test), and
the shrunk case then becomes an example test beside the existing ones in `save.rs`. This directory
exists so the policy has a home in version control and the default location is documented.
