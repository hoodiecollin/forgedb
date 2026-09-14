Row count as recorded at the last manifest write.

It can lag the files, since rows appended after the write are not counted here. A reader that needs the live committed count derives it from the length of the [`Manifest::row_anchor`] file instead.
