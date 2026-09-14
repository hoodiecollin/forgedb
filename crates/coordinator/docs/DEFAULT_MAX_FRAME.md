Default maximum frame payload length: 16 MiB (#145). Configurable per
coordinator via [`server::CoordConfig::max_frame`]; the client-side decode of
(small) server responses keeps this default.
