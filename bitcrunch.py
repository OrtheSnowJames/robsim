from PIL import Image
import sys

# Usage:
# python bitcrunch.py input.png output.png 64 64

if len(sys.argv) != 5:
    print("Usage: python bitcrunch.py <input> <output> <width> <height>")
    sys.exit(1)

input_path = sys.argv[1]
output_path = sys.argv[2]

target_width = int(sys.argv[3])
target_height = int(sys.argv[4])

# Open image
img = Image.open(input_path)

# Shrink image permanently
bitcrunched = img.resize((target_width, target_height), Image.NEAREST)

# Save
bitcrunched.save(output_path)

print(f"Saved bitcrunched image to {output_path}")
print(f"New resolution: {target_width}x{target_height}")