//! Go REST client SDK generator (`generate go --sdk`, #205).
//!
//! Emits a standalone, **pure-stdlib** (`net/http` + `encoding/json`) Go client
//! package for the generated REST API — the Go sibling of the TypeScript SDK
//! (`typescript.rs`). It produces:
//!
//! - a Go `struct` (with `json` tags) for each model, a `<Model>Create` input,
//!   and one struct per `@projection` (#113);
//! - a string-typed enum (`type <E> string` + `const` block) for each `#enum`,
//!   serialized as the variant-name string (matching the wire);
//! - a shared `ForgeDbError` / `ListResult[T]` / `ListOptions` surface;
//! - a `Client` with full CRUD (Get / List / Create / Update / Delete) plus
//!   per-projection read methods, faithfully wrapping the REST endpoint's real
//!   response shapes and status codes.
//!
//! Distinct from the Go *runtime* binding (`go.rs`, RFC #203) — this is a network
//! REST client (no cgo, no FFI cdylib), pure standard library. Like the TS SDK it
//! is a transport client over the already-generated, schema-tailored REST surface,
//! interpreting no schema at runtime — class-2 access glue per `CLAUDE.md`.

use crate::rust::RustGenerator;
use crate::{GeneratedCode, Result};
use forgedb_parser::{Field, FieldType, Model, RelationType, Schema};

/// Go REST client SDK generator.
pub struct GoSdkGenerator;

impl GoSdkGenerator {
    /// Generate the Go SDK (`client.go`) for a schema.
    pub fn generate(schema: &Schema) -> Result<GeneratedCode> {
        Ok(GeneratedCode {
            code: Self::generate_code(schema),
            description: format!("Go REST client SDK ({} models)", schema.models.len()),
        })
    }

    /// The `go.mod` for the generated SDK module. Written next to `client.go` only
    /// when absent, so user edits survive regeneration. Pure stdlib — no `require`.
    pub fn go_mod_scaffold(module: &str) -> String {
        format!("module {module}\n\ngo 1.21\n")
    }

