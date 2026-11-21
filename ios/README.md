# ios

Swift and SwiftUI: the iPhone app, in one Xcode project generated from a spec with [XcodeGen](https://github.com/yonaskolb/XcodeGen) rather than a hand-edited `.xcodeproj`. It links the same [core](../core/) as the desktop app, and does less than the desktop where that suits a phone. Its widgets, Live Activities and push extension join the same project later.

The app holds no logic (PLAN.md, "The apps hold no logic"). It draws screens and reaches the system: the camera, photos, files, notifications, the clipboard. Everything else is the core's, which it talks to only in JSON, through `loom_core_start`, `loom_core_send` and `loom_core_stop` ([core/README.md](../core/README.md#between-the-app-and-the-core)).

## Building it

On a Mac with Xcode and XcodeGen (`brew install xcodegen`), from the repository's root:

```sh
calx scripts/ios.calx           # the core for a phone and the simulator, then ios/Loom.xcodeproj
calx scripts/ios.calx --sim     # the core for the simulator alone, which builds faster
calx scripts/ios.calx --sample  # no core: the project alone, with the app on sample frames
open ios/Loom.xcodeproj
```

[`scripts/ios.calx`](../scripts/ios.calx) builds `core/lib.calx` with `calx build --staticlib --target aarch64-ios` and `aarch64-ios-sim`, gives `loomcore.h` a module map so Swift imports it as `LoomCore`, puts both in `build/ios/LoomCore.xcframework`, and runs `xcodegen` with `LOOM_CORE=YES`, which includes [`core.yml`](core.yml) to link it. `Loom.xcodeproj` isn't kept in git: change [`project.yml`](project.yml) and run the script again.

A build without the core (`--sample`, or `xcodegen` run by hand) runs on [`SampleCore`](Loom/Core/SampleCore.swift): sample agents and chats, a reply that streams, and a question to answer, in the frames the core sends. SwiftUI previews use it too. With the core linked, `-sample` at launch runs on it as well.

To run against an instance, set `LOOM_SERVER` in the scheme's environment, or type its address on the sign-in screen.

## Files

- [`Loom/Core/`](Loom/Core/): the C boundary. [`Core.swift`](Loom/Core/Core.swift) starts the core with its config (its cache in Application Support/Loom), sends it frames, and hands what it says to the main actor in batches. [`JSON.swift`](Loom/Core/JSON.swift) is the frames' JSON. [`SampleCore.swift`](Loom/Core/SampleCore.swift) stands in for the core without it.
- [`Loom/Model/Model.swift`](Loom/Model/Model.swift): what the screens show, as the core tells it, as [`desktop/src/model.rs`](../desktop/src/model.rs) does for the desktop app. Lists come from the core's cache with `core.get` and are fetched again when an event says they changed. Only a reply still being written lives here alone, from its deltas.
- [`Loom/App/`](Loom/App/): the app, and [`Session.swift`](Loom/App/Session.swift), which joins the core, the model and the system: `app.active` and `app.wake` as the app comes and goes, `app.network` when the network changes, `core.device_ask` for the clipboard, and `core.notify` as a notification. [`Notifications.swift`](Loom/App/Notifications.swift) asks for permission, and gives an approval Allow and Deny.
- [`Loom/Views/`](Loom/Views/): the screens.
- [`Loom/Design/`](Loom/Design/): loom's colours and type from the design tokens ([`design/generated/Tokens.swift`](../design/generated/Tokens.swift), with the `phone` sizes), each agent's thread, and Markdown.

## The screens

- **Sign in**: the instance's address, a username and a password (`login`), or **Scan a code**, VisionKit's scanner reading the QR code the desktop app shows under Add a phone (`link`, with the code's text). The simulator has no camera, so there the text can be pasted.
- **Agents**: each agent as one ongoing chat, with its thread and what it's doing, as the desktop's sidebar shows them.
- **A chat**: messages with Markdown and code blocks, the reply streaming in as it's written, thoughts folded and tool calls on one quiet line, questions with Allow and Deny, and the composer, with photos (sent as JPEG) and files.
- **Settings**: the account, the instance, the person's devices, notifications, and signing out.

For a screenshot of each screen, launch it with `-sample`, and `-signedOut`, `-open <agent id>` or `-settings`: `xcrun simctl launch booted no.aaenz.loom -sample -open a_1`.

## Push

Notifications show while the app runs, from the core's `core.notify`. Push to the lock screen comes with the instance's side ([LOO-47](https://acta.aaenz.no/aaen-studios/loom#LOO-47)). The place for it is ready:

- **Permission**: Settings → Notifications asks, and on a yes the app registers with APNs.
- **The token**: `Session.registered(pushToken:)` gets it, and sends the core the `push.register` command once `Session.pushRegisterReady` is turned on, when the instance has it.
- **Allow and Deny**: an `approval.needed` notification has both, answered as `approve` without opening the app.
- **Still to come**: the `aps-environment` entitlement, which needs the app signed with Elliot's Apple team, and the notification service extension for the push relay's encrypted notifications (Phase 10).
