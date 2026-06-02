from PIL import Image
import sys

if len(sys.argv) != 5:
    print("Usage: python resize.py <input> <output> <width> <height>")
    sys.exit(1)

input_file = sys.argv[1]
output_file = sys.argv[2]
width = int(sys.argv[3])
height = int(sys.argv[4])

img = Image.open(input_file)

# Resize by stretching/squashing to exactly match the target size
resized = img.resize((width, height))

resized.save(output_file)

print(f"Saved {output_file} ({width}x{height})")