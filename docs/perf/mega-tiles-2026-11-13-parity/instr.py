"""Applies (or with --revert restores) log-only instrumentation to a hex-world tree: each script
command, held-capture pause, readback store and resume logs `ab:` with Bevy's FrameCount and the
virtual elapsed time. Backups go beside the files as *.ab-orig. usage: instr.py <tree> [--revert]"""
import pathlib
import shutil
import sys

tree = pathlib.Path(sys.argv[1])
files = [tree / "crates/hex-app/src/capture.rs", tree / "crates/hex-app/src/control.rs"]
if "--revert" in sys.argv:
    for f in files:
        b = f.with_suffix(f.suffix + ".ab-orig")
        if b.exists():
            shutil.move(b, f)
    print("reverted")
    sys.exit(0)

edits = {
    files[0]: [
        (
            "move |shot: On<ScreenshotCaptured>, mut commands: Commands| {",
            "move |shot: On<ScreenshotCaptured>, mut commands: Commands, fc: Res<bevy::diagnostic::FrameCount>| {\n"
            "            info!(\"ab: readback {name} frame {}\", fc.0);",
        ),
        (
            "    mut paused_at: Local<Option<(std::time::Instant, u32)>>,\n) {",
            "    mut paused_at: Local<Option<(std::time::Instant, u32)>>,\n"
            "    fc: Res<bevy::diagnostic::FrameCount>,\n) {",
        ),
        (
            "            time.pause();\n",
            "            time.pause();\n"
            "            info!(\"ab: hold pause frame {} vt {:.4}\", fc.0, time.elapsed_secs_f64());\n",
        ),
        (
            "            time.unpause();\n",
            "            time.unpause();\n"
            "            info!(\"ab: hold resume frame {} after {} frames vt {:.4}\", fc.0, frames, time.elapsed_secs_f64());\n",
        ),
    ],
    files[1]: [
        (
            "    time: Res<'w, Time<bevy::time::Virtual>>,\n}",
            "    time: Res<'w, Time<bevy::time::Virtual>>,\n    fc: Res<'w, bevy::diagnostic::FrameCount>,\n}",
        ),
        ("        scene,\n        time,\n    } = scene;", "        scene,\n        time,\n        fc,\n    } = scene;"),
        (
            '    info!("command: {cmd:?}");',
            '    info!("ab: command frame {} vt {:.4} {cmd:?}", fc.0, time.elapsed_secs_f64());',
        ),
    ],
}
for f, reps in edits.items():
    src = f.read_text()
    for old, new in reps:
        if src.count(old) != 1:
            sys.exit(f"{f}: expected one match of {old!r}, found {src.count(old)}")
        src = src.replace(old, new)
    shutil.copy(f, f.with_suffix(f.suffix + ".ab-orig"))
    f.write_text(src)
print("instrumented")
