//! Python REST client SDK generator (`generate python --sdk`, #118).
//!
//! Emits a standalone, **dependency-free** (stdlib `urllib` + `dataclasses`)
//! Python client module for the generated REST API — the Python sibling of the
//! TypeScript SDK (`typescript.rs`). It produces:
//!
//! - a `@dataclass` for each model, a `<Model>Create` input, and one dataclass per
//!   `@projection` (#113);
//! - a `str`-valued `Enum` for each `#enum` (serialized as the variant-name
//!   string, matching the wire);
//! - a shared `ForgeDbError` / `ListResult` / `ListOptions` surface;
//! - a `ForgeDbClient` with full CRUD (get / list / create / update / delete) plus
//!   per-projection read methods, faithfully wrapping the REST endpoint's real
//!   response shapes and status codes.
//!
//! Distinct from the Python *runtime* binding (`pyo3.rs`, #51) — this is a network
//! REST client (no FFI). Like the TS SDK it is a transport client over the
//! already-generated, schema-tailored REST surface, interpreting no schema at
//! runtime — class-2 access glue per `CLAUDE.md`.

use crate::rust::RustGenerator;
use crate::{GeneratedCode, Result};
use forgedb_parser::{Field, FieldType, RelationType, Schema};

/// Python REST client SDK generator.
pub struct PythonSdkGenerator;

impl PythonSdkGenerator {
    /// Generate the Python SDK (`forgedb_client.py`) for a schema.
    pub fn generate(schema: &Schema) -> Result<GeneratedCode> {
        Ok(GeneratedCode {
            code: Self::generate_code(schema),
            description: format!("Python REST client SDK ({} models)", schema.models.len()),
        })
    }

