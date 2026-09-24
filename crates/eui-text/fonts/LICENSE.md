Inter (The Inter Project Authors, https://github.com/rsms/inter), JetBrains
Mono (JetBrains) and Noto Sans Symbols (Google) are all licensed under the
SIL Open Font License 1.1. They are embedded in the EUI client so that a
session makes no font request at all; Noto Sans Symbols is loaded last and
only fills glyphs the text faces lack.

Inter is embedded in four static weights — Regular, Medium, SemiBold and
Bold — one for each `font_weight` the wire names. All four are the
unmodified `extras/ttf/` files of the Inter 4.1 release
(https://github.com/rsms/inter/releases/tag/v4.1, font version 4.001,
git-9221beed3).
