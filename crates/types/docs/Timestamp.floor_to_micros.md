Floors the value to a multiple of `quantum_us` (res 5): a user-supplied
value finer than a field's declared precision is *recorded* coarsely
rather than refused, because rejecting a valid measurement for being too
precise only exports the flooring to every client.

**Floor, never truncate toward zero.** Truncation rounds a pre-epoch value
*forward* in time (`-0.5s` → `0s`), which breaks monotonicity across 1970
— two instants an hour apart could quantize to the same value from
opposite sides, or swap order.

A `quantum_us` of `0` or less is the identity (there is nothing to floor
to), so this never divides by zero on a caller's behalf.