    /// A `pyproject.toml` so the single-module SDK installs with any PEP 517
    /// frontend (`pip install .`). Written next to `forgedb_client.py` only when
    /// absent, so user edits survive regeneration. Dependency-free — stdlib only.
    pub fn pyproject_scaffold() -> &'static str {
        "[build-system]\n\
         requires = [\"setuptools>=61\"]\n\
         build-backend = \"setuptools.build_meta\"\n\
         \n\
         [project]\n\
         name = \"forgedb-client\"\n\
         version = \"0.1.0\"\n\
         requires-python = \">=3.8\"\n\
         description = \"Generated ForgeDB Python REST client SDK\"\n\
         \n\
         [tool.setuptools]\n\
         py-modules = [\"forgedb_client\"]\n"
    }

    fn generate_code(schema: &Schema) -> String {
        let mut c = String::new();
        c.push_str(FILE_HEADER);

        // Enums — a str-valued Enum per declared enum.
        for en in &schema.enums {
            c.push_str(&format!(
                "class {name}(str, Enum):\n\
                 \x20   \"\"\"{name} — serialized as its variant-name string.\"\"\"\n",
                name = en.name
            ));
            for v in &en.variants {
                c.push_str(&format!("    {v} = \"{v}\"\n"));
            }
            c.push_str("\n\n");
        }

        // Models + create-input + projection dataclasses.
        for model in &schema.models {
            Self::push_dataclass(&mut c, &model.name, &model.fields.iter().collect::<Vec<_>>(),
                &format!("{} — mirrors the wire shape of the generated model.", model.name));
            Self::push_dataclass(&mut c, &format!("{}Create", model.name),
                &RustGenerator::creatable_fields(model),
                &format!("Input to create_{} — a {} without server-synthesized (+uuid/+timestamp) fields.",
                    RustGenerator::to_snake_case(&model.name), model.name));
            for proj in &model.projections {
                let ty = format!("{}{}", model.name, RustGenerator::projection_pascal(&proj.name));
                Self::push_dataclass(&mut c, &ty, &RustGenerator::projected_field_set(model, proj),
                    &format!("Projection `{}` of {} — PK + declared columns only.", proj.name, model.name));
            }
        }

        c.push_str(SHARED_TYPES);
        Self::push_client(&mut c, schema);
        c
    }

    /// Emit one `@dataclass`. Fields are partitioned required-first then
    /// defaulted (nullable/opaque, `= None`) so the class body never puts a
    /// defaulted field before a required one (a Python `dataclass` error). JSON
    /// (de)serialization is by field name, so the reorder is behavior-neutral.
    fn push_dataclass(c: &mut String, name: &str, fields: &[&Field], doc: &str) {
        c.push_str(&format!("@dataclass\nclass {name}:\n    \"\"\"{doc}\"\"\"\n"));
        let mut required: Vec<&Field> = Vec::new();
        let mut optional: Vec<&Field> = Vec::new();
        for &f in fields {
            if Self::is_defaulted(f) {
                optional.push(f);
            } else {
                required.push(f);
            }
        }
        if required.is_empty() && optional.is_empty() {
            c.push_str("    pass\n\n\n");
            return;
        }
        for f in required {
            c.push_str(&format!("    {}: {}\n", f.name, Self::py_type(f)));
        }
        for f in optional {
            c.push_str(&format!("    {}: {} = None\n", f.name, Self::py_type(f)));
        }
        c.push_str("\n\n");
    }

    fn push_client(c: &mut String, schema: &Schema) {
        c.push_str(CLIENT_HEADER);
        for model in &schema.models {
            let name = &model.name;
            let snake = RustGenerator::to_snake_case(name);
            let kebab = Self::to_kebab_case(name);

            // get
            c.push_str(&format!(
                "    def get_{snake}(self, id: str) -> Optional[{name}]:\n\
                 \x20       \"\"\"Get a {name} by id, or None if it does not exist.\"\"\"\n\
                 \x20       status, raw = self._request(\"GET\", f\"/api/{kebab}/{{_seg(id)}}\")\n\
                 \x20       if status == 404:\n\
                 \x20           return None\n\
                 \x20       self._assert_ok(status, raw)\n\
                 \x20       return {name}(**json.loads(raw))\n\n"
            ));

            // list
            c.push_str(&format!(
                "    def list_{snake}(self, options: Optional[ListOptions] = None) -> ListResult:\n\
                 \x20       \"\"\"List {name} rows with optional pagination, sort, and exact-match filters.\"\"\"\n\
                 \x20       status, raw = self._request(\"GET\", \"/api/{kebab}\", query=self._list_query(options))\n\
                 \x20       self._assert_ok(status, raw)\n\
                 \x20       payload = json.loads(raw)\n\
                 \x20       return ListResult(\n\
                 \x20           data=[{name}(**row) for row in payload[\"data\"]],\n\
                 \x20           total=payload[\"total\"], limit=payload[\"limit\"], offset=payload[\"offset\"],\n\
                 \x20       )\n\n"
            ));

            // create
            c.push_str(&format!(
                "    def create_{snake}(self, data: {name}Create) -> str:\n\
                 \x20       \"\"\"Create a {name}; return the new id. Raises ForgeDbError on 422/409.\"\"\"\n\
                 \x20       status, raw = self._request(\"POST\", \"/api/{kebab}\", body=asdict(data))\n\
                 \x20       self._assert_ok(status, raw)\n\
                 \x20       return json.loads(raw)[\"id\"]\n\n"
            ));

            // update
            c.push_str(&format!(
                "    def update_{snake}(self, id: str, data: {name}) -> bool:\n\
                 \x20       \"\"\"Replace a {name} by id (whole-record PUT). False if absent.\"\"\"\n\
                 \x20       status, raw = self._request(\"PUT\", f\"/api/{kebab}/{{_seg(id)}}\", body=asdict(data))\n\
                 \x20       if status == 404:\n\
                 \x20           return False\n\
                 \x20       self._assert_ok(status, raw)\n\
                 \x20       return True\n\n"
            ));

            // delete
            c.push_str(&format!(
                "    def delete_{snake}(self, id: str) -> bool:\n\
                 \x20       \"\"\"Delete a {name} by id. True if deleted, False if absent; raises (409) on restrict.\"\"\"\n\
                 \x20       status, raw = self._request(\"DELETE\", f\"/api/{kebab}/{{_seg(id)}}\")\n\
                 \x20       if status == 404:\n\
                 \x20           return False\n\
                 \x20       self._assert_ok(status, raw)\n\
                 \x20       return True\n\n"
            ));

            // projections
            for proj in &model.projections {
                let ty = format!("{}{}", name, RustGenerator::projection_pascal(&proj.name));
                let proj_snake = RustGenerator::to_snake_case(&proj.name);
                let pname = &proj.name;
                c.push_str(&format!(
                    "    def get_{snake}_{proj_snake}(self, id: str) -> Optional[{ty}]:\n\
                     \x20       \"\"\"Get the `{pname}` projection of a {name} by id (None if absent).\"\"\"\n\
                     \x20       status, raw = self._request(\"GET\", f\"/api/{kebab}/{{_seg(id)}}\", query={{\"projection\": \"{pname}\"}})\n\
                     \x20       if status == 404:\n\
                     \x20           return None\n\
                     \x20       self._assert_ok(status, raw)\n\
                     \x20       return {ty}(**json.loads(raw))\n\n"
                ));
                c.push_str(&format!(
                    "    def list_{snake}_{proj_snake}(self, options: Optional[ListOptions] = None) -> ListResult:\n\
                     \x20       \"\"\"List {name} rows as the `{pname}` projection (PK + declared columns).\"\"\"\n\
                     \x20       query = self._list_query(options)\n\
                     \x20       query[\"projection\"] = \"{pname}\"\n\
                     \x20       status, raw = self._request(\"GET\", \"/api/{kebab}\", query=query)\n\
                     \x20       self._assert_ok(status, raw)\n\
                     \x20       payload = json.loads(raw)\n\
                     \x20       return ListResult(\n\
                     \x20           data=[{ty}(**row) for row in payload[\"data\"]],\n\
                     \x20           total=payload[\"total\"], limit=payload[\"limit\"], offset=payload[\"offset\"],\n\
                     \x20       )\n\n"
                ));
            }
        }
    }

    /// Whether a field is emitted with a `= None` default (and so goes in the
    /// trailing partition): nullable fields and the opaque bucket.
    fn is_defaulted(field: &Field) -> bool {
        let (opaque, _) = Self::base_type(&field.field_type);
        opaque || field.is_nullable()
    }

    /// Map a schema field to its Python annotation. Scalars map precisely; a FK
    /// reference is the uuid it stores (`str`); the opaque bucket (`json`,
    /// `char(N)`, fixed arrays, inline structs, and virtual one-to-many / M2M
    /// relations, which the server serializes as `null`) maps to `Any` — the
    /// honest analogue of the TS SDK's `unknown`/`any`.
    fn py_type(field: &Field) -> String {
        let (opaque, base) = Self::base_type(&field.field_type);
        if opaque {
            "Any".to_string()
        } else if field.is_nullable() {
            format!("Optional[{base}]")
        } else {
            base
        }
    }

    fn base_type(ft: &FieldType) -> (bool, String) {
        match ft {
            FieldType::U32 | FieldType::U64 | FieldType::I32 | FieldType::I64 => (false, "int".into()),
            FieldType::Timestamp => (false, "int".into()),
            FieldType::F64 => (false, "float".into()),
            FieldType::Bool => (false, "bool".into()),
            // #238: an inline `string(N)` is a string on the wire.
            FieldType::String | FieldType::StringN { .. } | FieldType::Uuid => {
                (false, "str".into())
            }
            FieldType::Decimal => (false, "str".into()),
            FieldType::Enum(name) => (false, name.clone()),
            FieldType::Relation(RelationType::RequiredReference(_))
            | FieldType::Relation(RelationType::OptionalReference(_)) => (false, "str".into()),
            FieldType::Nullable(inner) => Self::base_type(inner),
            _ => (true, "Any".into()),
        }
    }

    /// PascalCase model name → kebab-case URL segment (matches the Rust router).
    fn to_kebab_case(s: &str) -> String {
        let mut result = String::new();
        for (i, ch) in s.chars().enumerate() {
            if ch.is_uppercase() && i > 0 {
                result.push('-');
            }
            result.push(ch.to_ascii_lowercase());
        }
        result
    }
}

