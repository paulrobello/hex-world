"""Review sheet and clips for the farm animal motion frames `e2e/farm-motion-shots.mjs` writes here:
each sequence's frames cropped to the animal (the middle of the frame) and tiled in order, one row per
sequence, into sheet.jpg, and each sequence as a looping webm at 5 fps (ImageMagick and ffmpeg).
Usage: python3 docs/checkpoints/farm-motion/sheet.py (from the repo root)."""
import pathlib
import subprocess

D = pathlib.Path("docs/checkpoints/farm-motion")
CROP = "640x480+320+160"
rows = []
for seq in ("cow-walk", "sheep-graze", "cow-b-walk"):
    frames = sorted(D.glob(f"{seq}-[0-9].jpg"), key=lambda p: int(p.stem.rsplit("-", 1)[1]))
    if not frames:
        continue
    row = D / f"row-{seq}.jpg"
    subprocess.run(["magick", *map(str, frames), "-crop", CROP, "+repage", "-resize", "320x240",
                    "-bordercolor", "white", "-border", "2", "+append", str(row)], check=True)
    rows.append(row)
    subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-framerate", "5", "-start_number", "0",
                    "-i", str(D / f"{seq}-%d.jpg"), "-vf", "crop=640:480:320:160",
                    "-c:v", "libvpx-vp9", "-b:v", "0", "-crf", "34", str(D / f"{seq}.webm")], check=True)
subprocess.run(["magick", *map(str, rows), "-background", "white", "-append", "-quality", "85",
                str(D / "sheet.jpg")], check=True)
for row in rows:
    row.unlink()
print(f"wrote {D / 'sheet.jpg'}")
