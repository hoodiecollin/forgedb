use crate::rust::RustGenerator;
use crate::{GeneratedCode, Result};
use forgedb_parser::{Field, FieldType, Model, RelationType, Schema};

pub struct RustSdkGenerator;

impl RustSdkGenerator {
    pub fn generate(schema: &Schema) -> Result<GeneratedCode> {
        let raw = Self::generate_code(schema);
        let code = match syn::parse_file(&raw) {
            Ok(tree) => prettyplease::unparse(&tree),
            Err(e) => {
                return Err(crate::CodegenError::GenerationFailed(format!(
                    "generated Rust SDK did not parse: {e}"
                )));
            }
        };
        Ok(GeneratedCode {
            code,
            description: format!("Rust REST client SDK ({} models)", schema.models.len()),
        })
    }

    pub fn cargo_toml_scaffold(crate_name: &str) -> String {
        format!(
            "[package]\n\
             name = \"{crate_name}\"\n\
             version = \"0.1.0\"\n\
             edition = \"2021\"\n\
             \n\
             [dependencies]\n\
             reqwest = {{ version = \"0.12\", features = [\"json\"] }}\n\
             serde = {{ version = \"1\", features = [\"derive\"] }}\n\
             serde_json = \"1\"\n\
             \n\
             # The client methods are `async`; bring your own runtime, e.g.:\n\
             # tokio = {{ version = \"1\", features = [\"full\"] }}\n"
        )
    }

    fn generate_code(schema: &Schema) -> String {
        let mut c = String::new();
        c.push_str(
            "//\n\
             // Rust REST client SDK for a ForgeDB app. A transport client over the\n\
             // generated REST API; it interprets no schema at runtime.\n\
             #![allow(dead_code, clippy::all)]\n\n\
             use serde::{Deserialize, Serialize};\n\n",
        );

        for en in &schema.enums {
            c.push_str(&format!(
                "#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]\n\
                 pub enum {name} {{\n",
                name = en.name
            ));
            for v in &en.variants {
                c.push_str(&format!("    {v},\n"));
            }
            c.push_str("}\n\n");
        }

        for model in &schema.models {
            Self::push_model_struct(schema, &mut c, model);
            Self::push_create_struct(schema, &mut c, model);
            Self::push_projection_structs(schema, &mut c, model);
        }

        c.push_str(Self::shared_types());
        Self::push_client(&mut c, schema);
        c
    }

    fn push_model_struct(schema: &Schema, c: &mut String, model: &Model) {
        c.push_str(&format!(
            "#[derive(Debug, Clone, Serialize, Deserialize)]\n\
             pub struct {name} {{\n",
            name = model.name
        ));
        for field in &model.fields {
            c.push_str(&format!("    pub {}: {},\n", field.name, Self::map_type(schema, field)));
        }
        c.push_str("}\n\n");
    }

    fn push_create_struct(schema: &Schema, c: &mut String, model: &Model) {
        c.push_str(&format!(
            "#[derive(Debug, Clone, Serialize)]\n\
             pub struct {name}Create {{\n",
            name = model.name
        ));
        for field in RustGenerator::creatable_fields(model) {
            c.push_str(&format!("    pub {}: {},\n", field.name, Self::map_type(schema, field)));
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
                "#[derive(Debug, Clone, Deserialize)]\n\
                 pub struct {ty} {{\n"
            ));
            for field in RustGenerator::projected_field_set(model, proj) {
                c.push_str(&format!("    pub {}: {},\n", field.name, Self::map_type(schema, field)));
            }
            c.push_str("}\n\n");
        }
    }

    fn shared_types() -> &'static str {
        r#"#[derive(Debug, Clone)]
pub struct ForgeDbError {
    pub status: u16,
    pub message: String,
    pub body: Option<serde_json::Value>,
}

impl std::fmt::Display for ForgeDbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ForgeDB error (status {}): {}", self.status, self.message)
    }
}

impl std::error::Error for ForgeDbError {}

