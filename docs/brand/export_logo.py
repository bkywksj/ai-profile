"""ai-profile Logo 导出脚本（V02 · 全白三段）。

几何参数与 logo-iterate-v2.html 的 V02 完全一致；改造型只改 GEO / 配色常量后重跑。
渲染走 Playwright（Chromium 出图与浏览器里看到的一致），ICO 由 Pillow 从各尺寸 PNG 打包。

用法：python docs/brand/export_logo.py   → 产物写到 docs/brand/logo/
"""
import json
import math
import shutil
from pathlib import Path

from PIL import Image
from playwright.sync_api import sync_playwright

OUT = Path(__file__).parent / "logo"
NAME = "ai-profile"

INDIGO = "#4f46e5"      # 主色（= 文档站 --vp-c-brand-1）
INDIGO_DARK = "#6366f1"  # 深色底上的容器色：#4f46e5 放在 #121212 上偏闷，提亮一档
INDIGO_SOFT = "#a5b4fc"  # 无容器款在深底上的线条色
GEO = dict(r=31, sw=9, gap=30, rot=-90, core_r=9, rx=26)


def arcs_svg(color: str) -> str:
    """三段等分弧 + 中心点。按角度等分，缺口 gap 度。"""
    g = GEO
    parts = []
    for i in range(3):
        a0 = math.radians(g["rot"] + i * 120 + g["gap"] / 2)
        a1 = math.radians(g["rot"] + (i + 1) * 120 - g["gap"] / 2)
        x0, y0 = 60 + g["r"] * math.cos(a0), 60 + g["r"] * math.sin(a0)
        x1, y1 = 60 + g["r"] * math.cos(a1), 60 + g["r"] * math.sin(a1)
        parts.append(
            f'<path d="M{x0:.2f} {y0:.2f}A{g["r"]} {g["r"]} 0 0 1 {x1:.2f} {y1:.2f}"/>'
        )
    return (
        f'<g fill="none" stroke="{color}" stroke-width="{g["sw"]}" stroke-linecap="round">'
        + "".join(parts)
        + f'</g><circle cx="60" cy="60" r="{g["core_r"]}" fill="{color}"/>'
    )


def svg(bg: str | None, fg: str, *, full_bleed=False, crop=False) -> str:
    """bg=None 为无容器；full_bleed 为满铺方块（maskable 用）；crop 把无容器款裁到图形外缘。"""
    vb = "22 22 76 76" if crop else "0 0 120 120"
    box = ""
    if bg and full_bleed:
        box = f'<rect width="120" height="120" fill="{bg}"/>'
    elif bg:
        box = f'<rect x="4" y="4" width="112" height="112" rx="{GEO["rx"]}" fill="{bg}"/>'
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vb}">{box}{arcs_svg(fg)}</svg>\n'


VARIANTS = {
    # 文件名后缀: (svg 源, 说明)
    "": (svg(INDIGO, "#fff"), "主标识：靛蓝圆角方块 + 白色三段环（浅色背景默认用这个）"),
    "_dark": (svg(INDIGO_DARK, "#fff"), "深色背景用：容器提亮一档"),
    "_mark": (svg(None, INDIGO, crop=True), "无容器 · 靛蓝线条（浅色背景、行内小图标）"),
    "_mark_dark": (svg(None, INDIGO_SOFT, crop=True), "无容器 · 浅紫线条（深色背景）"),
    "_mono_white": (svg(None, "#fff", crop=True), "单色白（视频水印、深色截图）"),
    "_mono_black": (svg(None, "#000", crop=True), "单色黑（单色印刷、传真件）"),
}


