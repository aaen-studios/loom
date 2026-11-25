# Tailscale

[Tailscale](https://tailscale.com) puts your devices on one private network, wherever they are. Loom passes no traffic through servers of its own, so Tailscale is how a phone away from home reaches a PC at home, and how you reach a server without a domain.

## A server without a domain

Install Tailscale on the server and sign it in (`tailscale up`). Then `loom install` without `--domain` serves loom on your tailnet with `tailscale serve`, over HTTPS, at the server's `….ts.net` name, and prints that address.

What Tailscale serves already stays as it is. When something else has `/` or `/api` on port 443, loom takes the next free HTTPS port (8443, then 10000 and on), and its address has that port. `--https-port N` picks the port, and `--no-tailscale` leaves Tailscale alone. Install Tailscale on your PC and phone too, signed in to the same account.

If Tailscale isn't there when you install, loom listens only on the server itself. Install Tailscale and run `loom install` again.

## A PC at home

For your phone ([Adding a device](phones.md)): install Tailscale on the PC and the phone, signed in to the same account, and let other devices reach loom on the PC. The QR code holds the PC's Tailscale address beside its Wi-Fi one, so the phone gets through from anywhere. Loom encrypts that connection itself, so it doesn't depend on Tailscale's encryption.

Any other VPN that puts your phone and PC on one network works the same way.