impl ForgeDbError {
    fn transport(e: reqwest::Error) -> Self {
        Self { status: 0, message: e.to_string(), body: None }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListResult<T> {
    pub data: Vec<T>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Asc,
    Desc,
}

impl SortOrder {
    fn as_str(self) -> &'static str {
        match self {
            SortOrder::Asc => "asc",
            SortOrder::Desc => "desc",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub sort: Option<String>,
    pub order: Option<SortOrder>,
    pub filter: Vec<(String, String)>,
}

#[derive(Deserialize)]
struct CreatedId {
    id: String,
}

fn encode_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

pub struct ForgeDbClient {
    base_url: String,
    http: reqwest::Client,
}

impl ForgeDbClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }

    pub fn with_client(base_url: impl Into<String>, http: reqwest::Client) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http,
        }
    }

    fn list_query(&self, options: &ListOptions) -> Vec<(String, String)> {
        let mut q: Vec<(String, String)> = Vec::new();
        if let Some(v) = options.limit {
            q.push(("limit".to_string(), v.to_string()));
        }
        if let Some(v) = options.offset {
            q.push(("offset".to_string(), v.to_string()));
        }
        if let Some(v) = &options.sort {
            q.push(("sort".to_string(), v.clone()));
        }
        if let Some(v) = options.order {
            q.push(("order".to_string(), v.as_str().to_string()));
        }
        for (k, val) in &options.filter {
            q.push((k.clone(), val.clone()));
        }
        q
    }

    async fn assert_ok(&self, response: reqwest::Response) -> std::result::Result<reqwest::Response, ForgeDbError> {
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status().as_u16();
        let body = response.json::<serde_json::Value>().await.ok();
        let message = body
            .as_ref()
            .and_then(|b| b.get("error"))
            .and_then(|e| e.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("HTTP {status}"));
        Err(ForgeDbError { status, message, body })
    }
}

