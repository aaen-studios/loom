import CoreGraphics
import Testing
@testable import Loom

/// How a reply reads (PLAN.md, "One answer at the end"), its pictures' fan, and its cards,
/// as desktop/src/model.rs lays them out.
@MainActor
@Suite struct ReplyTests {
    func reply(_ blocks: [Block], live: Bool = false, status: String = "done") -> Message {
        Message(id: "m_1", role: "agent", at: 0, blocks: blocks, live: live, status: status)
    }

    func tool(_ call: String, parent: String? = nil) -> Block {
        var b = Block.text("tool", "")
        b.call = call
        b.parent = parent
        return b
    }

    func picture(_ name: String, mime: String = "image/png") -> Block {
        Block(kind: "file", text: "", call: nil, file: FileRef(file: name, name: name, mime: mime, size: 1))
    }

    @Test func itsStepsFoldUnderOneLineAndItsAnswerIsLast() {
        let m = reply([.text("thought", "Which run?"), tool("t_1"), .text("text", "Checking the run."), tool("t_2"), .text("text", "It passed.")])
        #expect(parts(m, tools: []) == [
            .fold(words: "Took 4 steps", working: false, parts: [.block(0), .tools([1]), .words([2]), .tools([3])]),
            .words([4]),
        ])
    }

    @Test func oneStepIsSaidSo() {
        let m = reply([tool("t_1"), .text("text", "Done.")])
        #expect(parts(m, tools: []) == [.fold(words: "Took 1 step", working: false, parts: [.tools([0])]), .words([1])])
    }

    @Test func wordsThatAskStayInView() {
        let m = reply([.text("text", "Shall I tag it?"), tool("t_1"), .text("text", "Tagged.")])
        #expect(parts(m, tools: []) == [.fold(words: "Took 1 step", working: false, parts: [.tools([1])]), .words([0, 2])])
    }

    @Test func whileItWorksTheLineIsTheStepItsOn() {
        #expect(parts(reply([], live: true), tools: []) == [.fold(words: "Thinking…", working: true, parts: [])])
        let words = reply([.text("text", "Let me check the run.\nThen the notes.")], live: true)
        #expect(parts(words, tools: []) == [.fold(words: "Let me check the run.", working: true, parts: [.words([0])])])
        let calling = reply([.text("text", "Let me check."), tool("t_1")], live: true)
        let tools = [Tool(call: "t_1", run: "r_1", title: "Check the last run", status: nil)]
        #expect(parts(calling, tools: tools) == [.fold(words: "Check the last run…", working: true, parts: [.words([0]), .tools([1])])])
    }

    @Test func stepsSayWhatTheyDoInPlainWords() {
        let worded = Tool(call: "t_1", run: "r_1", title: "Run on example-vps: tailscale serve --https=8444", status: nil, name: "run_command", doing: "Turning on HTTPS for port 8444", did: "Turned on HTTPS for port 8444", said: true)
        // While it runs, the line is what the call is doing, and the words it was made of
        // aren't a step of their own.
        let running = reply([.text("text", "Let me turn on HTTPS for port 8444."), tool("t_1")], live: true)
        #expect(parts(running, tools: [worded]) == [.fold(words: "Turning on HTTPS for port 8444…", working: true, parts: [.tools([1])])])
        let done = reply([.text("text", "Let me turn on HTTPS for port 8444."), tool("t_1"), .text("text", "Done.")])
        #expect(parts(done, tools: [worded]) == [.fold(words: "Took 1 step", working: false, parts: [.tools([1])]), .words([2])])
        #expect(stepCount([.words([0]), .tools([1, 2])]) == 3)
        #expect(worded.didWords == "Turned on HTTPS for port 8444")
        #expect(Tool(call: "t_2", run: "r_1", title: "ls", status: nil).didWords == "ls")
    }

    @Test func aTurnCutShortKeepsAllInView() {
        let m = reply([.text("thought", "Which run?"), .text("text", "Checking.")], status: "failed")
        #expect(parts(m, tools: []) == [.block(0), .words([1])])
    }

    @Test func aSubagentsStepsArentTheReplys() {
        var inside = Block.text("text", "Inside the subagent.")
        inside.parent = "t_1"
        let m = reply([tool("t_1"), inside, .text("text", "Done.")])
        #expect(insideACall(m) == [false, true, false])
        #expect(parts(m, tools: []) == [.fold(words: "Took 1 step", working: false, parts: [.tools([0])]), .words([2])])
    }

    @Test func picturesTogetherAreOneFan() {
        let m = reply([.text("text", "Here they are."), picture("a"), picture("b"), picture("c")])
        #expect(parts(m, tools: []) == [.words([0]), .pictures([1, 2, 3])])
    }

    @Test func aPersonsFilesGoAboveTheirWordsWithPicturesAsOneFan() {
        let m = Message(id: "m_2", role: "person", at: 0, blocks: [picture("notes.pdf", mime: "application/pdf"), picture("a"), .text("text", "These?"), picture("b")])
        #expect(fileParts(m) == [.pictures([1, 3]), .block(0)])
        #expect(parts(m, tools: []) == [.block(0), .block(1), .words([2]), .block(3)])
    }

    @Test func aCallGoesWhereToolStartedSaid() {
        var blocks: [Block] = [.text("text", "Let me look.")]
        placeTools(&blocks, [Tool(call: "t_1", run: "r_1", title: "", status: nil, block: 2)])
        #expect(blocks.map(\.kind) == ["text", "", "tool"])
        #expect(blocks[2].call == "t_1")
        // One with no block, or whose place has words, goes on the end; one there already stays.
        placeTools(&blocks, [Tool(call: "t_1", run: "r_1", title: "", status: nil, block: 2), Tool(call: "t_2", run: "r_1", title: "", status: nil), Tool(call: "t_3", run: "r_1", title: "", status: nil, block: 0)])
        #expect(blocks.compactMap(\.call) == ["t_1", "t_2", "t_3"])
        #expect(blocks.count == 5)
    }

    @Test func theFanLaysOutAsTheDesktopsAtRest() {
        #expect(fanCards(1).isEmpty)
        #expect(fanCards(2) == [FanCard(picture: 1, x: 0.1, tilt: 6, scale: 0.95)])
        #expect(fanCards(3).map(\.picture) == [1, 2])
        #expect(fanCards(3)[0].x == -fanCards(3)[1].x)
        // Four: the fourth furthest back, on the left.
        #expect(fanCards(4).map(\.picture) == [3, 1, 2])
        #expect(fanCards(4)[0].x < 0)
        // Five or more: only five are drawn, and the corner says how many there are.
        #expect(fanCards(5).count == 4)
        #expect(fanCards(9) == fanCards(5))
        #expect(fanCount(9) == "9")
    }

    @Test func theFanKeepsTheRoomItsCardsReachInto() {
        let three = fanReach(3, aspect: 0.75)
        #expect(abs(three.left - three.right) < 1e-9)
        #expect(three.up > 0)
        let two = fanReach(2, aspect: 0.75)
        #expect(two.right > two.left)
        let none = fanReach(1, aspect: 0.75)
        #expect(none.left == 0 && none.right == 0 && none.up == 0)
    }

    @Test func aFormsAnswersByLabel() {
        let fields = [
            FormField(label: "When", kind: "choice", required: true, options: ["Morning", "Evening"], placeholder: "", value: "Evening"),
            FormField(label: "Litres", kind: "number", required: true, options: [], placeholder: "2", value: 2),
            FormField(label: "Remind me", kind: "toggle", required: false, options: [], placeholder: "", value: true),
            FormField(label: "Note", kind: "text", required: false, options: [], placeholder: "", value: ""),
        ]
        #expect(fields[1].startText == "2")
        #expect(formAnswers(fields, typed: ["", " 3 ", "", "  "], picks: ["Evening", .null, false, .null]) == .success(["When": "Evening", "Litres": "3", "Remind me": false]))
        #expect(formAnswers(fields, typed: ["", "", "", ""], picks: ["Evening", .null, true, .null]) == .failure(FormMissing(label: "Litres")))
        #expect(formAnswers(fields, typed: ["", "3", "", ""], picks: ["", .null, true, .null]) == .failure(FormMissing(label: "When")))
    }

    @Test func aChoicesCardSaysTheAnswerSentAndGoesQuiet() throws {
        let m = caughtUp()
        m.select("a_1")
        let ask: JSON = ["id": "m_1", "conversation": "c_1", "role": "agent", "agent": "a_1", "at": 1,
                         "blocks": [["kind": "card", "card": ["id": "card_1", "type": "choices", "title": "Shall I tell Ola yes?", "options": ["Yes", "Not yet"]]]]]
        came(m, [ask])
        let open = try #require(m.shown.first?.blocks.first?.card)
        #expect(open.takesAnswers)
        #expect(open.chosen == nil)
        came(m, [ask, message("m_2", in: "c_1", role: "person", text: "Not yet")])
        let answered = try #require(m.shown.first?.blocks.first?.card)
        #expect(answered.chosen == 1)
        #expect(answered.passed)
        #expect(!answered.takesAnswers)
    }

    @Test func aCardInAReplyStillBeingWrittenAnswersLater() throws {
        let card = try #require(Card(["id": "card_1", "type": "form", "title": "Next watering", "fields": []]))
        var live = card
        live.live = true
        #expect(!live.takesAnswers && live.answersLater)
        #expect(Card(["id": "x", "type": "poster"]) == nil)
    }
}
