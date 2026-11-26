import XCTest
@testable import Loom

/// Emoji drawn as pictures in Select text and the composer, selected and copied as the
/// text they are (LOO-347).
final class EmojiCopyTests: XCTestCase {
    private func whole(_ pieces: [Emoji.Piece]) -> NSRange {
        NSRange(location: 0, length: pieces.map(Emoji.shownLength).reduce(0, +))
    }

    private func emoji(_ p: Emoji.Piece) -> (copies: String, own: Bool)? {
        if case .emoji(let copies, _, let own) = p { return (copies, own) }
        return nil
    }

    func testEachEmojiIsOneCharacterAndCopiesAsItself() {
        let text = "Word 😭 word word 💢 wad"
        let pieces = Emoji.pieces(text, paths: [:])
        XCTAssertEqual(pieces, [
            .words("Word "),
            .emoji(copies: "😭", picture: "1f62d", own: false),
            .words(" word word "),
            .emoji(copies: "💢", picture: "1f4a2", own: false),
            .words(" wad"),
        ])
        XCTAssertEqual(Emoji.copied(pieces, whole(pieces)), text)
        // "rd ", the 😭's one character, " wo": inside the words on each side.
        XCTAssertEqual(Emoji.copied(pieces, NSRange(location: 2, length: 7)), "rd 😭 wo")
        // Up to the emoji and not over it.
        XCTAssertEqual(Emoji.copied(pieces, NSRange(location: 2, length: 3)), "rd ")
    }

    func testAnEmojiOfManyCodePointsIsOne() {
        let text = "a 👍🏽 b 👨‍👩‍👧 c #️⃣"
        let pieces = Emoji.pieces(text, paths: [:])
        XCTAssertEqual(pieces.compactMap(emoji).map(\.copies), ["👍🏽", "👨‍👩‍👧", "#️⃣"])
        XCTAssertEqual(pieces.filter { emoji($0) != nil }.map(Emoji.shownLength), [1, 1, 1])
        XCTAssertEqual(Emoji.copied(pieces, whole(pieces)), text)
        // The family alone: the selection takes it in whole.
        XCTAssertEqual(Emoji.copied(pieces, NSRange(location: 6, length: 1)), "👨‍👩‍👧")
    }

    func testThePersonsOwnCopyAsTheirNames() {
        let text = "hi <:party_cat:e_3> and <a:wave:e_4> there"
        let pieces = Emoji.pieces(text, paths: ["e_3": "/f/e_3", "e_4": "/f/e_4"])
        XCTAssertEqual(pieces.compactMap(emoji).map(\.copies), [":party_cat:", ":wave:"])
        XCTAssertEqual(pieces.compactMap(emoji).map(\.own), [true, true])
        XCTAssertEqual(Emoji.copied(pieces, whole(pieces)), "hi :party_cat: and :wave: there")
        // One whose picture isn't here yet is its name among the words.
        XCTAssertEqual(Emoji.pieces(text, paths: [:]), [.words("hi :party_cat: and :wave: there")])
    }

    func testCodeKeepsWhatsWritten() {
        let text = "x `:sob: 😭 <:knot:e_1>` y 😭"
        let pieces = Emoji.pieces(text, paths: ["e_1": "/f/e_1"])
        XCTAssertEqual(pieces.compactMap(emoji).map(\.copies), ["😭"])
        XCTAssertEqual(Emoji.copied(pieces, whole(pieces)), text)
    }

    func testTheComposerCopiesItsPiecesAsWordsAndPastesThemBack() {
        let parts: [Either] = [
            .words("a "),
            .piece(emojiPiece("<:party_cat:e_3>")),
            .words(" "),
            .piece(emojiPiece("😭")),
            .words(" see "),
            .piece(linkPiece("https://example.com")),
        ]
        let text = copiedText(parts)
        XCTAssertEqual(text, "a :party_cat: 😭 see https://example.com")
        // The emoji are kept as pieces, the link as its words.
        let kept = keptParts(parts)
        XCTAssertEqual(kept?.last, .words("https://example.com"))
        let data = try? JSONEncoder().encode(kept)
        XCTAssertEqual(pastedParts(text, kept: data).map(copiedText), text)
        XCTAssertEqual(pastedParts(text, kept: data)?[1], .piece(emojiPiece("<:party_cat:e_3>")))
        // Words that aren't the ones copied any more are pasted as words.
        XCTAssertNil(pastedParts("a :party_cat:", kept: data))
        // Nothing to keep without an emoji or a mention.
        XCTAssertNil(keptParts([.words("a "), .piece(linkPiece("https://example.com"))]))
    }
}
