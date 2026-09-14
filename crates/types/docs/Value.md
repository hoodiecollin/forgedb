A primitive value of any ForgeDB schema type, tagged with its type.

Serde uses the adjacently tagged form, so the JSON for `Value::I32(42)` is
`{"type":"i32","value":42}`, with the tag being the name [`Self::type_name`] returns.
`From` conversions exist from every wrapped type, plus `&str`.
