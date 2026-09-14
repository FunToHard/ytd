import os
from PIL import Image, ImageDraw

def create_ytd_icon(size):
    # Create RGBA image
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    
    # Red background with rounded corners
    padding = max(1, size // 10)
    corner_radius = max(2, size // 4)
    draw.rounded_rectangle(
        [padding, padding, size - padding, size - padding],
        radius=corner_radius,
        fill=(255, 0, 0, 255)
    )
    
    # White Play / Download Arrow
    cx, cy = size // 2, size // 2
    r = size // 4
    
    # Play button triangle pointing right
    triangle = [
        (cx - r // 2, cy - r),
        (cx + r, cy),
        (cx - r // 2, cy + r)
    ]
    draw.polygon(triangle, fill=(255, 255, 255, 255))
    
    return img

def main():
    os.makedirs("daemon/resources", exist_ok=True)
    os.makedirs("extension/icons", exist_ok=True)
    
    # Extension PNG icons
    for s in [16, 48, 128]:
        icon = create_ytd_icon(s)
        icon.save(f"extension/icons/icon{s}.png")
        print(f"Generated extension/icons/icon{s}.png")
        
    # Windows .ico file
    sizes = [16, 32, 48, 64, 128, 256]
    images = [create_ytd_icon(s) for s in sizes]
    images[0].save(
        "daemon/resources/app-icon.ico",
        format="ICO",
        sizes=[(s, s) for s in sizes],
        append_images=images[1:]
    )
    print("Generated daemon/resources/app-icon.ico")

    # Raw 32x32 RGBA bytes for tray-icon
    icon32 = create_ytd_icon(32)
    with open("daemon/resources/icon_32x32.rgba", "wb") as f:
        f.write(icon32.tobytes())
    print("Generated daemon/resources/icon_32x32.rgba")

if __name__ == "__main__":
    main()
