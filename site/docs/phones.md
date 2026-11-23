# Phones

> Loom isn't on TestFlight or the App Store yet. To use it on your iPhone now, [build it yourself](iphone.md).

Add your phone from a device that's already signed in: **Add a phone**, in the server popup (click the server's name at the bottom of the sidebar), shows a QR code, and the loom app on the phone scans it. The phone gets a sign-in of its own, which you can take away from the same page.

## An instance on a server

The phone reaches it at the server's address, over TLS, as the desktop app does.

## An instance in the desktop app

The instance listens only on your computer until you turn on **Phones can reach this computer** on the **Add a phone** page. Then it also listens on your network, on port 47420 (or another, if that one's taken), and takes only an encrypted connection there: a [Noise](https://noiseprotocol.org) handshake against the instance's key, which is in the QR code. Nothing reaches the instance on that port without it, and everything after it is encrypted end to end. Windows asks once whether loom may use the network; allow it on private networks.

- **At home**, the phone reaches the computer over the Wi-Fi.
- **Away from home**, it goes through [Tailscale](tailscale.md), if you use it: the QR code holds the computer's Tailscale address too, and the phone tries the Wi-Fi first and Tailscale next.

Turn the switch off to stop phones reaching it again.

## Notifications

While the loom app is open, it shows notifications as they come. Notifications while it's closed aren't in the app yet. For those, the instance needs Apple's push service, and Apple takes only pushes signed with the key of the account that built the app. So an instance does one of these:

- **It has that key itself.** `loom push key AuthKey_….p8 KEY_ID TEAM_ID BUNDLE_ID` sets it up, if you build the app with your own Apple account.
- **It uses a push relay.** On **Add a phone**, under **Notifications on iPhone**, the owner gives the relay's address beside **Push relay** and chooses **Use**. **Stop** turns it off. Phones that connect after that use it. The relay never reads a notification:
  - Each phone seals its push token for the relay, so your instance never has it.
  - Your instance seals each notification for the phone it goes to, so the relay and Apple see only that something is new.
  - Your instance signs every request, and the relay takes only so many an hour from each instance.
  - The relay keeps nothing but that count. Anyone can run one: it's `loom-push`, in the [push](../../push/README.md) folder.
- **Neither**, which is how every instance starts. The phone gets notifications only while the app is open.
