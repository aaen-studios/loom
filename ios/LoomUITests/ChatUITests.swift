import XCTest

/// The chat as a person uses it, on sample frames: the emoji picker in the keyboard's
/// place (LOO-361) and a long message cut with Show more (LOO-357). Each step keeps a
/// screenshot in the result bundle.
@MainActor
final class ChatUITests: XCTestCase {
    /// The app on sample frames, at the Builder's chat, whose reply with the release
    /// notes is long enough to be cut.
    private func builder() -> XCUIApplication {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["-sample", "-open", "a_1"]
        app.launch()
        XCTAssertTrue(app.buttons["Emoji"].waitForExistence(timeout: 20), "the chat's composer")
        return app
    }

    private func shot(_ app: XCUIApplication, _ name: String) {
        let a = XCTAttachment(screenshot: app.screenshot())
        a.name = name
        a.lifetime = .keepAlways
        add(a)
    }

    /// Opens the emoji picker with the composer's smiley.
    private func openPicker(_ app: XCUIApplication) -> XCUIElement {
        app.buttons["Emoji"].tap()
        let find = app.textFields["Find an emoji"]
        XCTAssertTrue(find.waitForExistence(timeout: 5), "the picker")
        XCTAssertTrue(app.buttons["Keyboard"].exists, "the smiley is a keyboard while the picker is up")
        return find
    }

    // MARK: The emoji picker (LOO-361)

    func testASwipeDownOnThePickerAtItsTopPutsItAway() {
        let app = builder()
        let find = openPicker(app)
        shot(app, "picker up")
        let cell = app.buttons[":grinning:"].firstMatch
        XCTAssertTrue(cell.waitForExistence(timeout: 5))
        let from = cell.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
        from.press(forDuration: 0.05, thenDragTo: from.withOffset(CGVector(dx: 0, dy: 220)))
        XCTAssertTrue(find.waitForNonExistence(timeout: 5), "the picker went")
        XCTAssertTrue(app.buttons["Emoji"].exists)
        shot(app, "picker swiped away")
    }

    func testATapInTheChatPutsThePickerAway() {
        let app = builder()
        let find = openPicker(app)
        let words = app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "Did the build on main pass")).firstMatch
        XCTAssertTrue(words.waitForExistence(timeout: 5))
        words.tap()
        XCTAssertTrue(find.waitForNonExistence(timeout: 5), "the picker went")
        XCTAssertTrue(app.buttons["Emoji"].exists)
        shot(app, "picker tapped away")
    }

    func testTheKeyboardGlyphSwapsTheKeyboardAndThePicker() {
        let app = builder()
        let find = openPicker(app)
        let field = app.textViews["Message"]
        app.buttons["Keyboard"].tap()
        XCTAssertTrue(find.waitForNonExistence(timeout: 5), "the picker went for the keyboard")
        XCTAssertTrue(app.buttons["Emoji"].waitForExistence(timeout: 2))
        XCTAssertTrue(field.value(forKey: "hasKeyboardFocus") as? Bool ?? false, "the composer has the cursor")
        shot(app, "keyboard back")
        app.buttons["Emoji"].tap()
        XCTAssertTrue(find.waitForExistence(timeout: 5), "the picker came back")
        XCTAssertTrue(app.buttons["Keyboard"].exists)
        shot(app, "picker back")
    }

    // MARK: A long message (LOO-357)

    func testShowMoreOpensALongMessageAndShowLessFoldsIt() {
        let app = builder()
        let more = app.buttons["Show more"].firstMatch
        XCTAssertTrue(more.waitForExistence(timeout: 10), "the long reply is cut")
        shot(app, "cut")
        more.tap()
        let less = app.buttons["Show less"].firstMatch
        XCTAssertTrue(less.waitForExistence(timeout: 5), "it opened")
        XCTAssertFalse(app.buttons["Show more"].exists)
        shot(app, "opened")
        less.tap()
        XCTAssertTrue(app.buttons["Show more"].firstMatch.waitForExistence(timeout: 5), "it folded again")
        shot(app, "folded again")
    }

    func testATapOnTheCutWordsOpensThem() {
        let app = builder()
        XCTAssertTrue(app.buttons["Show more"].firstMatch.waitForExistence(timeout: 10), "the long reply is cut")
        let words = app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "Routines run on webhooks")).firstMatch
        XCTAssertTrue(words.waitForExistence(timeout: 5))
        words.tap()
        XCTAssertTrue(app.buttons["Show less"].firstMatch.waitForExistence(timeout: 5), "it opened")
        shot(app, "opened by a tap on its words")
    }
}
