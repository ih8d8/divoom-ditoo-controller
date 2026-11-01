#!/usr/bin/env python3
import os
from collections import deque
import numpy as np
from PIL import Image, ImageDraw, ImageFilter

def render_led_display(img_16x16, cell_size=9, margin=6, title=None, subtitle=None):
    """
    Renders a 16x16 PIL Image into a physical retro Ditoo LED grid card.
    """
    grid_w = 16 * cell_size
    grid_h = 16 * cell_size
    header_h = 24 if title else 0
    footer_h = 24 if subtitle else 0

    total_w = grid_w + 2 * margin
    total_h = grid_h + 2 * margin + header_h + footer_h

    card = Image.new("RGBA", (total_w, total_h), (12, 14, 18, 255))
    draw = ImageDraw.Draw(card)

    # Outer rounded bezel
    draw.rounded_rectangle(
        [1, 1, total_w - 2, total_h - 2],
        radius=7,
        fill=(14, 16, 21, 255),
        outline=(34, 38, 48, 255),
        width=1
    )

    # Optional top header
    if title:
        draw.text((total_w // 2, 12), title, fill=(200, 210, 225, 255), anchor="mm")
        draw.line([margin + 2, header_h, total_w - margin - 2, header_h], fill=(26, 30, 40, 255), width=1)

    # Reentrant bezel around LED matrix
    grid_top = margin + header_h
    draw.rectangle(
        [margin - 2, grid_top - 2, margin + grid_w + 1, grid_top + grid_h + 1],
        fill=(8, 9, 12, 255),
        outline=(22, 25, 32, 255),
        width=1
    )

    rgb_im = img_16x16.convert("RGB")
    for y in range(16):
        for x in range(16):
            r, g, b = rgb_im.getpixel((x, y))
            px = margin + x * cell_size
            py = grid_top + y * cell_size

            if r == 0 and g == 0 and b == 0:
                # Inactive LED cell
                draw.rounded_rectangle(
                    [px + 1, py + 1, px + cell_size - 1, py + cell_size - 1],
                    radius=2,
                    fill=(16, 18, 24, 255),
                    outline=(11, 12, 16, 255),
                    width=1
                )
            else:
                # Glowing LED halo
                draw.rounded_rectangle(
                    [px, py, px + cell_size, py + cell_size],
                    radius=3,
                    fill=(r // 4, g // 4, b // 4, 255)
                )
                # LED core
                draw.rounded_rectangle(
                    [px + 1, py + 1, px + cell_size - 1, py + cell_size - 1],
                    radius=2,
                    fill=(r, g, b, 255)
                )
                # Specular dome highlight
                hl_r = min(255, r + 90)
                hl_g = min(255, g + 90)
                hl_b = min(255, b + 90)
                draw.rectangle(
                    [px + 3, py + 3, px + 4, py + 4],
                    fill=(hl_r, hl_g, hl_b, 220)
                )

    # Optional bottom subtitle
    if subtitle:
        footer_top = grid_top + grid_h + margin
        draw.line([margin + 2, footer_top - 1, total_w - margin - 2, footer_top - 1], fill=(26, 30, 40, 255), width=1)
        draw.text((total_w // 2, footer_top + 10), subtitle, fill=(120, 140, 165, 255), anchor="mm")

    return card

def generate_transparent_device():
    """Converts raw Ditoo Pro device photo into transparent PNG with soft ground shadow."""
    raw_path = "assets/screenshots/ditoo_device_transparent.png"
    if not os.path.exists(raw_path):
        return

    im = Image.open(raw_path).convert("RGBA")
    alpha = np.array(im)[:, :, 3]
    y_indices, x_indices = np.where(alpha > 10)
    x0, x1 = x_indices.min(), x_indices.max()
    y0, y1 = y_indices.min(), y_indices.max()

    cropped = im.crop((x0, y0, x1, y1))
    target_h = 260
    target_w = int(cropped.width * (target_h / cropped.height))
    resized = cropped.resize((target_w, target_h), Image.Resampling.LANCZOS)

    canvas_w = target_w + 40
    canvas_h = target_h + 30
    canvas = Image.new("RGBA", (canvas_w, canvas_h), (0, 0, 0, 0))

    shadow_layer = Image.new("RGBA", (canvas_w, canvas_h), (0, 0, 0, 0))
    s_draw = ImageDraw.Draw(shadow_layer)
    s_draw.ellipse([20, canvas_h - 25, canvas_w - 20, canvas_h - 5], fill=(0, 0, 0, 110))
    shadow_layer = shadow_layer.filter(ImageFilter.GaussianBlur(radius=6))

    canvas.paste(shadow_layer, (0, 0), shadow_layer)
    canvas.paste(resized, (20, 10), resized)
    canvas.save("assets/screenshots/ditoo_device.png", "PNG", optimize=True)

def main():
    os.makedirs("assets/screenshots", exist_ok=True)
    generate_transparent_device()

    card_cell = 9
    card_margin = 6
    card_w = 16 * card_cell + 2 * card_margin # 156
    gap_x = 12
    margin_x = 12
    margin_y = 12
    cols = 4

    # 1. Generate 4-screen live display banner (screens_showcase.png)
    screens = [
        ("target/screens/clock.png", "Native Clock", "Emerald Green RTC"),
        ("target/screens/sysmon.png", "CPU & RAM Mon", "C 42% | R 58%"),
        ("target/screens/battery.png", "Device Battery", "85% Charging ⚡"),
        ("target/screens/disk_warning.png", "Disk Warning", "88% Storage Alert"),
    ]

    card_h_screen = 16 * card_cell + 2 * card_margin + 24 + 24 # 204
    total_w = cols * card_w + (cols + 1) * gap_x # 684
    total_h_screen = card_h_screen + 2 * margin_y # 228

    screens_banner = Image.new("RGBA", (total_w, total_h_screen), (9, 10, 13, 255))
    sb_draw = ImageDraw.Draw(screens_banner)
    sb_draw.rounded_rectangle(
        [1, 1, total_w - 2, total_h_screen - 2],
        radius=10,
        fill=(11, 12, 16, 255),
        outline=(26, 30, 40, 255),
        width=1
    )

    for idx, (path, title, subtitle) in enumerate(screens):
        im = Image.open(path)
        card = render_led_display(im, cell_size=card_cell, margin=card_margin, title=title, subtitle=subtitle)
        x_off = gap_x + idx * (card_w + gap_x)
        screens_banner.paste(card, (x_off, margin_y))

    screens_banner.save("assets/screenshots/screens_showcase.png")

    # 2. Generate 12-icon catalog grid banner (icons_grid.png)
    icons_showcase = [
        ("claude", "Claude Code"),
        ("codex", "Codex CLI"),
        ("antigravity", "Antigravity"),
        ("terminal", "Terminal"),
        ("mail", "Email / Mail"),
        ("chat", "Chat / Slack"),
        ("git", "Git / GitHub"),
        ("music", "Music Player"),
        ("bell", "Reminder"),
        ("disk", "Floppy Disk"),
        ("done", "Task Done"),
        ("alert", "Action Prompt"),
    ]

    rows = 3
    gap_y = 12
    card_h_icon = 16 * card_cell + 2 * card_margin + 24 # 180
    total_h_icons = rows * card_h_icon + (rows + 1) * gap_y # 588

    icons_banner = Image.new("RGBA", (total_w, total_h_icons), (9, 10, 13, 255))
    ib_draw = ImageDraw.Draw(icons_banner)
    ib_draw.rounded_rectangle(
        [1, 1, total_w - 2, total_h_icons - 2],
        radius=10,
        fill=(11, 12, 16, 255),
        outline=(26, 30, 40, 255),
        width=1
    )

    for idx, (name, label) in enumerate(icons_showcase):
        col = idx % cols
        row = idx // cols
        x_off = gap_x + col * (card_w + gap_x)
        y_off = gap_y + row * (card_h_icon + gap_y)

        icon_path = f"assets/icons/{name}.png"
        if not os.path.exists(icon_path):
            continue
        icon_im = Image.open(icon_path)
        card = render_led_display(icon_im, cell_size=card_cell, margin=card_margin, title=label)
        icons_banner.paste(card, (x_off, y_off))

    icons_banner.save("assets/screenshots/icons_grid.png")
    print("Rendered screens_showcase.png and icons_grid.png successfully with exact matching 684px width!")

if __name__ == "__main__":
    main()
