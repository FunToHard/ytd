import os
from PIL import Image, ImageDraw

def create_ytd_icon(size):
    # Render at 4x scale for high-fidelity anti-aliasing
    scale = 4
    canvas_size = size * scale
    img = Image.new("RGBA", (canvas_size, canvas_size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    
    # Red background with rounded corners
    padding = max(1, int(canvas_size * 0.08))
    corner_radius = max(2, int(canvas_size * 0.22))
    draw.rounded_rectangle(
        [padding, padding, canvas_size - padding, canvas_size - padding],
        radius=corner_radius,
        fill=(255, 0, 0, 255)
    )
    
    # White Play / Download Arrow (optically centered)
    cx, cy = canvas_size // 2, canvas_size // 2
    r = int(canvas_size * 0.24)
    w_half = int(r * 0.95)
    h_half = int(r * 1.05)
    offset_x = int(r * 0.15)
    
    triangle = [
        (cx - w_half + offset_x, cy - h_half),
        (cx + w_half + offset_x, cy),
        (cx - w_half + offset_x, cy + h_half)
    ]
    draw.polygon(triangle, fill=(255, 255, 255, 255))
    
    # Downsample using high-quality Lanczos resampling
    return img.resize((size, size), Image.Resampling.LANCZOS)

def main():
    os.makedirs("daemon/resources", exist_ok=True)
    os.makedirs("extension/icons", exist_ok=True)
    
    # Extension PNG icons (16, 48, 128)
    for s in [16, 48, 128]:
        icon = create_ytd_icon(s)
        icon.save(f"extension/icons/icon{s}.png")
        print(f"Generated extension/icons/icon{s}.png ({s}x{s})")
        
    # Windows multi-resolution .ico file
    # Note: In Pillow, the base image MUST be the largest resolution (256x256),
    # otherwise Pillow's IcoImagePlugin silently drops all sizes larger than the base image!
    sizes = [16, 24, 32, 48, 64, 128, 256]
    images = [create_ytd_icon(s) for s in sizes]
    images[-1].save(
        "daemon/resources/app-icon.ico",
        format="ICO",
        sizes=[(s, s) for s in sizes],
        append_images=images[:-1]
    )
    print("Generated daemon/resources/app-icon.ico (with resolutions 16, 24, 32, 48, 64, 128, 256)")

    # Raw 32x32 RGBA bytes for tray-icon
    icon32 = create_ytd_icon(32)
    with open("daemon/resources/icon_32x32.rgba", "wb") as f:
        f.write(icon32.tobytes())
    print("Generated daemon/resources/icon_32x32.rgba (32x32 RGBA)")

if __name__ == "__main__":
    main()
