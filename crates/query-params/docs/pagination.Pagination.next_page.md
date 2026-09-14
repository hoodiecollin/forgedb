The window after this one: same `limit`, `offset` advanced by `limit`.

Unlike [`Self::end`] the addition is not saturating. It does not check whether such a page exists; pair it with [`Self::has_next`].
