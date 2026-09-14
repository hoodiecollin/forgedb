Default maximum frame payload length, 16 MiB.

[`decode_msg`] applies it unconditionally; the coordinator applies [`server::CoordConfig::max_frame`], which defaults to it, to every frame it reads from a client.
