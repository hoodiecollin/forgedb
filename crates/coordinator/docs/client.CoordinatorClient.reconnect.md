Replace the socket, discarding any reply stranded on the old one, and clear
the poison flag.

Takes `&self` so it works through the `Arc` the generated
`CoordinatedDatabase` holds — recovery policy stays in generated code,
beside the `Busy` budget and retry limit that already live there, rather
than being decided by this substrate.

On failure the flag is **left set**: the stream is still the old,
desynchronized one.
