A complete WAL entry: a model-name routing tag plus an operation.

The `model_name` field is an opaque string stored verbatim in the entry
header. The WAL never interprets it. Generated code uses it to route
replayed entries back to the correct model; ForgeDB itself never branches
on it.
