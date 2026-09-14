Direction of a sort; `Default` is [`Self::Asc`].

[`Self::from_str`] parses the `order` query parameter leniently. The derived `Deserialize` impl is stricter: it accepts exactly the lowercase names `asc` and `desc`.
