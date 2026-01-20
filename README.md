Small tool to help maintaining Medieval 2 Total War mod translations. Specifically, this tool can:

- Automatically update mod translations to a newer version of said mod (no more manually diffing and editing txt files).
- Automatically translate text unchanged from vanilla, which makes it so you only need to translate the lines in your mod that are actually from the mod.
- Automatically translate text using the DeepL API.

## How to use it

How to translate the vanilla text of a mod:

- Put the txt files from the game in your language in "translated_old".
- Put the txt files from the most up-to-date version of the mod in "eng_new".
- Put the txt files from the game in english in "eng_old".
- Launch the program. Your new files should appear in "output" when the terminal closes.

How to update a mod translation:

- Put the txt files from your old translation in "translated_old".
- Put the txt files from the most up-to-date version of the mod in "eng_new".
- Put the txt files from the version of the mod for which the old translation is for in "eng_old".
- Launch the program. Your new files should appear in "output" when the terminal closes.

How to auto-translate a mod:

- Do the same steps as for updating a mod translation, but don't launch the program yet.
- Make an environment variable called DEEPL_API_KEY with a valid DeepL API key in it.
- Shift-Right-Click the folder containing the program, and hit "Open Powershell here".
- In the terminal, execute `.\med2_translator.exe -l XX`, where XX is the language code you want to translate to. You can get a list of language codes executing `.\med2_translator.exe -h`.
- Once it finishes, you'll have your files in the output folder.
