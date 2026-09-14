Physical-layout metadata for one model directory, persisted as JSON (`manifest.json`) beside the column files.

It records what a schema-blind reader needs to bound every file in the directory: the columns and their files, the file whose length counts committed rows, the app's schema serial and the engine's byte-format generation. It carries no schema semantics. Generated code writes it on open and at checkpoints through [`Manifest::save_to`]; backup and inspection tooling read it through [`Manifest::load_from`].

Every field added after the first on-disk format is `#[serde(default)]`, so an older manifest still deserializes, and unknown keys are ignored.