class Renderer:
    def __init__(self, pw):
        # 用本机 Edge，免去 playwright install 下载一份 Chromium
        self.browser = pw.chromium.launch(channel="msedge")

    def shot(self, html: str, w: int, h: int, path: Path, scale=1):
        page = self.browser.new_page(viewport={"width": w, "height": h}, device_scale_factor=scale)
        page.set_content(
            "<!DOCTYPE html><html><head><meta charset='utf-8'><style>"
            "html,body{margin:0;padding:0;background:transparent}"
            "svg{display:block}</style></head><body>" + html + "</body></html>"
        )
        path.parent.mkdir(parents=True, exist_ok=True)
        page.screenshot(path=str(path), omit_background=True)
        page.close()

    def svg_png(self, src: str, size: int, path: Path):
        self.shot(src.replace("<svg ", f'<svg width="{size}" height="{size}" ', 1), size, size, path)

    def lockup(self, mode: str, layout: str, path: Path):
        """横排 / 竖排：图标 + 「ai-profile」字标 + 中文副标题。"""
        bg, fg, sub, icon = {
            "light": ("#ffffff", "#1d1d1f", "#6e6e73", VARIANTS[""][0]),
            "dark": ("#121212", "#f5f5f7", "#a1a1a6", VARIANTS["_dark"][0]),
            "mono": ("#ffffff", "#000000", "#000000", VARIANTS["_mono_black"][0]),
        }[mode]
        font = "'Segoe UI','Inter','Microsoft YaHei',sans-serif"
        if layout == "horizontal":
            w, h, isz = 880, 240, 168
            body = (
                f"<div style='width:{w}px;height:{h}px;background:{bg};display:flex;align-items:center;"
                f"gap:36px;padding:0 48px;box-sizing:border-box;font-family:{font}'>"
                f"<div style='width:{isz}px;height:{isz}px'>{icon.replace('<svg ', f'<svg width={isz} height={isz} ', 1)}</div>"
                f"<div><div style='font-size:84px;font-weight:700;color:{fg};letter-spacing:-2px;line-height:1'>ai-profile</div>"
                f"<div style='font-size:28px;color:{sub};margin-top:14px'>AI 模型服务配置层</div></div></div>"
            )
        else:
            w, h, isz = 520, 520, 220
            body = (
                f"<div style='width:{w}px;height:{h}px;background:{bg};display:flex;flex-direction:column;"
                f"align-items:center;justify-content:center;font-family:{font}'>"
                f"<div style='width:{isz}px;height:{isz}px'>{icon.replace('<svg ', f'<svg width={isz} height={isz} ', 1)}</div>"
                f"<div style='font-size:64px;font-weight:700;color:{fg};letter-spacing:-1.5px;margin-top:30px;line-height:1'>ai-profile</div>"
                f"<div style='font-size:24px;color:{sub};margin-top:14px'>AI 模型服务配置层</div></div>"
            )
        self.shot(body, w, h, path, scale=2)


def main():
    if OUT.exists():
        shutil.rmtree(OUT)
    OUT.mkdir(parents=True)

    for suffix, (src, _) in VARIANTS.items():
        (OUT / f"{NAME}{suffix}.svg").write_text(src, encoding="utf-8")

    sizes = [16, 32, 48, 64, 96, 128, 192, 256, 512, 1024]
    with sync_playwright() as pw:
        r = Renderer(pw)
        light = VARIANTS[""][0]
        for s in sizes:
            r.svg_png(light, s, OUT / "png" / f"{NAME}_{s}x{s}.png")
            r.svg_png(VARIANTS["_dark"][0], s, OUT / "png-dark" / f"{NAME}_dark_{s}x{s}.png")
        for s in [256, 512, 1024]:
            r.svg_png(VARIANTS["_mark"][0], s, OUT / "transparent" / f"{NAME}_transparent_{s}.png")

        fav = OUT / "favicon"
        for s, fn in [(16, "favicon-16x16.png"), (32, "favicon-32x32.png"), (180, "apple-touch-icon.png"),
                      (192, "android-chrome-192x192.png"), (512, "android-chrome-512x512.png")]:
            # apple-touch-icon 由 iOS 自己切圆角，给满铺版，避免圆角里再套一圈白边
            src = svg(INDIGO, "#fff", full_bleed=True) if s == 180 else light
            r.svg_png(src, s, fav / fn)
        # maskable：满铺底色，图形外缘半径 35.5/120≈0.30，落在 0.40 的安全圆内，不用再缩
        r.svg_png(svg(INDIGO, "#fff", full_bleed=True), 512, fav / "maskable-icon-512.png")
        shutil.copy(OUT / f"{NAME}.svg", fav / "favicon.svg")

        for layout in ["horizontal", "vertical"]:
            for mode in ["light", "dark", "mono"]:
                r.lockup(mode, layout, OUT / "lockup" / f"{NAME}_{layout}_{mode}.png")
        r.browser.close()

    # ICO：每个尺寸用各自渲染的 PNG（不从大图缩，16px 才不糊）
    ico_sizes = [16, 32, 48, 64, 128, 256]
    imgs = [Image.open(OUT / "png" / f"{NAME}_{s}x{s}.png").convert("RGBA") for s in ico_sizes]
    imgs[-1].save(OUT / f"{NAME}.ico", format="ICO", sizes=[(s, s) for s in ico_sizes], append_images=imgs[:-1])
    fav_imgs = [i for i, s in zip(imgs, ico_sizes) if s in (16, 32, 48)]
    fav_imgs[-1].save(OUT / "favicon" / "favicon.ico", format="ICO", sizes=[(16, 16), (32, 32), (48, 48)],
                      append_images=fav_imgs[:-1])

    manifest = {
        "name": "ai-profile",
        "short_name": "ai-profile",
        "icons": [
            {"src": "/android-chrome-192x192.png", "sizes": "192x192", "type": "image/png"},
            {"src": "/android-chrome-512x512.png", "sizes": "512x512", "type": "image/png"},
            {"src": "/maskable-icon-512.png", "sizes": "512x512", "type": "image/png", "purpose": "maskable"},
        ],
        "theme_color": INDIGO,
        "background_color": "#ffffff",
        "display": "standalone",
    }
    (OUT / "favicon" / "site.webmanifest").write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n",
                                                      encoding="utf-8")
    print("ok", sum(1 for p in OUT.rglob("*") if p.is_file()), "files")


if __name__ == "__main__":
    main()
