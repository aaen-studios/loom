# Building the iPhone app

Loom isn't on TestFlight or the App Store yet. Until it is, you build the app on a Mac and put it on your phone with your own Apple account.

## What you need

- A Mac with Xcode.
- [XcodeGen](https://github.com/yonaskolb/XcodeGen): `brew install xcodegen`.
- [Calx](https://calxlang.com), nightly 202611231257 or newer (`calx update` brings an older one up to date).
- For your phone, an Apple account. A free one works, but the app it signs stops after 7 days and has to be installed again. With Apple's developer program (99 USD a year) it lasts a year.

## Building it

```sh
git clone https://github.com/aaen-studios/loom.git
cd loom
calx scripts/ios.calx
open ios/Loom.xcodeproj
```

`calx scripts/ios.calx` builds loom's core for the phone and the simulator, and makes the Xcode project, `ios/Loom.xcodeproj`. `calx scripts/ios.calx --sim` builds the core for the simulator alone, which is faster.

The script makes the project anew each time, from `ios/project.yml`, so what you change in Xcode's project settings lasts only until you run it again. Change `ios/project.yml` for anything you want to keep.

## In the simulator

In Xcode, pick an iPhone simulator at the top and choose **Run**.

The simulator has no camera, so **Scan a code** gives you a field to paste the code's text into instead, which starts with `loom://link`. Or sign in with the instance's address, your username and your password. An instance on the same Mac is at `http://127.0.0.1:` and its port.

## On your phone

1. In Xcode, open **Settings → Accounts** and add your Apple account.
2. Select the **Loom** project, then the **Loom** target, and open **Signing & Capabilities**. Pick your team, and change the bundle identifier from `no.aaenz.loom` to one of your own, such as `com.yourname.loom`.
3. Connect your phone with a cable, and pick it at the top of Xcode's window. The first time, the phone asks you to turn on Developer Mode, in **Settings → Privacy & Security**, and to restart.
4. Choose **Run**. The first time, the phone won't open an app from a developer it doesn't know: in **Settings → General → VPN & Device Management**, trust your account.
5. Open loom and add your phone as [Phones](phones.md) says.

## Notifications

The app shows notifications while it's open. Notifications on the lock screen, through Apple's push service, aren't in the app yet.
