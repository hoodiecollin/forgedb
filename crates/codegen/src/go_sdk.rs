use crate::rust::RustGenerator;
use crate::{GeneratedCode, Result};
use forgedb_parser::{Field, FieldType, Model, RelationType, Schema};

pub struct GoSdkGenerator;

impl GoSdkGenerator {
    pub fn generate(schema: &Schema) -> Result<GeneratedCode> {
        Ok(GeneratedCode {
            code: Self::generate_code(schema),
            description: format!("Go REST client SDK ({} models)", schema.models.len()),
        })
    }

    pub fn go_mod_scaffold(module: &str) -> String {
        format!("module {module}\n\ngo 1.21\n")
    }

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

        for en in &schema.enums {
            c.push_str(&format!(
                "type {name} string\n\n\
                 const (\n",
                name = en.name
            ));
            for v in &en.variants {
                c.push_str(&format!("\t{name}{v} {name} = \"{v}\"\n", name = en.name, v = v));
            }
            c.push_str(")\n\n");
        }

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
            "type {name} struct {{\n",
            name = model.name
        ));
        for field in &model.fields {
            c.push_str(&Self::struct_field(schema, field));
        }
        c.push_str("}\n\n");
    }

    fn push_create_struct(schema: &Schema, c: &mut String, model: &Model) {
        c.push_str(&format!(
            "type {name}Create struct {{\n",
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
                "type {ty} struct {{\n"
            ));
            for field in RustGenerator::projected_field_set(model, proj) {
                c.push_str(&Self::struct_field(schema, field));
            }
            c.push_str("}\n\n");
        }
    }

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

            c.push_str(&format!(
                "func (c *Client) Get{name}(id string) (*{name}, error) {{\n\
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

            c.push_str(&format!(
                "func (c *Client) List{name}(opts *ListOptions) (*ListResult[{name}], error) {{\n\
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

            c.push_str(&format!(
                "func (c *Client) Create{name}(data *{name}Create) (string, error) {{\n\
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

            c.push_str(&format!(
                "func (c *Client) Update{name}(id string, data *{name}) (bool, error) {{\n\
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

            c.push_str(&format!(
                "func (c *Client) Delete{name}(id string) (bool, error) {{\n\
                 \tu := fmt.Sprintf(\"%s/api/{kebab}/%s\", c.baseURL, url.PathEscape(id))\n\
                 \tresp, err := c.do(http.MethodDelete, u, \"\")\n\
                 \tif err != nil {{ return false, err }}\n\
                 \tdefer resp.Body.Close()\n\
                 \tif resp.StatusCode == 404 {{ return false, nil }}\n\
                 \tif !ok(resp) {{ return false, errorFrom(resp) }}\n\
                 \treturn true, nil\n\
                 }}\n\n"
            ));

            for proj in &model.projections {
                let ty = format!("{}{}", name, RustGenerator::projection_pascal(&proj.name));
                let method = format!("{}{}", name, RustGenerator::projection_pascal(&proj.name));
                let pname = &proj.name;
                c.push_str(&format!(
                    "func (c *Client) Get{method}(id string) (*{ty}, error) {{\n\
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
                    "func (c *Client) List{method}(opts *ListOptions) (*ListResult[{ty}], error) {{\n\
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

    fn go_type(schema: &Schema, field: &Field) -> String {
        let (opaque, base) = Self::base_type(schema, &field.field_type);
        if opaque {
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
            FieldType::Timestamp(_) => (false, "string".into()),
            FieldType::Bool => (false, "bool".into()),
            FieldType::String | FieldType::StringN { .. } | FieldType::Uuid => {
                (false, "string".into())
            }
            FieldType::Decimal => (false, "string".into()),
            FieldType::Enum(name) => (false, name.clone()),
            FieldType::Relation(
                RelationType::RequiredReference(_) | RelationType::OptionalReference(_),
            ) => Self::base_type(schema, &RustGenerator::resolved_type(schema, ft)),
            FieldType::Nullable(inner) => Self::base_type(schema, inner),
            _ => (true, "json.RawMessage".into()),
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

const SHARED_TYPES: &str = r#"type ForgeDbError struct {
	Status  int
	Message string
	Body    json.RawMessage
}

func (e *ForgeDbError) Error() string {
	return fmt.Sprintf("ForgeDB error (status %d): %s", e.Status, e.Message)
}

func transportErr(err error) *ForgeDbError {
	return &ForgeDbError{Status: 0, Message: err.Error()}
}

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

type ListResult[T any] struct {
	Data   []T `json:"data"`
	Total  int `json:"total"`
	Limit  int `json:"limit"`
	Offset int `json:"offset"`
}

type ListOptions struct {
	Limit  *int
	Offset *int
	Sort   string // "" = unset
	Order  string // "asc" | "desc" | "" = unset
	Filter map[string]string
}

type Client struct {
	baseURL string
	http    *http.Client
}

func NewClient(baseURL string) *Client {
	return &Client{baseURL: strings.TrimRight(baseURL, "/"), http: http.DefaultClient}
}

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