    /// A `README.md` for the generated SDK. Written only when absent (a scaffold).
    pub fn readme_scaffold() -> &'static str {
        "# Generated ForgeDB Go client SDK\n\
         \n\
         A pure-standard-library (`net/http` + `encoding/json`) REST client for your\n\
         ForgeDB app's generated API. No cgo, no external modules.\n\
         \n\
         ```go\n\
         client := forgedbclient.NewClient(\"http://localhost:3000\")\n\
         // client.GetX / ListX / CreateX / UpdateX / DeleteX per model\n\
         ```\n\
         \n\
         Regenerating overwrites `client.go` but never this file.\n"
    }

    fn generate_code(schema: &Schema) -> String {
        let mut c = String::new();
        c.push_str(FILE_HEADER);

        // Enums — a string type + a const block per declared enum.
        for en in &schema.enums {
            c.push_str(&format!(
                "// {name} — one of its declared variant-name strings.\n\
                 type {name} string\n\n\
                 const (\n",
                name = en.name
            ));
            for v in &en.variants {
                c.push_str(&format!("\t{name}{v} {name} = \"{v}\"\n", name = en.name, v = v));
            }
            c.push_str(")\n\n");
        }

        // Models + create-input + projection structs.
        for model in &schema.models {
            Self::push_model_struct(schema, &mut c, model);
            Self::push_create_struct(schema, &mut c, model);
            Self::push_projection_structs(schema, &mut c, model);
        }

        c.push_str(SHARED_TYPES);
        Self::push_client(&mut c, schema);
        c
    }

    fn push_model_struct(schema: &Schema, c: &mut String, model: &Model) {
        c.push_str(&format!(
            "// {name} mirrors the wire shape of the generated model.\n\
             type {name} struct {{\n",
            name = model.name
        ));
        for field in &model.fields {
            c.push_str(&Self::struct_field(schema, field));
        }
        c.push_str("}\n\n");
    }

    fn push_create_struct(schema: &Schema, c: &mut String, model: &Model) {
        c.push_str(&format!(
            "// {name}Create is the input to CreateX — a {name} without the fields the\n\
             // server synthesizes (+uuid/+timestamp autos).\n\
             type {name}Create struct {{\n",
            name = model.name
        ));
        for field in RustGenerator::creatable_fields(model) {
            c.push_str(&Self::struct_field(schema, field));
        }
        c.push_str("}\n\n");
    }

    fn push_projection_structs(schema: &Schema, c: &mut String, model: &Model) {
        for proj in &model.projections {
            let ty = format!(
                "{}{}",
                model.name,
                RustGenerator::projection_pascal(&proj.name)
            );
            c.push_str(&format!(
                "// {ty} is the `{proj}` projection of {model} — PK + declared columns.\n\
                 type {ty} struct {{\n",
                proj = proj.name,
                model = model.name
            ));
            for field in RustGenerator::projected_field_set(model, proj) {
                c.push_str(&Self::struct_field(schema, field));
            }
            c.push_str("}\n\n");
        }
    }

    /// One Go struct field line: `Exported Type `json:"snake_name"``.
    fn struct_field(schema: &Schema, field: &Field) -> String {
        format!(
            "\t{} {} `json:\"{}\"`\n",
            pascal(&field.name),
            Self::go_type(schema, field),
            field.name
        )
    }

    fn push_client(c: &mut String, schema: &Schema) {
        for model in &schema.models {
            let name = &model.name;
            let kebab = Self::to_kebab_case(name);

            // Get
            c.push_str(&format!(
                "// Get{name} fetches a {name} by id, or (nil, nil) if it does not exist.\n\
                 func (c *Client) Get{name}(id string) (*{name}, error) {{\n\
                 \tu := fmt.Sprintf(\"%s/api/{kebab}/%s\", c.baseURL, url.PathEscape(id))\n\
                 \tresp, err := c.do(http.MethodGet, u, \"\")\n\
                 \tif err != nil {{ return nil, err }}\n\
                 \tdefer resp.Body.Close()\n\
                 \tif resp.StatusCode == 404 {{ return nil, nil }}\n\
                 \tif !ok(resp) {{ return nil, errorFrom(resp) }}\n\
                 \tvar out {name}\n\
                 \tif err := json.NewDecoder(resp.Body).Decode(&out); err != nil {{ return nil, transportErr(err) }}\n\
                 \treturn &out, nil\n\
                 }}\n\n"
            ));

            // List
            c.push_str(&format!(
                "// List{name} lists {name} rows with optional pagination, sort, and filters.\n\
                 func (c *Client) List{name}(opts *ListOptions) (*ListResult[{name}], error) {{\n\
                 \tu := fmt.Sprintf(\"%s/api/{kebab}\", c.baseURL)\n\
                 \tif q := c.listQuery(opts).Encode(); q != \"\" {{ u += \"?\" + q }}\n\
                 \tresp, err := c.do(http.MethodGet, u, \"\")\n\
                 \tif err != nil {{ return nil, err }}\n\
                 \tdefer resp.Body.Close()\n\
                 \tif !ok(resp) {{ return nil, errorFrom(resp) }}\n\
                 \tvar out ListResult[{name}]\n\
                 \tif err := json.NewDecoder(resp.Body).Decode(&out); err != nil {{ return nil, transportErr(err) }}\n\
                 \treturn &out, nil\n\
                 }}\n\n"
            ));

            // Create
            c.push_str(&format!(
                "// Create{name} creates a {name} and returns the new id. Returns a *ForgeDbError\n\
                 // on a constraint (422) or integrity conflict (409).\n\
                 func (c *Client) Create{name}(data *{name}Create) (string, error) {{\n\
                 \tpayload, err := json.Marshal(data)\n\
                 \tif err != nil {{ return \"\", transportErr(err) }}\n\
                 \tu := fmt.Sprintf(\"%s/api/{kebab}\", c.baseURL)\n\
                 \tresp, err := c.do(http.MethodPost, u, string(payload))\n\
                 \tif err != nil {{ return \"\", err }}\n\
                 \tdefer resp.Body.Close()\n\
                 \tif !ok(resp) {{ return \"\", errorFrom(resp) }}\n\
                 \tvar out struct {{ Id string `json:\"id\"` }}\n\
                 \tif err := json.NewDecoder(resp.Body).Decode(&out); err != nil {{ return \"\", transportErr(err) }}\n\
                 \treturn out.Id, nil\n\
                 }}\n\n"
            ));

            // Update
            c.push_str(&format!(
                "// Update{name} replaces a {name} by id (whole-record PUT). Returns false if the\n\
                 // id does not exist; a *ForgeDbError on a constraint/conflict.\n\
                 func (c *Client) Update{name}(id string, data *{name}) (bool, error) {{\n\
                 \tpayload, err := json.Marshal(data)\n\
                 \tif err != nil {{ return false, transportErr(err) }}\n\
                 \tu := fmt.Sprintf(\"%s/api/{kebab}/%s\", c.baseURL, url.PathEscape(id))\n\
                 \tresp, err := c.do(http.MethodPut, u, string(payload))\n\
                 \tif err != nil {{ return false, err }}\n\
                 \tdefer resp.Body.Close()\n\
                 \tif resp.StatusCode == 404 {{ return false, nil }}\n\
                 \tif !ok(resp) {{ return false, errorFrom(resp) }}\n\
                 \treturn true, nil\n\
                 }}\n\n"
            ));

            // Delete
            c.push_str(&format!(
                "// Delete{name} deletes a {name} by id. Returns true if deleted, false if absent;\n\
                 // a *ForgeDbError (409) if @on_delete(restrict) children block it.\n\
                 func (c *Client) Delete{name}(id string) (bool, error) {{\n\
                 \tu := fmt.Sprintf(\"%s/api/{kebab}/%s\", c.baseURL, url.PathEscape(id))\n\
                 \tresp, err := c.do(http.MethodDelete, u, \"\")\n\
                 \tif err != nil {{ return false, err }}\n\
                 \tdefer resp.Body.Close()\n\
                 \tif resp.StatusCode == 404 {{ return false, nil }}\n\
                 \tif !ok(resp) {{ return false, errorFrom(resp) }}\n\
                 \treturn true, nil\n\
                 }}\n\n"
            ));

            // Projections
            for proj in &model.projections {
                let ty = format!("{}{}", name, RustGenerator::projection_pascal(&proj.name));
                let method = format!("{}{}", name, RustGenerator::projection_pascal(&proj.name));
                let pname = &proj.name;
                c.push_str(&format!(
                    "// Get{method} fetches the `{pname}` projection of a {name} by id (nil if absent).\n\
                     func (c *Client) Get{method}(id string) (*{ty}, error) {{\n\
                     \tu := fmt.Sprintf(\"%s/api/{kebab}/%s?projection={pname}\", c.baseURL, url.PathEscape(id))\n\
                     \tresp, err := c.do(http.MethodGet, u, \"\")\n\
                     \tif err != nil {{ return nil, err }}\n\
                     \tdefer resp.Body.Close()\n\
                     \tif resp.StatusCode == 404 {{ return nil, nil }}\n\
                     \tif !ok(resp) {{ return nil, errorFrom(resp) }}\n\
                     \tvar out {ty}\n\
                     \tif err := json.NewDecoder(resp.Body).Decode(&out); err != nil {{ return nil, transportErr(err) }}\n\
                     \treturn &out, nil\n\
                     }}\n\n"
                ));
                c.push_str(&format!(
                    "// List{method} lists {name} rows as the `{pname}` projection (PK + declared columns).\n\
                     func (c *Client) List{method}(opts *ListOptions) (*ListResult[{ty}], error) {{\n\
                     \tq := c.listQuery(opts)\n\
                     \tq.Set(\"projection\", \"{pname}\")\n\
                     \tu := fmt.Sprintf(\"%s/api/{kebab}\", c.baseURL)\n\
                     \tif enc := q.Encode(); enc != \"\" {{ u += \"?\" + enc }}\n\
                     \tresp, err := c.do(http.MethodGet, u, \"\")\n\
                     \tif err != nil {{ return nil, err }}\n\
                     \tdefer resp.Body.Close()\n\
                     \tif !ok(resp) {{ return nil, errorFrom(resp) }}\n\
                     \tvar out ListResult[{ty}]\n\
                     \tif err := json.NewDecoder(resp.Body).Decode(&out); err != nil {{ return nil, transportErr(err) }}\n\
                     \treturn &out, nil\n\
                     }}\n\n"
                ));
            }
        }
    }

    /// Map a schema field to its Go SDK wire type. Scalars map precisely; a FK
    /// reference is the uuid it stores (`string`); the opaque bucket (`json`,
    /// `char(N)`, fixed arrays, inline structs, and virtual one-to-many / M2M
    /// relations, which the server serializes as `null`) maps to
    /// `json.RawMessage` — the honest analogue of the TS SDK's `unknown`/`any`.
    fn go_type(schema: &Schema, field: &Field) -> String {
        let (opaque, base) = Self::base_type(schema, &field.field_type);
        if opaque {
            // json.RawMessage already carries `null`, so no pointer wrapper.
            "json.RawMessage".to_string()
        } else if field.is_nullable() {
            format!("*{base}")
        } else {
            base
        }
    }

    fn base_type(schema: &Schema, ft: &FieldType) -> (bool, String) {
        match ft {
            FieldType::U32 => (false, "uint32".into()),
            FieldType::U64 => (false, "uint64".into()),
            FieldType::I32 => (false, "int32".into()),
            FieldType::I64 => (false, "int64".into()),
            FieldType::F64 => (false, "float64".into()),
            // #254: RFC 3339 string on the wire.
            FieldType::Timestamp(_) => (false, "string".into()),
            FieldType::Bool => (false, "bool".into()),
            // #238: an inline `string(N)` is a string on the wire.
            FieldType::String | FieldType::StringN { .. } | FieldType::Uuid => {
                (false, "string".into())
            }
            FieldType::Decimal => (false, "string".into()),
            FieldType::Enum(name) => (false, name.clone()),
            // #266: an FK carries the target's identity value on the wire.
            FieldType::Relation(
                RelationType::RequiredReference(_) | RelationType::OptionalReference(_),
            ) => Self::base_type(schema, &RustGenerator::resolved_type(schema, ft)),
            FieldType::Nullable(inner) => Self::base_type(schema, inner),
            _ => (true, "json.RawMessage".into()),
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

/// PascalCase (exported) a snake_case schema identifier (`author_id` → `AuthorId`).
fn pascal(name: &str) -> String {
    name.split('_')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut ch = p.chars();
            match ch.next() {
                Some(first) => first.to_uppercase().collect::<String>() + ch.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

const FILE_HEADER: &str = r#"// Code generated by ForgeDB. DO NOT EDIT.
//
// Go REST client SDK for a ForgeDB app. A transport client over the generated
// REST API; it interprets no schema at runtime. Pure standard library.
package forgedbclient

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strconv"
	"strings"
)

"#;

/// The schema-independent SDK support types + HTTP client scaffolding.
const SHARED_TYPES: &str = r#"// ForgeDbError is returned on any non-2xx response (except a Get/Update/Delete
// 404, surfaced as nil/false), and on transport failures (Status == 0). It
// carries the HTTP status and the parsed {error} body when present.
type ForgeDbError struct {
	Status  int
	Message string
	Body    json.RawMessage
}

func (e *ForgeDbError) Error() string {
	return fmt.Sprintf("ForgeDB error (status %d): %s", e.Status, e.Message)
}

// transportErr wraps a client-side failure (dial, body decode, …): Status 0.
func transportErr(err error) *ForgeDbError {
	return &ForgeDbError{Status: 0, Message: err.Error()}
}

// errorFrom builds a *ForgeDbError from a non-2xx response, extracting the
// server's {error} message when the body is JSON. It reads (but does not close)
// the body — the caller's deferred Close still runs.
func errorFrom(resp *http.Response) *ForgeDbError {
	raw, _ := io.ReadAll(resp.Body)
	msg := fmt.Sprintf("HTTP %d", resp.StatusCode)
	var parsed struct {
		Error string `json:"error"`
	}
	if json.Unmarshal(raw, &parsed) == nil && parsed.Error != "" {
		msg = parsed.Error
	}
	return &ForgeDbError{Status: resp.StatusCode, Message: msg, Body: json.RawMessage(raw)}
}

func ok(resp *http.Response) bool {
	return resp.StatusCode >= 200 && resp.StatusCode < 300
}

// ListResult is a page of list results — mirrors the REST list response envelope.
type ListResult[T any] struct {
	Data   []T `json:"data"`
	Total  int `json:"total"`
	Limit  int `json:"limit"`
	Offset int `json:"offset"`
}

// ListOptions holds a list query. Filter holds exact-match ?field=value pairs
// matched by the generated per-model filter server-side.
type ListOptions struct {
	Limit  *int
	Offset *int
	Sort   string // "" = unset
	Order  string // "asc" | "desc" | "" = unset
	Filter map[string]string
}

// Client is a typed client for a ForgeDB app's REST API.
type Client struct {
	baseURL string
	http    *http.Client
}

// NewClient builds a client targeting baseURL (a trailing slash is trimmed).
func NewClient(baseURL string) *Client {
	return &Client{baseURL: strings.TrimRight(baseURL, "/"), http: http.DefaultClient}
}

// NewClientWith builds a client over a caller-supplied *http.Client.
func NewClientWith(baseURL string, hc *http.Client) *Client {
	return &Client{baseURL: strings.TrimRight(baseURL, "/"), http: hc}
}

func (c *Client) listQuery(o *ListOptions) url.Values {
	q := url.Values{}
	if o == nil {
		return q
	}
	if o.Limit != nil {
		q.Set("limit", strconv.Itoa(*o.Limit))
	}
	if o.Offset != nil {
		q.Set("offset", strconv.Itoa(*o.Offset))
	}
	if o.Sort != "" {
		q.Set("sort", o.Sort)
	}
	if o.Order != "" {
		q.Set("order", o.Order)
	}
	for k, v := range o.Filter {
		q.Set(k, v)
	}
	return q
}

// do performs a request. A non-empty body is sent as application/json.
func (c *Client) do(method, rawURL, body string) (*http.Response, error) {
	var reader io.Reader
	if body != "" {
		reader = strings.NewReader(body)
	}
	req, err := http.NewRequest(method, rawURL, reader)
	if err != nil {
		return nil, transportErr(err)
	}
	if body != "" {
		req.Header.Set("Content-Type", "application/json")
	}
	resp, err := c.http.Do(req)
	if err != nil {
		return nil, transportErr(err)
	}
	return resp, nil
}

"#;
