from PIL import Image
import colorsys

INPUT = "assets/table.png"
OUTPUT = "dark_oak.png"

# tweak these if you want
BRIGHTNESS_MULT = 0.62
SATURATION_MULT = 0.85
RED_MULT = 0.92
GREEN_MULT = 0.78
BLUE_MULT = 0.72

img = Image.open(INPUT).convert("RGBA")
pixels = img.load()

for y in range(img.height):
    for x in range(img.width):
        r, g, b, a = pixels[x, y]

        # skip transparent pixels
        if a == 0:
            continue

        # normalize
        rf = r / 255.0
        gf = g / 255.0
        bf = b / 255.0

        # rgb -> hsv
        h, s, v = colorsys.rgb_to_hsv(rf, gf, bf)

        # target warm wood tones only
        # roughly catches oak/tan/brown hues
        if 0.05 < h < 0.16:
            v *= BRIGHTNESS_MULT
            s *= SATURATION_MULT

            nr, ng, nb = colorsys.hsv_to_rgb(h, s, v)

            nr *= RED_MULT
            ng *= GREEN_MULT
            nb *= BLUE_MULT

            r = int(max(0, min(255, nr * 255)))
            g = int(max(0, min(255, ng * 255)))
            b = int(max(0, min(255, nb * 255)))

        pixels[x, y] = (r, g, b, a)

img.save(OUTPUT)
print(f"Saved to {OUTPUT}")