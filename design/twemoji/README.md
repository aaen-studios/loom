# Twemoji

The pictures loom draws built-in emoji with, on the desktop and the iPhone ([PLAN.md, "Emoji"](../../PLAN.md#emoji)): [Twemoji](https://github.com/jdecked/twemoji) 17.0.3, the set Discord uses.

Twemoji's graphics are copyright 2014–2021 Twitter and 2022–present Jason Sofonia and Justine De Caires, and licensed under [CC-BY 4.0](https://creativecommons.org/licenses/by/4.0/), in [LICENSE-GRAPHICS](LICENSE-GRAPHICS). The apps' licences say so, with a link.

- `svg/`: one SVG for each emoji, named as Twemoji names them: the code points in hex, joined by `-`, without U+FE0F unless the emoji joins with U+200D (`1f62d.svg` is 😭).
- `sequences.txt`: each way an emoji is written, fully qualified or not, a tab, and its picture's name, for the apps to find emoji in a message. A rune that's text unless U+FE0F follows it, such as © or ❤, is an emoji only with it.

`calx scripts/emoji.calx` makes both, and `shared/emoji/emoji.tsv`, from the versions it pins; `--twemoji 17.0.4` vendors another release.
