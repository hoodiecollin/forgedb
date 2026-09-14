A `limit`/`offset` window over a list of rows.

[`Self::new`] and [`Self::from_params`] clamp `limit` to `1..=`[`MAX_LIMIT`] and leave `offset` unbounded; `Default` is [`DEFAULT_LIMIT`] and offset `0`. The derived `Deserialize` impl applies the same defaults for a missing `limit` or `offset` but does **not** clamp. [`Self::apply`] slices a page out of a slice; [`Self::next_page`], [`Self::prev_page`] and [`Self::has_next`] navigate between pages.
