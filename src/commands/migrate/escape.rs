use crate::{Result, error::CliError};
use forgedb_migrations::{EscapeLanguage, HopBodyClass, SchemaChange, checksum};
use std::path::{Path, PathBuf};

pub fn language_for(internal_targets: &[String]) -> EscapeLanguage {
    let has = |t: &str| internal_targets.iter().any(|x| x == t);
    if has("typescript") || has("napi") {
        EscapeLanguage::TypeScript
    } else if has("pyo3") || has("python-sdk") {
        EscapeLanguage::Python
    } else {
        EscapeLanguage::Rust
    }
}

pub fn write_scaffold(
    migrations_dir: &Path,
    migration_id: &str,
    lang: EscapeLanguage,
    changes: &[SchemaChange],
    dest_schema: &forgedb_parser::Schema,
    versions: (u32, u32),
) -> Result<(PathBuf, String)> {
    let dir = forgedb_migrations::migration_body_dir(migrations_dir, migration_id);
    std::fs::create_dir_all(&dir)
        .map_err(|e| CliError::Migration(format!("could not create {}: {e}", dir.display())))?;
    let path = dir.join(lang.transform_file());
    let body = scaffold(lang, changes, dest_schema, versions);

    if !path.exists() {
        std::fs::write(&path, &body).map_err(|e| {
            CliError::Migration(format!("could not write {}: {e}", path.display()))
        })?;
    }
    Ok((path, checksum::compute(body.as_bytes())))
}

pub fn write_support_files(
    migrations_dir: &Path,
    migration_id: &str,
    lang: EscapeLanguage,
    versions: &[(u32, &forgedb_parser::Schema)],
) -> Result<Vec<PathBuf>> {
    if lang == EscapeLanguage::Rust {
        return Ok(Vec::new());
    }
    let dir = forgedb_migrations::migration_body_dir(migrations_dir, migration_id);
    std::fs::create_dir_all(&dir)
        .map_err(|e| CliError::Migration(format!("could not create {}: {e}", dir.display())))?;

    let mut written = Vec::new();
    let (host_name, host_src) = match lang {
        EscapeLanguage::TypeScript => forgedb_codegen::typescript_host(),
        EscapeLanguage::Python => forgedb_codegen::python_host(),
        EscapeLanguage::Rust => unreachable!("returned above"),
    };
    let mut emit = |name: String, src: String| -> Result<()> {
        let p = dir.join(&name);
        std::fs::write(&p, src)
            .map_err(|e| CliError::Migration(format!("could not write {}: {e}", p.display())))?;
        written.push(p);
        Ok(())
    };
    emit(host_name, host_src)?;
    for (v, schema) in versions {
        let (name, src) = match lang {
            EscapeLanguage::TypeScript => forgedb_codegen::typescript_types(schema, *v),
            EscapeLanguage::Python => forgedb_codegen::python_types(schema, *v),
            EscapeLanguage::Rust => unreachable!("returned above"),
        };
        emit(name, src)?;
    }
    Ok(written)
}

fn residue_by_model(changes: &[SchemaChange]) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut by_model: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for c in changes {
        if c.hop_body_class() != HopBodyClass::Authored {
            continue;
        }
        by_model
            .entry(c.target_model().to_string())
            .or_default()
            .push(c.description());
    }
    by_model
}

fn scaffold(
    lang: EscapeLanguage,
    changes: &[SchemaChange],
    dest_schema: &forgedb_parser::Schema,
    versions: (u32, u32),
) -> String {
    match lang {
        EscapeLanguage::Rust => rust_scaffold(changes),
        EscapeLanguage::TypeScript => typescript_scaffold(changes, dest_schema, versions),
        EscapeLanguage::Python => python_scaffold(changes, dest_schema, versions),
    }
}

fn header(comment: &str, changes: &[SchemaChange], lang: EscapeLanguage) -> String {
    let mut s = String::new();
    let l = |s: &mut String, line: &str| {
        s.push_str(comment);
        if !line.is_empty() {
            s.push(' ');
            s.push_str(line);
        }
        s.push('\n');
    };
    l(&mut s, "Authored transform for this migration.");
    l(&mut s, "");
    l(
        &mut s,
        "ForgeDB could not PROVE a new-row value for the change(s) below from the",
    );
    l(&mut s, "schema diff alone, so you are writing it. This function is called for");
    l(&mut s, "EVERY row of EVERY model in this hop, AFTER the automatic (additive /");
    l(&mut s, "rename / drop) field ops have been applied. Return the row reshaped");
    l(&mut s, "into the NEXT version; a model you do not need to touch, return as-is.");
    l(&mut s, "");
    for (model, residue) in residue_by_model(changes) {
        l(&mut s, &format!("{model}:"));
        for r in residue {
            l(&mut s, &format!("  - {r}"));
        }
    }
    if !matches!(lang, EscapeLanguage::Rust) {
        l(&mut s, "");
        l(
            &mut s,
            "This file is YOURS — ForgeDB never rewrites it. The v*.ts / v*.py type",
        );
        l(&mut s, "modules beside it are ForgeDB's and are regenerated every time.");
    }
    s
}

