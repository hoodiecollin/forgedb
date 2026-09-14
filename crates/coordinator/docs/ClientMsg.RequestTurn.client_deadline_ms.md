The client's own I/O deadline, in milliseconds (#274).

The coordinator clamps its grant wait to fit inside this, so a `Busy`
reply always reaches the client before it stops reading.  Without it
neither side could see the other's deadline and the operator was
silently responsible for keeping two numbers — `--turn-timeout` and a
constant compiled into the client — in a relationship nothing checked.

`0` (the `serde` default, i.e. a **pre-#274 client**, which omits the
field entirely) means "unknown — assume the legacy 35s".  Deliberately
an additive field rather than a handshake message: the protocol is
internally-tagged JSON with no version field (#277), so an unknown
*variant* breaks whichever peer ships second, while an unknown *field*
is ignored in both directions.
