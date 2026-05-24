import json
import pyperclip
text = pyperclip.paste()

json_string = json.dumps(text)

print(json_string)
pyperclip.copy(json_string)
