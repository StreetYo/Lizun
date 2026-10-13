"""Builds the landing page media (site/media) from the transparent demo recordings.

The recordings come from the local demo tool (scripts/demo, not in the repository):
out/reel-alpha and out/widgets-alpha, and the same with -en for the English page (recorded with\nLIZUN_LANG=en) into site/media/en. The island is laid over a
generated wallpaper and encoded as small H.264 MP4s, because iOS Safari cannot play video with
an alpha channel from WebM. Needs ffmpeg and Pillow:

    python scripts/site-media.py [scripts/demo/out] [site/media]
"""
import json
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

REPO = Path(__file__).resolve().parent.parent
W, H = 2000, 1200  # size of the alpha recordings


def wallpaper(path: Path, w=W, h=H):
    """Dark blue wallpaper with soft colour blobs, in the colours of the app icon."""
    img = Image.new("RGB", (w, h), (9, 16, 38))
    d = ImageDraw.Draw(img)
    for cx, cy, r, col in [
        (0.15, 0.95, 0.55, (10, 124, 255)),
        (0.85, 0.85, 0.50, (92, 194, 255)),
        (0.55, 1.15, 0.45, (120, 92, 255)),
        (0.95, 0.10, 0.30, (40, 70, 160)),
        (0.05, 0.15, 0.25, (30, 60, 140)),
    ]:
        rr = r * w
        d.ellipse([cx * w - rr, cy * h - rr * 0.7, cx * w + rr, cy * h + rr * 0.7], fill=col)
    img = img.filter(ImageFilter.GaussianBlur(w * 0.12))
    # Darken the top so the black island still reads as an island, not a hole.
    shade = Image.linear_gradient("L").resize((w, h)).point(lambda v: 90 + v * 165 // 255)
    img = Image.composite(img, Image.new("RGB", (w, h), (6, 10, 24)), shade)
    img.save(path)


def ff(*args):
    subprocess.run(["ffmpeg", "-v", "error", "-y", *args], check=True)


X264 = ["-c:v", "libx264", "-preset", "slow", "-pix_fmt", "yuv420p", "-profile:v", "high",
        "-movflags", "+faststart", "-an"]


def clip(src, wall, out, start, end, crop, width, crf=23):
    x, y, cw, ch = crop
    flt = (f"[0][1]overlay=shortest=1,crop={cw}:{ch}:{x}:{y},"
           f"scale={width}:-2:flags=lanczos,fps=30")
    ff("-loop", "1", "-i", str(wall), "-ss", str(start), "-to", str(end), "-i", str(src),
       "-filter_complex", flt, *X264, "-crf", str(crf), str(out))


def poster(video, out, at):
    ff("-ss", str(at), "-i", str(video), "-frames:v", "1", "-q:v", "3", str(out))


def og_image(wall, frame_png, out, title, subtitle):
    """1200x630 link preview: the open island on the wallpaper with the slogan."""
    base = Image.open(wall).resize((1200, 720)).crop((0, 0, 1200, 630))
    island = Image.open(frame_png).convert("RGBA").crop((330, 0, 1670, 480))
    island = island.resize((int(island.width * 0.66), int(island.height * 0.66)), Image.LANCZOS)
    base.paste(island, ((1200 - island.width) // 2, 0), island)
    d = ImageDraw.Draw(base)
    bold = ImageFont.truetype("C:/Windows/Fonts/segoeuib.ttf", 64)
    reg = ImageFont.truetype("C:/Windows/Fonts/segoeui.ttf", 32)
    d.text((600, 440), title, font=bold, fill="white", anchor="mm")
    d.text((600, 520), subtitle, font=reg, fill=(200, 215, 240), anchor="mm")
    base.save(out, quality=88)


def alpha_mov(folder: Path) -> Path:
    """island_alpha.mov of a reel.mjs recording with LIZUN_ALPHA=1, made from its PNG frames
    the first time: each frame lasts until the next one, as in widgets.mjs."""
    mov = folder / "island_alpha.mov"
    if mov.exists():
        return mov
    timeline = json.loads((folder / "timeline.json").read_text(encoding="utf-8"))
    frames = timeline["frames"]
    lines = []
    for i, frame in enumerate(frames):
        nxt = frames[i + 1]["t"] if i + 1 < len(frames) else timeline["end"]
        lines += [f"file '{frame['file']}'", f"duration {max(0.001, nxt - frame['t']):.4f}"]
    lines.append(f"file '{frames[-1]['file']}'")
    (folder / "frames.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "concat", "-safe", "0", "-i", "frames.txt",
                    "-vf", "fps=30", "-c:v", "prores_ks", "-profile:v", "4444",
                    "-pix_fmt", "yuva444p10le", mov.name], cwd=folder, check=True)
    return mov


def build(alpha: Path, widgets: Path, wall: Path, dest: Path):
    dest.mkdir(parents=True, exist_ok=True)

    # Hero: the whole 28 s recording, zoomed in on the island so it reads on a phone.
    clip(alpha, wall, dest / "hero.mp4", 0, 28, (300, 0, 1400, 840), 1000, crf=25)
    poster(dest / "hero.mp4", dest / "hero.jpg", 5)

    # One clip per feature, cut from the same recording (times from a contact sheet of it).
    feature_crop = (330, 0, 1340, 670)
    for name, start, end, at in [
        ("music", 2.2, 9, 3),
        ("tray", 9, 15.6, 3),
        ("clipboard", 15.5, 19.4, 1.5),
        ("tasks", 19.3, 24.5, 3),
    ]:
        clip(alpha, wall, dest / f"{name}.mp4", start, end, feature_crop, 800)
        poster(dest / f"{name}.mp4", dest / f"{name}.jpg", at)

    # Widgets: every recording has the island centred at the top, at most 736 wide.
    widget_crop = (560, 0, 880, 520)
    for mov in sorted(widgets.glob("*.mov")):
        out = dest / f"w-{mov.stem}.mp4"
        clip(mov, wall, out, 0, 99, widget_crop, 640)
        poster(out, dest / f"w-{mov.stem}.jpg", 2.5)


def main(src: Path, dest: Path):
    tmp = src / "site"
    tmp.mkdir(exist_ok=True)
    wall = tmp / "wallpaper.png"
    wallpaper(wall)

    # Russian island for the main page, English (LIZUN_LANG=en recordings) for /en/.
    for lang, sub, suffix, title, subtitle in [
        ("ru", "", "", "Dynamic Island для Windows",
         "Бесплатно. Музыка, файлы, буфер обмена, задачи и виджеты"),
        ("en", "en", "-en", "Dynamic Island for Windows",
         "Free. Music, files, clipboard, tasks and widgets"),
    ]:
        alpha = alpha_mov(src / f"reel-alpha{suffix}")
        build(alpha, src / f"widgets-alpha{suffix}", wall, dest / sub)
        # Link preview for messengers and social networks.
        frame = tmp / f"open-{lang}.png"
        ff("-ss", "5", "-i", str(alpha), "-frames:v", "1", str(frame))
        og_image(wall, frame, dest / f"og-{lang}.jpg", title, subtitle)


if __name__ == "__main__":
    args = sys.argv[1:]
    main(Path(args[0]) if args else REPO / "scripts" / "demo" / "out",
         Path(args[1]) if len(args) > 1 else REPO / "site" / "media")