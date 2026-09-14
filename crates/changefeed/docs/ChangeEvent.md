A single positional change signal: model `model` gained a row at `row_index`.

Field-blind by construction — it carries the model's name (a generated
`&'static str`) and the append position, nothing else. A subscriber that wants
the actual record materializes it in generated code from `row_index`.
