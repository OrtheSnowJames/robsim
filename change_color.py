from PIL import Image
import argparse

def hex_to_rgb(hex_color):
    hex_color = hex_color.lstrip("#")
    if len(hex_color) != 6:
        raise ValueError("Colors must be in the format #RRGGBB")
    return tuple(int(hex_color[i:i+2], 16) for i in (0, 2, 4))

parser = argparse.ArgumentParser(
    description="Replace a color in an image."
)

parser.add_argument("input", help="Input image")
parser.add_argument("output", help="Output image")
parser.add_argument("old_color", help="Color to replace (e.g. #FF0000)")
parser.add_argument("new_color", help="Replacement color (e.g. #00FF00)")

args = parser.parse_args()

old_rgb = hex_to_rgb(args.old_color)
new_rgb = hex_to_rgb(args.new_color)

img = Image.open(args.input).convert("RGBA")
pixels = img.load()

for y in range(img.height):
    for x in range(img.width):
        r, g, b, a = pixels[x, y]

        if (r, g, b) == old_rgb:
            pixels[x, y] = (*new_rgb, a)

img.save(args.output)

print(
    f"Replaced {args.old_color} with {args.new_color} "
    f"and saved to {args.output}"
)