fn rust_scaffold(changes: &[SchemaChange]) -> String {
    let mut s = header("//", changes, EscapeLanguage::Rust);
    s.push('\n');
    s.push_str(
        "pub fn authored_transform(model: &str, mut row: serde_json::Value) -> serde_json::Value {\n",
    );
    s.push_str("    match model {\n");
    for (model, _) in residue_by_model(changes) {
        s.push_str(&format!("        {model:?} => {{\n"));
        s.push_str("            // e.g. re-encode a changed field:\n");
        s.push_str(
            "            // if let Some(v) = row.get(\"<field>\").and_then(|x| x.as_u64()) {\n\
             \x20           //     row[\"<field>\"] = serde_json::Value::String(v.to_string());\n\
             \x20           // }\n",
        );
        s.push_str("            row\n        }\n");
    }
    s.push_str("        _ => row,\n    }\n}\n");
    s
}

fn typescript_scaffold(
    changes: &[SchemaChange],
    _dest: &forgedb_parser::Schema,
    (from, to): (u32, u32),
) -> String {
    let mut s = header("//", changes, EscapeLanguage::TypeScript);
    s.push('\n');
    s.push_str("import { runTransform, type Row } from \"./host\";\n");
    s.push_str("// The typed models of the version you are reading FROM and writing TO.\n");
    s.push_str("// ForgeDB regenerates these from the committed schema snapshots.\n");
    s.push_str(&format!("import type * as From from \"./v{from}\";\n"));
    s.push_str(&format!("import type * as To from \"./v{to}\";\n\n"));
    s.push_str("export function transform(model: string, row: Row): Row {\n");
    s.push_str("  switch (model) {\n");
    for (model, _) in residue_by_model(changes) {
        s.push_str(&format!("    case {model:?}: {{\n"));
        s.push_str(&format!(
            "      const from = row as unknown as From.{model};\n"
        ));
        s.push_str(&format!(
            "      // const to: To.{model} = {{ ...from, /* your change here */ }};\n"
        ));
        s.push_str("      // return to as unknown as Row;\n");
        s.push_str("      return from as unknown as Row;\n");
        s.push_str("    }\n");
    }
    s.push_str("    default:\n      return row;\n  }\n}\n\n");
    s.push_str("// ForgeDB spawns this file and speaks one JSON object per line on\n");
    s.push_str("// stdin/stdout; you never edit below this line.\n");
    s.push_str("runTransform(transform);\n");
    s
}

fn python_scaffold(
    changes: &[SchemaChange],
    _dest: &forgedb_parser::Schema,
    (from, to): (u32, u32),
) -> String {
    let mut s = header("#", changes, EscapeLanguage::Python);
    s.push('\n');
    s.push_str("from host import run_transform, Row\n");
    s.push_str("# The typed models of the version you are reading FROM and writing TO.\n");
    s.push_str("# ForgeDB regenerates these from the committed schema snapshots.\n");
    s.push_str(&format!("import v{from}\nimport v{to}\n\n\n"));
    s.push_str("def transform(model: str, row: Row) -> Row:\n");
    for (model, _) in residue_by_model(changes) {
        s.push_str(&format!("    if model == {model:?}:\n"));
        s.push_str(&format!(
            "        source: v{from}.{model} = row  # type: ignore[assignment]\n"
        ));
        s.push_str(&format!(
            "        # result: v{to}.{model} = {{**source, \"...\": ...}}\n"
        ));
        s.push_str("        # return result  # type: ignore[return-value]\n");
        s.push_str("        return row\n");
    }
    s.push_str("    return row\n\n\n");
    s.push_str("# ForgeDB spawns this file and speaks one JSON object per line on\n");
    s.push_str("# stdin/stdout; you never edit below this line.\n");
    s.push_str("if __name__ == \"__main__\":\n    run_transform(transform)\n");
    s
}