const FILE_HEADER: &str = r#"# Generated by ForgeDB — DO NOT EDIT
#
# Python REST client SDK for a ForgeDB app. A transport client over the generated
# REST API; it interprets no schema at runtime. Standard library only.
from __future__ import annotations

import json
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import asdict, dataclass
from enum import Enum
from typing import Any, Generic, List, Optional, TypeVar

"#;

/// The schema-independent SDK support types.
const SHARED_TYPES: &str = r#"class ForgeDbError(Exception):
    """Raised on any non-2xx response (except a get/update/delete 404, surfaced as
    None/False) and on transport failures (status == 0). Carries the HTTP status
    and the parsed {error} body when present."""

    def __init__(self, status: int, message: str, body: Any = None) -> None:
        super().__init__(f"ForgeDB error (status {status}): {message}")
        self.status = status
        self.message = message
        self.body = body


T = TypeVar("T")


@dataclass
class ListResult(Generic[T]):
    """A page of list results — mirrors the REST list response envelope."""

    data: List[T]
    total: int
    limit: int
    offset: int


@dataclass
class ListOptions:
    """Options for a list query. `filter` holds exact-match ?field=value pairs
    matched by the generated per-model filter server-side."""

    limit: Optional[int] = None
    offset: Optional[int] = None
    sort: Optional[str] = None
    order: Optional[str] = None  # "asc" | "desc"
    filter: Optional[dict] = None


