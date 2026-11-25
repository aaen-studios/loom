# Adding a device

> Loom isn't on TestFlight or the App Store yet. To use it on your iPhone now, [build it yourself](iphone.md).

Add a phone or a computer from a device that's already signed in:

1. Open **Add a device**: on the desktop, it's in the server popup (click the server's name at the bottom of the sidebar). On the iPhone, it's in **Settings**.
2. It shows a QR code and a short code, such as `K7QF-M2XD@loom.example.com`, which holds the server's address too. On the new device, scan the QR code with loom on a phone, or paste the code into loom on a computer. **Copy link** gives a `loom://add?…` link instead, which opens loom on that code.
3. Both devices show the same four digits. On the device that's signed in, loom asks **Add "MacBook"?**. Choose **Add** if the digits match, or **Not me** if they don't.

A code works once, for ten minutes, and **New code** makes another. An ask no one answers runs out after five minutes. Each device gets a sign-in of its own. **Your devices**, on the same page, lists them with when each was last connected, and **Remove** signs one out.

A device that can't reach a server that's only on [Tailscale](tailscale.md) says so: turn Tailscale on there, signed in to the same account, and try again.

An instance still takes a username and password: choose **Sign in with a password** on the new device. An account made with a link code (`loom link`) has no password at first, and **Set a password**, under **Your account**, gives it one.

## An instance on a server

The phone reaches it at the server's address, over TLS, as the desktop app does.

## An instance in the desktop app

The instance listens only on your computer until you turn on **Other devices can reach this computer** on the **Add a device** page. Then it also listens on your network, on port 47420 (or another, if that one's taken), and takes only an encrypted connection there: a [Noise](https://noiseprotocol.org) handshake against the instance's key, which is in the QR code. Nothing reaches the instance on that port without it, and everything after it is encrypted end to end. Windows asks once whether loom may use the network; allow it on private networks.

- **At home**, the phone reaches the computer over the Wi-Fi.
- **Away from home**, it goes through [Tailscale](tailscale.md), if you use it: the QR code holds the computer's Tailscale address too, and the phone tries the Wi-Fi first and Tailscale next.

Turn the switch off to stop other devices reaching it again.

## Notifications

While the loom app is open, it shows notifications as they come. Notifications while it's closed aren't in the app yet. For those, the instance needs Apple's push service, and Apple takes only pushes signed with the key of the account that built the app. So an instance does one of these:

- **It has that key itself.** `loom push key AuthKey_….p8 KEY_ID TEAM_ID BUNDLE_ID` sets it up, if you build the app with your own Apple account.
- **It uses a push relay.** On **Add a device**, under **Notifications on iPhone**, the owner gives the relay's address beside **Push relay** and chooses **Use**. **Stop** turns it off. Phones that connect after that use it. The relay never reads a notification:
  - Each phone seals its push token for the relay, so your instance never has it.
  - Your instance seals each notification for the phone it goes to, so the relay and Apple see only that something is new.
  - Your instance signs every request, and the relay takes only so many an hour from each instance.
  - The relay keeps nothing but that count. Anyone can run one: it's `loom-push`, in the [push](../../push/README.md) folder.
- **Neither**, which is how every instance starts. The phone gets notifications only while the app is open.
