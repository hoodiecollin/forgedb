The window before this one, or `None` when `offset` is already `0`.

The new `offset` is `offset - limit`, floored at `0`, so a page that started mid-way through a previous window steps back to the list start.