def _seg(value: object) -> str:
    """Percent-encode one URL path segment."""
    return urllib.parse.quote(str(value), safe="")


"#;

/// The client class preamble (constructor + shared helpers); the per-model
/// methods are appended after it.
const CLIENT_HEADER: &str = r#"class ForgeDbClient:
    """A typed client for a ForgeDB app's REST API."""

    def __init__(self, base_url: str = "http://localhost:3000") -> None:
        # Trim a trailing slash so path concatenation is unambiguous.
        self.base_url = base_url.rstrip("/")

    def _request(self, method: str, path: str, query=None, body=None):
        """Perform a request; return (status, raw_bytes). Raises ForgeDbError on a
        transport failure (never on an HTTP status — the caller decides)."""
        url = self.base_url + path
        if query:
            qs = urllib.parse.urlencode(query)
            if qs:
                url += "?" + qs
        data = None
        headers = {}
        if body is not None:
            data = json.dumps(body).encode("utf-8")
            headers["Content-Type"] = "application/json"
        req = urllib.request.Request(url, data=data, headers=headers, method=method)
        try:
            with urllib.request.urlopen(req) as resp:
                return resp.status, resp.read()
        except urllib.error.HTTPError as e:
            return e.code, e.read()
        except urllib.error.URLError as e:
            raise ForgeDbError(0, str(e.reason)) from e

    @staticmethod
    def _assert_ok(status: int, raw: bytes) -> None:
        if 200 <= status < 300:
            return
        body = None
        message = f"HTTP {status}"
        try:
            body = json.loads(raw)
            if isinstance(body, dict) and "error" in body:
                message = str(body["error"])
        except Exception:
            pass
        raise ForgeDbError(status, message, body)

    @staticmethod
    def _list_query(options: Optional[ListOptions]) -> dict:
        q: dict = {}
        if options is None:
            return q
        if options.limit is not None:
            q["limit"] = options.limit
        if options.offset is not None:
            q["offset"] = options.offset
        if options.sort is not None:
            q["sort"] = options.sort
        if options.order is not None:
            q["order"] = options.order
        if options.filter:
            for k, v in options.filter.items():
                q[k] = v
        return q

"#;
