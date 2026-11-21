# Phones

> The loom app for iPhone is still being built. This page says how it will work.

Add your phone from a device that's already signed in: **Add a phone** shows a QR code, and the loom app on the phone scans it. The phone gets a sign-in of its own, which you can take away from the same page.

## An instance on a server

The phone reaches it at the server's address, over TLS, as the desktop app does.

## An instance in the desktop app

The instance listens only on your computer until you choose **Let phones reach this computer** on the **Add a phone** page. Then it also listens on your network, on port 47420 (or another, if that one's taken), and takes only an encrypted connection there: a [Noise](https://noiseprotocol.org) handshake against the instance's key, which is in the QR code. Nothing reaches the instance on that port without it, and everything after it is encrypted end to end. Windows asks once whether loom may use the network; allow it on private networks.

- **At home**, the phone reaches the computer over the Wi-Fi.
- **Away from home**, it goes through [Tailscale](tailscale.md), if you use it: the QR code holds the computer's Tailscale address too, and the phone tries the Wi-Fi first and Tailscale next. (Putting the addresses in the QR code is still to come.)

**Stop phones reaching this computer** turns it off again.