"#
    }

    fn push_client(c: &mut String, schema: &Schema) {
        c.push_str("impl ForgeDbClient {\n");
        for model in &schema.models {
            let name = &model.name;
            let snake = RustGenerator::to_snake_case(name);
            let kebab = Self::to_kebab_case(name);

            c.push_str(&format!(
                "    pub async fn get_{snake}(&self, id: &str) -> std::result::Result<Option<{name}>, ForgeDbError> {{\n\
                 \x20       let url = format!(\"{{}}/api/{kebab}/{{}}\", self.base_url, encode_segment(id));\n\
                 \x20       let response = self.http.get(&url).send().await.map_err(ForgeDbError::transport)?;\n\
                 \x20       if response.status().as_u16() == 404 {{ return Ok(None); }}\n\
                 \x20       let response = self.assert_ok(response).await?;\n\
                 \x20       response.json::<{name}>().await.map(Some).map_err(ForgeDbError::transport)\n\
                 \x20   }}\n\n"
            ));

            c.push_str(&format!(
                "    pub async fn list_{snake}(&self, options: &ListOptions) -> std::result::Result<ListResult<{name}>, ForgeDbError> {{\n\
                 \x20       let url = format!(\"{{}}/api/{kebab}\", self.base_url);\n\
                 \x20       let response = self.http.get(&url).query(&self.list_query(options)).send().await.map_err(ForgeDbError::transport)?;\n\
                 \x20       let response = self.assert_ok(response).await?;\n\
                 \x20       response.json::<ListResult<{name}>>().await.map_err(ForgeDbError::transport)\n\
                 \x20   }}\n\n"
            ));

            c.push_str(&format!(
                "    pub async fn create_{snake}(&self, data: &{name}Create) -> std::result::Result<String, ForgeDbError> {{\n\
                 \x20       let url = format!(\"{{}}/api/{kebab}\", self.base_url);\n\
                 \x20       let response = self.http.post(&url).json(data).send().await.map_err(ForgeDbError::transport)?;\n\
                 \x20       let response = self.assert_ok(response).await?;\n\
                 \x20       response.json::<CreatedId>().await.map(|r| r.id).map_err(ForgeDbError::transport)\n\
                 \x20   }}\n\n"
            ));

            c.push_str(&format!(
                "    pub async fn update_{snake}(&self, id: &str, data: &{name}) -> std::result::Result<bool, ForgeDbError> {{\n\
                 \x20       let url = format!(\"{{}}/api/{kebab}/{{}}\", self.base_url, encode_segment(id));\n\
                 \x20       let response = self.http.put(&url).json(data).send().await.map_err(ForgeDbError::transport)?;\n\
                 \x20       if response.status().as_u16() == 404 {{ return Ok(false); }}\n\
                 \x20       self.assert_ok(response).await?;\n\
                 \x20       Ok(true)\n\
                 \x20   }}\n\n"
            ));

            c.push_str(&format!(
                "    pub async fn delete_{snake}(&self, id: &str) -> std::result::Result<bool, ForgeDbError> {{\n\
                 \x20       let url = format!(\"{{}}/api/{kebab}/{{}}\", self.base_url, encode_segment(id));\n\
                 \x20       let response = self.http.delete(&url).send().await.map_err(ForgeDbError::transport)?;\n\
                 \x20       if response.status().as_u16() == 404 {{ return Ok(false); }}\n\
                 \x20       self.assert_ok(response).await?;\n\
                 \x20       Ok(true)\n\
                 \x20   }}\n\n"
            ));

            for proj in &model.projections {
                let ty = format!("{}{}", name, RustGenerator::projection_pascal(&proj.name));
                let proj_snake = RustGenerator::to_snake_case(&proj.name);
                let pname = &proj.name;
                c.push_str(&format!(
                    "    pub async fn get_{snake}_{proj_snake}(&self, id: &str) -> std::result::Result<Option<{ty}>, ForgeDbError> {{\n\
                     \x20       let url = format!(\"{{}}/api/{kebab}/{{}}\", self.base_url, encode_segment(id));\n\
                     \x20       let response = self.http.get(&url).query(&[(\"projection\", \"{pname}\")]).send().await.map_err(ForgeDbError::transport)?;\n\
                     \x20       if response.status().as_u16() == 404 {{ return Ok(None); }}\n\
                     \x20       let response = self.assert_ok(response).await?;\n\
                     \x20       response.json::<{ty}>().await.map(Some).map_err(ForgeDbError::transport)\n\
                     \x20   }}\n\n"
                ));
                c.push_str(&format!(
                    "    pub async fn list_{snake}_{proj_snake}(&self, options: &ListOptions) -> std::result::Result<ListResult<{ty}>, ForgeDbError> {{\n\
                     \x20       let url = format!(\"{{}}/api/{kebab}\", self.base_url);\n\
                     \x20       let mut q = self.list_query(options);\n\
                     \x20       q.push((\"projection\".to_string(), \"{pname}\".to_string()));\n\
                     \x20       let response = self.http.get(&url).query(&q).send().await.map_err(ForgeDbError::transport)?;\n\
                     \x20       let response = self.assert_ok(response).await?;\n\
                     \x20       response.json::<ListResult<{ty}>>().await.map_err(ForgeDbError::transport)\n\
                     \x20   }}\n\n"
                ));
            }
        }
        c.push_str("}\n");
    }

    fn map_type(schema: &Schema, field: &Field) -> String {
        let (opaque, base) = Self::base_type(schema, &field.field_type);
        if opaque {
            base
        } else if field.is_nullable() {
            format!("Option<{base}>")
        } else {
            base
        }
    }

    fn base_type(schema: &Schema, ft: &FieldType) -> (bool, String) {
        match ft {
            FieldType::U32 => (false, "u32".into()),
            FieldType::U64 => (false, "u64".into()),
            FieldType::I32 => (false, "i32".into()),
            FieldType::I64 => (false, "i64".into()),
            FieldType::F64 => (false, "f64".into()),
            FieldType::Timestamp(_) => (false, "String".into()),
            FieldType::Bool => (false, "bool".into()),
            FieldType::String | FieldType::StringN { .. } | FieldType::Uuid => {
                (false, "String".into())
            }
            FieldType::Decimal => (false, "String".into()),
            FieldType::Enum(name) => (false, name.clone()),
            FieldType::Relation(
                RelationType::RequiredReference(_) | RelationType::OptionalReference(_),
            ) => Self::base_type(schema, &RustGenerator::resolved_type(schema, ft)),
            FieldType::Nullable(inner) => Self::base_type(schema, inner),
            _ => (true, "serde_json::Value".into()),
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
