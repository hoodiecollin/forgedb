A single positional change signal: a change of `kind` to model `model`, with
the row it concerns at `row_index`.

Field-blind by construction: it carries a generated `&'static str` model
name, a row position and a [`ChangeKind`], nothing else. A subscriber that
wants the record materializes it in generated code from `row_index`. `Copy`.
