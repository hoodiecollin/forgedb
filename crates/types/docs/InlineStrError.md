Why a `&str` did not fit an [`InlineStr`]: it was longer than the capacity.

Carries both lengths so a caller can report the bound without re-deriving it.
