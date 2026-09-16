"""Reports the real Whisper tokenizer's structure.

Written because three assertions failed against the actual file and guessing at
why would have meant guessing at the tokenizer's shape. In particular: the code
counted 51,865 defined ids where `config.json` says 51,864, which is one more
than it should be — consistent with an id appearing in both `vocab` and
`added_tokens` and being counted twice.

Run: python scripts/probe-tokenizer.py
"""

import json
import os
import pathlib

DIR = pathlib.Path(os.environ["USERPROFILE"]) / ".loom" / "voice" / "whisper"


def load(name):
    path = DIR / name
    if not path.exists():
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def main():
    tokenizer = load("tokenizer.json")
    config = load("config.json")
    generation = load("generation_config.json")

    if tokenizer is None:
        print(f"missing: {DIR / 'tokenizer.json'}")
        print("Run: python scripts/fetch-whisper.py")
        return 1

    vocab = tokenizer["model"]["vocab"]
    added = tokenizer.get("added_tokens", [])

    vocab_ids = list(vocab.values())
    added_ids = [token["id"] for token in added]

    print(f"vocab entries      : {len(vocab):,}")
    print(f"added_tokens       : {len(added):,}")
    print(f"vocab id range     : {min(vocab_ids):,} .. {max(vocab_ids):,}")
    print(f"added id range     : {min(added_ids):,} .. {max(added_ids):,}")
    print()

    overlap = sorted(set(vocab_ids) & set(added_ids))
    print(f"OVERLAP            : {len(overlap)} id(s)")
    for id_ in overlap[:10]:
        # Which token holds it in each map.
        from_vocab = [t for t, i in vocab.items() if i == id_]
        from_added = [t["content"] for t in added if t["id"] == id_]
        print(f"  id {id_:,}: vocab {from_vocab}, added {from_added}")
    print()

    unique = len(set(vocab_ids) | set(added_ids))
    print(f"unique ids         : {unique:,}")
    print(f"counted naively    : {len(vocab) + len(added):,}  (this over-counts by {len(vocab) + len(added) - unique})")
    print()

    if config:
        print(f"config.vocab_size        : {config.get('vocab_size')}")
        print(f"config.eos_token_id      : {config.get('eos_token_id')}")
        print(f"config.bos_token_id      : {config.get('bos_token_id')}")
        print(f"config.no_timestamps     : {config.get('no_timestamps_token_id')}")
        print(f"config.forced_decoder    : {config.get('forced_decoder_ids')}")
    else:
        print("config.json missing")
    print()

    if generation:
        print(f"generation.no_timestamps : {generation.get('no_timestamps_token_id')}")
        print(f"generation.eos           : {generation.get('eos_token_id')}")
        print(f"generation.bos           : {generation.get('bos_token_id')}")
    else:
        print("generation_config.json missing")
    print()

    # How many tokens hold a *partial* UTF-8 sequence, which is why a per-token
    # decode can legitimately produce replacement characters.
    def bytes_to_unicode():
        printable = list(range(33, 127)) + list(range(161, 173)) + list(range(174, 256))
        codes = list(printable)
        n = 0
        for byte in range(256):
            if byte not in printable:
                printable.append(byte)
                codes.append(256 + n)
                n += 1
        return {chr(c): b for b, c in zip(printable, codes)}

    table = bytes_to_unicode()
    partial = 0
    for token in vocab:
        raw = bytes(table[ch] for ch in token if ch in table)
        if len(raw) < len(token.encode("utf-8", "replace")):
            pass
        try:
            raw.decode("utf-8")
        except UnicodeDecodeError:
            partial += 1

    print(f"tokens holding a partial UTF-8 sequence: {partial:,}")
    print(f"  of {len(vocab):,} ({100 * partial / len(vocab):.2f}%)")
    print("  These decode to a replacement character *on their own* and are")
    print("  correct: the rest of the character is in the next token.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
