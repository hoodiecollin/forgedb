use crate::rust::RustGenerator;
use crate::{GeneratedCode, Result};
use forgedb_parser::{Field, FieldType, RelationType, Schema};

pub struct PythonSdkGenerator;

impl PythonSdkGenerator {
    pub fn generate(schema: &Schema) -> Result<GeneratedCode> {
        Ok(GeneratedCode {
            code: Self::generate_code(schema),
            description: format!("Python REST client SDK ({} models)", schema.models.len()),
        })
    }

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

        for model in &schema.models {
            Self::push_dataclass(schema, &mut c, &model.name, &model.fields.iter().collect::<Vec<_>>(),
                &format!("{} — mirrors the wire shape of the generated model.", model.name));
            Self::push_dataclass(schema, &mut c, &format!("{}Create", model.name),
                &RustGenerator::creatable_fields(model),
                &format!("Input to create_{} — a {} without server-synthesized (+uuid/+timestamp) fields.",
                    RustGenerator::to_snake_case(&model.name), model.name));
            for proj in &model.projections {
                let ty = format!("{}{}", model.name, RustGenerator::projection_pascal(&proj.name));
                Self::push_dataclass(schema, &mut c, &ty, &RustGenerator::projected_field_set(model, proj),
                    &format!("Projection `{}` of {} — PK + declared columns only.", proj.name, model.name));
            }
        }

        c.push_str(SHARED_TYPES);
        Self::push_client(&mut c, schema);
        c
    }

    fn push_dataclass(schema: &Schema, c: &mut String, name: &str, fields: &[&Field], doc: &str) {
        c.push_str(&format!("@dataclass\nclass {name}:\n    \"\"\"{doc}\"\"\"\n"));
        let mut required: Vec<&Field> = Vec::new();
        let mut optional: Vec<&Field> = Vec::new();
        for &f in fields {
            if Self::is_defaulted(schema, f) {
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
            c.push_str(&format!("    {}: {}\n", f.name, Self::py_type(schema, f)));
        }
        for f in optional {
            c.push_str(&format!("    {}: {} = None\n", f.name, Self::py_type(schema, f)));
        }
        c.push_str("\n\n");
    }

    fn push_client(c: &mut String, schema: &Schema) {
        c.push_str(CLIENT_HEADER);
        for model in &schema.models {
            let name = &model.name;
            let snake = RustGenerator::to_snake_case(name);
            let kebab = Self::to_kebab_case(name);

            c.push_str(&format!(
                "    def get_{snake}(self, id: str) -> Optional[{name}]:\n\
                 \x20       \"\"\"Get a {name} by id, or None if it does not exist.\"\"\"\n\
                 \x20       status, raw = self._request(\"GET\", f\"/api/{kebab}/{{_seg(id)}}\")\n\
                 \x20       if status == 404:\n\
                 \x20           return None\n\
                 \x20       self._assert_ok(status, raw)\n\
                 \x20       return {name}(**json.loads(raw))\n\n"
            ));

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

            c.push_str(&format!(
                "    def create_{snake}(self, data: {name}Create) -> str:\n\
                 \x20       \"\"\"Create a {name}; return the new id. Raises ForgeDbError on 422/409.\"\"\"\n\
                 \x20       status, raw = self._request(\"POST\", \"/api/{kebab}\", body=asdict(data))\n\
                 \x20       self._assert_ok(status, raw)\n\
                 \x20       return json.loads(raw)[\"id\"]\n\n"
            ));

            c.push_str(&format!(
                "    def update_{snake}(self, id: str, data: {name}) -> bool:\n\
                 \x20       \"\"\"Replace a {name} by id (whole-record PUT). False if absent.\"\"\"\n\
                 \x20       status, raw = self._request(\"PUT\", f\"/api/{kebab}/{{_seg(id)}}\", body=asdict(data))\n\
                 \x20       if status == 404:\n\
                 \x20           return False\n\
                 \x20       self._assert_ok(status, raw)\n\
                 \x20       return True\n\n"
            ));

            c.push_str(&format!(
                "    def delete_{snake}(self, id: str) -> bool:\n\
                 \x20       \"\"\"Delete a {name} by id. True if deleted, False if absent; raises (409) on restrict.\"\"\"\n\
                 \x20       status, raw = self._request(\"DELETE\", f\"/api/{kebab}/{{_seg(id)}}\")\n\
                 \x20       if status == 404:\n\
                 \x20           return False\n\
                 \x20       self._assert_ok(status, raw)\n\
                 \x20       return True\n\n"
            ));

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

    fn is_defaulted(schema: &Schema, field: &Field) -> bool {
        let (opaque, _) = Self::base_type(schema, &field.field_type);
        opaque || field.is_nullable()
    }

    fn py_type(schema: &Schema, field: &Field) -> String {
        let (opaque, base) = Self::base_type(schema, &field.field_type);
        if opaque {
            "Any".to_string()
        } else if field.is_nullable() {
            format!("Optional[{base}]")
        } else {
            base
        }
    }

    fn base_type(schema: &Schema, ft: &FieldType) -> (bool, String) {
        match ft {
            FieldType::U32 | FieldType::U64 | FieldType::I32 | FieldType::I64 => (false, "int".into()),
            FieldType::Timestamp(_) => (false, "str".into()),
            FieldType::F64 => (false, "float".into()),
            FieldType::Bool => (false, "bool".into()),
            FieldType::String | FieldType::StringN { .. } | FieldType::Uuid => {
                (false, "str".into())
            }
            FieldType::Decimal => (false, "str".into()),
            FieldType::Enum(name) => (false, name.clone()),
            FieldType::Relation(
                RelationType::RequiredReference(_) | RelationType::OptionalReference(_),
            ) => Self::base_type(schema, &RustGenerator::resolved_type(schema, ft)),
            FieldType::Nullable(inner) => Self::base_type(schema, inner),
            _ => (true, "Any".into()),
        }
    }

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
from __future__ import annotations

import json
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import asdict, dataclass
from enum import Enum
from typing import Any, Generic, List, Optional, TypeVar

"#;

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

const CLIENT_HEADER: &str = r#"class ForgeDbClient:
    """A typed client for a ForgeDB app's REST API."""

    def __init__(self, base_url: str = "http://localhost:3000") -> None:
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
