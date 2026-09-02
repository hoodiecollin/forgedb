use crate::config::GenConfig;
use crate::{CodegenError, GeneratedCode, Result};
use forgedb_parser::Schema;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

thread_local! {
    static ACTIVE_CONFIG: std::cell::Cell<GenConfig> = const { std::cell::Cell::new(GenConfig::DEFAULT) };
}

pub struct RustGenerator;

struct BufferedGather {
    decls: Vec<TokenStream>,
    inits: Vec<TokenStream>,
    reads: Vec<TokenStream>,
    values: Vec<TokenStream>,
}

pub const CURRENT_ENGINE_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BoundLiteral {
    Int(i64),
    Frac(String),
}

impl BoundLiteral {
    fn render(&self) -> String {
        match self {
            BoundLiteral::Int(n) => n.to_string(),
            BoundLiteral::Frac(s) => s.clone(),
        }
    }

    pub(crate) fn decimal_parts(lexeme: &str) -> Option<(i128, u32)> {
        let (int_part, frac_part) = lexeme.split_once('.').unwrap_or((lexeme, ""));
        let digits = format!("{int_part}{frac_part}");
        digits
            .parse::<i128>()
            .ok()
            .map(|m| (m, frac_part.len() as u32))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OnDeletePolicy {
    Restrict,
    Cascade,
    SetNull,
}

impl RustGenerator {
    pub fn generate(schema: &Schema) -> Result<GeneratedCode> {
        Self::generate_with_schema_version(schema, 1)
    }

    pub fn generate_with_schema_version(
        schema: &Schema,
        schema_version: u32,
    ) -> Result<GeneratedCode> {
        Self::generate_with_config(schema, schema_version, GenConfig::DEFAULT)
    }

    pub fn generate_with_versions(
        schema: &Schema,
        schema_version: u32,
        engine_version: u32,
    ) -> Result<GeneratedCode> {
        Self::generate_with_config_and_engine(
            schema,
            schema_version,
            engine_version,
            GenConfig::DEFAULT,
        )
    }

    pub fn generate_with_config(
        schema: &Schema,
        schema_version: u32,
        config: GenConfig,
    ) -> Result<GeneratedCode> {
        Self::generate_with_config_and_engine(
            schema,
            schema_version,
            CURRENT_ENGINE_VERSION,
            config,
        )
    }

    pub fn generate_with_config_and_engine(
        schema: &Schema,
        schema_version: u32,
        engine_version: u32,
        config: GenConfig,
    ) -> Result<GeneratedCode> {
        ACTIVE_CONFIG.with(|c| c.set(config));
        let code = Self::generate_code(schema, schema_version, engine_version)?;

        Ok(GeneratedCode {
            code,
            description: format!(
                "Rust database implementation ({} models)",
                schema.models.len()
            ),
        })
    }

    fn schema_attr(attr: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
        if Self::active_cfg().needs_utoipa() {
            attr
        } else {
            quote! {}
        }
    }

    fn to_schema_derive() -> proc_macro2::TokenStream {
        if Self::active_cfg().needs_utoipa() {
            quote! { , ToSchema }
        } else {
            quote! {}
        }
    }

    fn active_cfg() -> GenConfig {
        ACTIVE_CONFIG.with(|c| c.get())
    }

    fn wal_fsync_policy_tokens() -> TokenStream {
        let variant = format_ident!("{}", Self::active_cfg().fsync.wal_policy_variant());
        quote! { forgedb_wal::FsyncPolicy::#variant }
    }

    fn is_projectable(schema: &Schema, field: &forgedb_parser::Field) -> bool {
        Self::is_variable_column_type(&field.field_type)
            || Self::is_fixed_size_type(schema, &field.field_type)
    }

    fn validate_projections(schema: &Schema) -> Result<()> {
        for model in &schema.models {
            for proj in &model.projections {
                for fname in &proj.fields {
                    let field = model
                        .fields
                        .iter()
                        .find(|f| &f.name == fname)
                        .ok_or_else(|| {
                            CodegenError::InvalidSchema(format!(
                                "@projection '{}' on model '{}' references undefined field '{}'",
                                proj.name, model.name, fname
                            ))
                        })?;
                    if !Self::is_projectable(schema, field) {
                        return Err(CodegenError::InvalidSchema(format!(
                            "@projection '{}' on model '{}' cannot include field '{}': \
                             relation and virtual fields have no column and are not projectable \
                             (use eager-load / relation traversal for related records)",
                            proj.name, model.name, fname
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    fn on_delete_policy(field: &forgedb_parser::ast::Field) -> OnDeletePolicy {
        for c in &field.constraints {
            if c.name == "on_delete" {
                if let Some(forgedb_parser::ast::ConstraintParam::String(p)) = c.params.first() {
                    return match p.as_str() {
                        "cascade" => OnDeletePolicy::Cascade,
                        "set_null" => OnDeletePolicy::SetNull,
                        _ => OnDeletePolicy::Restrict,
                    };
                }
            }
        }
        OnDeletePolicy::Restrict
    }

    fn validate_on_delete(schema: &Schema) -> Result<()> {
        for model in &schema.models {
            for field in &model.fields {
                let Some(c) = field.constraints.iter().find(|c| c.name == "on_delete") else {
                    continue;
                };
                let value = match c.params.first() {
                    Some(forgedb_parser::ast::ConstraintParam::String(p)) => p.as_str(),
                    _ => {
                        return Err(CodegenError::InvalidSchema(format!(
                            "@on_delete on '{}.{}' requires a policy argument \
                             (restrict | cascade | set_null)",
                            model.name, field.name
                        )));
                    }
                };
                if !matches!(value, "restrict" | "cascade" | "set_null") {
                    return Err(CodegenError::InvalidSchema(format!(
                        "@on_delete('{}') on '{}.{}' is not a valid policy \
                         (expected restrict | cascade | set_null)",
                        value, model.name, field.name
                    )));
                }
                let optional_fk = match &field.field_type {
                    forgedb_parser::FieldType::Relation(
                        forgedb_parser::RelationType::OptionalReference(_),
                    ) => true,
                    forgedb_parser::FieldType::Relation(
                        forgedb_parser::RelationType::RequiredReference(_),
                    ) => false,
                    _ => {
                        return Err(CodegenError::InvalidSchema(format!(
                            "@on_delete on '{}.{}' is only valid on a foreign-key field \
                             (`*Target` required or `?Target` optional)",
                            model.name, field.name
                        )));
                    }
                };
                if value == "set_null" && !optional_fk {
                    return Err(CodegenError::InvalidSchema(format!(
                        "@on_delete(set_null) on '{}.{}' is invalid: the field is a required \
                         foreign key (`*`) and cannot be set to null — use an optional FK \
                         (`?Target`), or choose `cascade`/`restrict`",
                        model.name, field.name
                    )));
                }
            }
        }
        Ok(())
    }

    fn generate_code(schema: &Schema, schema_version: u32, engine_version: u32) -> Result<String> {
        Self::validate_projections(schema)?;
        Self::validate_on_delete(schema)?;

        let mut tokens = TokenStream::new();

        let header = quote! {
        };
        tokens.extend(header);

        let inline_str_import = Self::inline_str_import(schema);
        let utoipa_import = if Self::active_cfg().needs_utoipa() {
            quote! { use utoipa::ToSchema; }
        } else {
            quote! {}
        };
        let imports = quote! {
            #![allow(dead_code, unused_imports, irrefutable_let_patterns, unused_mut)]

            use std::collections::HashMap;
            use std::path::{Path, PathBuf};
            use forgedb_storage::{FixedColumn, VariableColumn, Tombstones};
            #inline_str_import
            use forgedb_types::{Uuid, Timestamp, Value};
            use serde::{Deserialize, Serialize};
            #utoipa_import
        };
        tokens.extend(imports);

        let cfg = Self::active_cfg();

        let __wal_checkpoint_interval =
            proc_macro2::Literal::u64_unsuffixed(cfg.wal_checkpoint_interval);
        tokens.extend(quote! {
            const WAL_CHECKPOINT_INTERVAL: u64 = #__wal_checkpoint_interval;
        });

        let __compaction_threshold =
            proc_macro2::Literal::u64_unsuffixed(cfg.compaction_threshold);
        tokens.extend(quote! {
            const COMPACTION_DEAD_THRESHOLD: u64 = #__compaction_threshold;
            const COMPACTION_DEAD_CEILING_FACTOR: u64 = 4;
        });

        let __expected_schema_version =
            proc_macro2::Literal::u32_unsuffixed(schema_version);
        let __expected_engine_version =
            proc_macro2::Literal::u32_unsuffixed(engine_version);
        tokens.extend(quote! {
            const EXPECTED_SCHEMA_VERSION: u32 = #__expected_schema_version;
            const EXPECTED_ENGINE_VERSION: u32 = #__expected_engine_version;
        });

        tokens.extend(quote! {
            fn __forgedb_default_ts() -> Timestamp {
                Timestamp::from_micros(0)
            }

            fn __forgedb_ws_key(parts: &[&[u8]]) -> Vec<u8> {
                let mut k = Vec::with_capacity(
                    parts.iter().map(|p| p.len() + 4).sum::<usize>(),
                );
                for p in parts {
                    k.extend_from_slice(&(p.len() as u32).to_le_bytes());
                    k.extend_from_slice(p);
                }
                k
            }
        });

        if Self::schema_needs_big_array_serde(schema) {
            tokens.extend(quote! {
                mod __forgedb_big_array {
                    use serde::de::{Error as _, SeqAccess, Visitor};
                    use serde::ser::SerializeTuple;
                    use serde::{Deserialize, Deserializer, Serializer};

                    pub fn serialize<S, T, const N: usize>(
                        value: &[T; N],
                        serializer: S,
                    ) -> Result<S::Ok, S::Error>
                    where
                        S: Serializer,
                        T: serde::Serialize,
                    {
                        let mut tup = serializer.serialize_tuple(N)?;
                        for item in value.iter() {
                            tup.serialize_element(item)?;
                        }
                        tup.end()
                    }

                    struct ArrayVisitor<T, const N: usize>(std::marker::PhantomData<T>);

                    impl<'de, T, const N: usize> Visitor<'de> for ArrayVisitor<T, N>
                    where
                        T: Deserialize<'de>,
                    {
                        type Value = [T; N];

                        fn expecting(
                            &self,
                            f: &mut std::fmt::Formatter<'_>,
                        ) -> std::fmt::Result {
                            write!(f, "an array of {N} elements")
                        }

                        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
                        where
                            A: SeqAccess<'de>,
                        {
                            let mut items = Vec::with_capacity(N);
                            while let Some(item) = seq.next_element::<T>()? {
                                if items.len() == N {
                                    return Err(A::Error::invalid_length(N + 1, &self));
                                }
                                items.push(item);
                            }
                            let got = items.len();
                            <[T; N]>::try_from(items)
                                .map_err(|_| A::Error::invalid_length(got, &self))
                        }
                    }

                    pub fn deserialize<'de, D, T, const N: usize>(
                        deserializer: D,
                    ) -> Result<[T; N], D::Error>
                    where
                        D: Deserializer<'de>,
                        T: Deserialize<'de>,
                    {
                        deserializer.deserialize_tuple(
                            N,
                            ArrayVisitor::<T, N>(std::marker::PhantomData),
                        )
                    }

                    pub mod option {
                        use serde::{Deserialize, Deserializer, Serializer};

                        struct Wrap<T, const N: usize>([T; N]);

                        impl<T: serde::Serialize, const N: usize> serde::Serialize
                            for Wrap<T, N>
                        {
                            fn serialize<S: Serializer>(
                                &self,
                                s: S,
                            ) -> Result<S::Ok, S::Error> {
                                super::serialize(&self.0, s)
                            }
                        }

                        impl<'de, T: Deserialize<'de>, const N: usize> Deserialize<'de>
                            for Wrap<T, N>
                        {
                            fn deserialize<D: Deserializer<'de>>(
                                d: D,
                            ) -> Result<Self, D::Error> {
                                super::deserialize(d).map(Wrap)
                            }
                        }

                        pub fn serialize<S, T, const N: usize>(
                            value: &Option<[T; N]>,
                            serializer: S,
                        ) -> Result<S::Ok, S::Error>
                        where
                            S: Serializer,
                            T: serde::Serialize + Copy,
                        {
                            match value {
                                Some(inner) => serializer.serialize_some(&Wrap(*inner)),
                                None => serializer.serialize_none(),
                            }
                        }

                        pub fn deserialize<'de, D, T, const N: usize>(
                            deserializer: D,
                        ) -> Result<Option<[T; N]>, D::Error>
                        where
                            D: Deserializer<'de>,
                            T: Deserialize<'de>,
                        {
                            Ok(Option::<Wrap<T, N>>::deserialize(deserializer)?
                                .map(|w| w.0))
                        }
                    }
                }

                mod __forgedb_big_bytes {
                    use serde::de::{Error as _, SeqAccess, Visitor};
                    use serde::ser::SerializeTuple;
                    use serde::{Deserialize, Deserializer, Serialize, Serializer};

                    pub fn serialize<S, const N: usize>(
                        value: &[u8; N],
                        serializer: S,
                    ) -> Result<S::Ok, S::Error>
                    where
                        S: Serializer,
                    {
                        let mut tup = serializer.serialize_tuple(N)?;
                        for byte in value.iter() {
                            tup.serialize_element(byte)?;
                        }
                        tup.end()
                    }

                    struct BytesVisitor<const N: usize>;

                    impl<'de, const N: usize> Visitor<'de> for BytesVisitor<N> {
                        type Value = [u8; N];

                        fn expecting(
                            &self,
                            f: &mut std::fmt::Formatter<'_>,
                        ) -> std::fmt::Result {
                            write!(f, "an array of {N} bytes")
                        }

                        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
                        where
                            A: SeqAccess<'de>,
                        {
                            let mut out = [0u8; N];
                            for (i, slot) in out.iter_mut().enumerate() {
                                *slot = seq
                                    .next_element()?
                                    .ok_or_else(|| A::Error::invalid_length(i, &self))?;
                            }
                            if seq.next_element::<u8>()?.is_some() {
                                return Err(A::Error::invalid_length(N + 1, &self));
                            }
                            Ok(out)
                        }
                    }

                    pub fn deserialize<'de, D, const N: usize>(
                        deserializer: D,
                    ) -> Result<[u8; N], D::Error>
                    where
                        D: Deserializer<'de>,
                    {
                        deserializer.deserialize_tuple(N, BytesVisitor::<N>)
                    }

                    struct Wrap<const N: usize>([u8; N]);

                    impl<const N: usize> Serialize for Wrap<N> {
                        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                            serialize(&self.0, s)
                        }
                    }

                    impl<'de, const N: usize> Deserialize<'de> for Wrap<N> {
                        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                            deserialize(d).map(Wrap)
                        }
                    }

                    pub mod option {
                        use serde::{Deserialize, Deserializer, Serializer};

                        pub fn serialize<S, const N: usize>(
                            value: &Option<[u8; N]>,
                            serializer: S,
                        ) -> Result<S::Ok, S::Error>
                        where
                            S: Serializer,
                        {
                            match value {
                                Some(bytes) => serializer.serialize_some(&super::Wrap(*bytes)),
                                None => serializer.serialize_none(),
                            }
                        }

                        pub fn deserialize<'de, D, const N: usize>(
                            deserializer: D,
                        ) -> Result<Option<[u8; N]>, D::Error>
                        where
                            D: Deserializer<'de>,
                        {
                            Ok(Option::<super::Wrap<N>>::deserialize(deserializer)?
                                .map(|w| w.0))
                        }
                    }

                    pub mod array {
                        use serde::de::{Error as _, SeqAccess, Visitor};
                        use serde::ser::SerializeTuple;
                        use serde::{Deserializer, Serializer};

                        pub fn serialize<S, const N: usize, const M: usize>(
                            value: &[[u8; N]; M],
                            serializer: S,
                        ) -> Result<S::Ok, S::Error>
                        where
                            S: Serializer,
                        {
                            let mut tup = serializer.serialize_tuple(M)?;
                            for inner in value.iter() {
                                tup.serialize_element(&super::Wrap(*inner))?;
                            }
                            tup.end()
                        }

                        struct OuterVisitor<const N: usize, const M: usize>;

                        impl<'de, const N: usize, const M: usize> Visitor<'de>
                            for OuterVisitor<N, M>
                        {
                            type Value = [[u8; N]; M];

                            fn expecting(
                                &self,
                                f: &mut std::fmt::Formatter<'_>,
                            ) -> std::fmt::Result {
                                write!(f, "an array of {M} arrays of {N} bytes")
                            }

                            fn visit_seq<A>(
                                self,
                                mut seq: A,
                            ) -> Result<Self::Value, A::Error>
                            where
                                A: SeqAccess<'de>,
                            {
                                let mut out = [[0u8; N]; M];
                                for (i, slot) in out.iter_mut().enumerate() {
                                    let wrapped: super::Wrap<N> = seq
                                        .next_element()?
                                        .ok_or_else(|| A::Error::invalid_length(i, &self))?;
                                    *slot = wrapped.0;
                                }
                                Ok(out)
                            }
                        }

                        pub fn deserialize<'de, D, const N: usize, const M: usize>(
                            deserializer: D,
                        ) -> Result<[[u8; N]; M], D::Error>
                        where
                            D: Deserializer<'de>,
                        {
                            deserializer.deserialize_tuple(M, OuterVisitor::<N, M>)
                        }
                    }
                }
            });
        }

        if Self::schema_needs_f64_key(schema) {
            tokens.extend(quote! {
                fn __forgedb_f64_key(__v: f64) -> u64 {
                    let __v = if __v == 0.0 {
                        0.0
                    } else if __v.is_nan() {
                        f64::NAN
                    } else {
                        __v
                    };
                    let __bits = __v.to_bits();
                    let __mask = ((__bits as i64 >> 63) as u64) | 0x8000_0000_0000_0000;
                    __bits ^ __mask
                }
            });
        }

        if Self::needs_inline_str(schema) {
            tokens.extend(Self::generate_identity_alphabet_helper());
        }
        if Self::needs_inline_len_helper(schema) {
            tokens.extend(Self::generate_inline_len_helper());
        }

        tokens.extend(quote! {
            #[derive(Debug, Clone, PartialEq)]
            pub enum ValidationError {
                Unique { model: &'static str, field: &'static str },
                DanglingReference {
                    model: &'static str,
                    field: &'static str,
                    target: &'static str,
                },
                ReferencedByChildren { model: &'static str, field: &'static str },
                Constraint {
                    model: &'static str,
                    field: &'static str,
                    rule: &'static str,
                    message: String,
                },
                SequenceExhausted { model: &'static str, field: &'static str },
            }

            impl ValidationError {
                pub fn status_code(&self) -> u16 {
                    match self {
                        ValidationError::Unique { .. }
                        | ValidationError::DanglingReference { .. }
                        | ValidationError::ReferencedByChildren { .. } => 409,
                        ValidationError::Constraint { .. } => 422,
                        ValidationError::SequenceExhausted { .. } => 500,
                    }
                }
            }

            impl std::fmt::Display for ValidationError {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    match self {
                        ValidationError::Unique { model, field } => {
                            write!(f, "unique constraint violated on field `{}.{}`", model, field)
                        }
                        ValidationError::DanglingReference { model, field, target } => {
                            write!(
                                f,
                                "field `{}.{}` references a non-existent {}",
                                model, field, target
                            )
                        }
                        ValidationError::ReferencedByChildren { model, field } => {
                            write!(
                                f,
                                "cannot delete: {} rows still reference it via `{}` (on_delete=restrict)",
                                model, field
                            )
                        }
                        ValidationError::Constraint { model, field, rule, message } => {
                            write!(f, "field `{}.{}` violates `{}`: {}", model, field, rule, message)
                        }
                        ValidationError::SequenceExhausted { model, field } => {
                            write!(
                                f,
                                "auto-increment sequence for `{}.{}` is exhausted",
                                model, field
                            )
                        }
                    }
                }
            }

            impl std::error::Error for ValidationError {}
        });

        tokens.extend(quote! {
            #[derive(Debug)]
            pub enum ApplyError {
                Decode(String),
                Validation(ValidationError),
            }

            impl std::fmt::Display for ApplyError {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    match self {
                        ApplyError::Decode(m) => {
                            write!(f, "failed to decode replication frame: {}", m)
                        }
                        ApplyError::Validation(e) => {
                            write!(f, "replicated write rejected: {}", e)
                        }
                    }
                }
            }

            impl std::error::Error for ApplyError {}

            impl From<ValidationError> for ApplyError {
                fn from(e: ValidationError) -> Self {
                    ApplyError::Validation(e)
                }
            }
        });

        tokens.extend(quote! {
            #[derive(Debug)]
            pub enum TxError {
                Validation(ValidationError),
                Io(String),
                Conflict,
                CoordinatorUnavailable(String),
            }

            impl TxError {
                pub fn status_code(&self) -> u16 {
                    match self {
                        TxError::Validation(e) => e.status_code(),
                        TxError::Io(_) => 500,
                        TxError::Conflict => 409,
                        TxError::CoordinatorUnavailable(_) => 503,
                    }
                }
            }

            impl std::fmt::Display for TxError {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    match self {
                        TxError::Validation(e) => write!(f, "transaction rejected: {}", e),
                        TxError::Io(m) => write!(f, "transaction I/O failure: {}", m),
                        TxError::Conflict => write!(f, "transaction conflict (retry)"),
                        TxError::CoordinatorUnavailable(m) => {
                            write!(f, "commit coordinator unavailable: {}", m)
                        }
                    }
                }
            }

            impl std::error::Error for TxError {}

            impl From<ValidationError> for TxError {
                fn from(e: ValidationError) -> Self {
                    TxError::Validation(e)
                }
            }
        });

        for struct_def in &schema.structs {
            let struct_tokens = Self::generate_struct(schema, struct_def);
            tokens.extend(struct_tokens);
        }

        for enum_def in &schema.enums {
            tokens.extend(Self::generate_enum(enum_def)?);
        }

        for model in &schema.models {
            let model_tokens = Self::generate_model(model, schema)?;
            tokens.extend(model_tokens);
        }

        for model in &schema.models {
            tokens.extend(Self::generate_change_event_structs(model));
        }

        for model in &schema.models {
            tokens.extend(Self::generate_live_delta_enum(schema, model));
        }

        let junction_tokens = Self::generate_junction_structs(schema);
        tokens.extend(junction_tokens);

        let db_tokens = Self::generate_database(schema)?;
        tokens.extend(db_tokens);

        let traversal_tokens = Self::generate_traversal_impl(schema);
        tokens.extend(traversal_tokens);

        let validated_writes = Self::generate_validated_writes(schema);
        tokens.extend(validated_writes);

        let transaction_impl = Self::generate_transaction_impl(schema);
        tokens.extend(transaction_impl);

        let shared_db_impl = Self::generate_shared_database_impl(schema);
        tokens.extend(shared_db_impl);

        let reader_traversal_tokens = Self::generate_reader_traversal_impl(schema);
        tokens.extend(reader_traversal_tokens);

        let eager_tokens = Self::generate_eager_load(schema);
        tokens.extend(eager_tokens);

        let coord_tokens = Self::generate_coordinated_client(schema);
        tokens.extend(coord_tokens);

        let syntax_tree = syn::parse_file(&tokens.to_string())
            .map_err(|e| crate::CodegenError::GenerationFailed(format!("Failed to parse generated code: {}", e)))?;

        Ok(prettyplease::unparse(&syntax_tree))
    }

    fn generate_model(model: &forgedb_parser::Model, schema: &Schema) -> Result<TokenStream> {
        let model_name = format_ident!("{}", model.name);
        let storage_name = format_ident!("{}Storage", model.name);

        let fields: Vec<_> = model
            .fields
            .iter()
            .map(|f| Self::model_struct_field(schema, f, true, Self::is_inline_key_field(schema, model, f)))
            .collect();

        let storage_fields = Self::generate_storage_fields(schema, model);

        let storage_inits = Self::generate_storage_inits(model, schema);

        let id_type = Self::id_type_tokens(schema, model);

        let field_validation = Self::generate_field_validation(model);

        let insert_logic = Self::generate_insert_logic(schema, model);

        let mutation_methods = match (
            Self::generate_update_logic(schema, model),
            Self::generate_delete_logic(schema, model),
        ) {
            (Some(update_logic), Some(delete_logic)) => quote! {
                pub fn update(&mut self, id: #id_type, record: #model_name) -> Result<bool, ValidationError> {
                    #update_logic
                }

                pub fn delete(&mut self, id: #id_type) -> bool {
                    #delete_logic
                }
            },
            _ => quote! {},
        };

        let apply_method = Self::generate_apply_method(model).unwrap_or_else(|| quote! {});
        let commit_method = Self::generate_commit_method(schema, model);

        let row_index_ident = format_ident!("row_index");
        let id_read_at_row =
            Self::generate_id_read_expr(schema, model, &quote! { self }, &row_index_ident);

        let snapshot_accessors = Self::generate_snapshot_accessors(
            &id_type,
            &model_name,
            id_read_at_row.as_ref(),
        );

        let read_at_logic = Self::generate_read_at_logic(schema, model);

        let (projection_structs, projection_methods) =
            Self::generate_projections(schema, model, &id_type, id_read_at_row.as_ref());

        let (scan_struct, scan_methods) = Self::generate_list_scan(schema, model);

        let index_lookups = Self::generate_index_lookups(schema, model);

        let columnar_export = Self::generate_columnar_export(schema, model);

        let rehydrate_logic = Self::generate_rehydrate_logic(schema, model);

        let recover_method = Self::generate_recover_method(schema, model);

        let checkpoint_method = Self::generate_checkpoint_method(schema, model);

        let compact_method = Self::generate_compact_method(model);

        let txn_storage_methods = Self::generate_txn_storage_methods(schema, model);

        let write_manifest = Self::generate_write_manifest(schema, model);
        let autoseq_methods = Self::generate_autoseq_methods(schema, model);

        let autoseq_floor = {
            let loads: Vec<TokenStream> = Self::sequence_auto_fields(model)
                .iter()
                .map(|f| {
                    let seq = Self::autoseq_field_ident(f);
                    let key = &f.name;
                    quote! {
                        if let Some(&__floor) = __persisted.get(#key) {
                            db.#seq.fetch_max(__floor, std::sync::atomic::Ordering::SeqCst);
                        }
                    }
                })
                .collect();
            if loads.is_empty() {
                quote! {}
            } else {
                let manifest_rel = format!("{}/manifest.json", Self::to_snake_case(&model.name));
                quote! {
                    {
                        let __persisted = forgedb_storage::Manifest::load_from(&root.join(#manifest_rel))
                            .map(|m| m.auto_sequences)
                            .unwrap_or_default();
                        #(#loads)*
                    }
                }
            }
        };

        let reader_name = format_ident!("{}StorageReader", model.name);
        let reader_fields = Self::generate_reader_storage_fields(schema, model);
        let reader_inits = Self::generate_reader_inits(schema, model);
        let reader_index_probes = Self::generate_index_probes(schema, model, false);

        let to_schema_derive = Self::to_schema_derive();
        let tokens = quote! {
            #[repr(C)]
            #[derive(Debug, Clone, Serialize, Deserialize #to_schema_derive)]
            pub struct #model_name {
                #(#fields),*
            }

            #projection_structs

            #scan_struct

            pub struct #storage_name {
                #storage_fields
            }

            impl #storage_name {
                pub fn new() -> Self {
                    Self::new_at(std::path::Path::new("."))
                }

                pub fn new_at(root: &std::path::Path) -> Self {
                    let mut db = Self {
                        #storage_inits
                    };
                    db.recover_from_wal();
                    #rehydrate_logic
                    #autoseq_floor
                    let _ = db.write_manifest(root);
                    db
                }

                fn new_at_no_rehydrate(root: &std::path::Path) -> Self {
                    let mut db = Self {
                        #storage_inits
                    };
                    db.recover_from_wal();
                    let _ = db.write_manifest(root);
                    db
                }

                #write_manifest

                #autoseq_methods

                pub fn attach_changefeed(&mut self, feed: forgedb_changefeed::ChangeFeed) {
                    self.changefeed = Some(feed);
                }

                pub fn attach_broker(
                    &mut self,
                    broker: Option<std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>>,
                ) {
                    self.broker = broker;
                }

                pub fn insert(&mut self, record: #model_name) -> Result<#id_type, ValidationError> {
                    #insert_logic
                }

                #mutation_methods

                #apply_method

                #commit_method

                #recover_method

                #checkpoint_method

                #compact_method

                #txn_storage_methods

                pub fn read_at(&self, row_index: usize) -> Option<#model_name> {
                    #read_at_logic
                }

                pub fn get(&self, id: #id_type) -> Option<#model_name> {
                    let row_index = *self.id_to_row.get(&id)?;
                    self.read_at(row_index)
                }

                pub fn row_count(&self) -> usize {
                    self.row_count
                }

                pub fn snapshot(&self) -> forgedb_storage::Snapshot {
                    forgedb_storage::Snapshot::new(self.row_count)
                }

                #snapshot_accessors

                pub fn all(&self) -> Vec<#model_name> {
                    let mut records = Vec::new();
                    for id in self.id_to_row.keys() {
                        if let Some(record) = self.get(*id) {
                            records.push(record);
                        }
                    }
                    records
                }

                #index_lookups

                #columnar_export

                #projection_methods

                #scan_methods

                pub fn reader(&self) -> #reader_name {
                    #reader_name {
                        #reader_inits
                    }
                }
            }

            pub struct #reader_name {
                #reader_fields
            }

            impl #reader_name {
                pub fn read_at(&self, row_index: usize) -> Option<#model_name> {
                    #read_at_logic
                }

                #snapshot_accessors

                #reader_index_probes
            }

            #field_validation
        };

        Ok(tokens)
    }

    fn generate_change_event_structs(model: &forgedb_parser::Model) -> TokenStream {
        let model_name = format_ident!("{}", model.name);
        let field_name = format_ident!("{}", Self::to_snake_case(&model.name));

        let variants = ["Inserted", "Updated", "Deleted"];

        let structs = variants.iter().map(|suffix| {
            let event_name = format_ident!("{}{}", model.name, suffix);
            let to_schema_derive = Self::to_schema_derive();
            quote! {
                #[derive(Debug, Clone, Serialize, Deserialize #to_schema_derive)]
                pub struct #event_name {
                    pub #field_name: #model_name,
                }
            }
        });

        quote! { #(#structs)* }
    }

    fn generate_live_delta_enum(schema: &Schema, model: &forgedb_parser::Model) -> TokenStream {
        let model_name = format_ident!("{}", model.name);
        let delta_name = format_ident!("{}LiveDelta", model.name);
        let id_type = Self::id_type_tokens(schema, model);
        let id_schema_attr = model.identity_field()
            .and_then(|f| Self::schema_value_type(schema, &f.field_type, true))
            .map(|vt| Self::schema_attr(quote! { #[schema(value_type = #vt)] }))
            .unwrap_or_default();
        let to_schema_derive = Self::to_schema_derive();
        quote! {
            #[derive(Debug, Clone, Serialize, Deserialize #to_schema_derive)]
            #[serde(tag = "kind", rename_all = "lowercase")]
            pub enum #delta_name {
                Init { rows: Vec<#model_name> },
                Added { row: #model_name },
                Updated { row: #model_name },
                Removed { #id_schema_attr id: #id_type },
            }
        }
    }

    fn generate_snapshot_accessors(
        id_type: &TokenStream,
        model_name: &proc_macro2::Ident,
        id_read: Option<&TokenStream>,
    ) -> TokenStream {
        match id_read {
            Some(_id_read) => quote! {
                pub fn get_at(&self, snap: &forgedb_storage::Snapshot, id: #id_type) -> Option<#model_name> {
                    let watermark = snap.watermark();
                    let versions = self.id_versions.get(&id)?;
                    let pos = versions.partition_point(|&r| r < watermark);
                    if pos == 0 {
                        return None;
                    }
                    self.read_at(versions[pos - 1])
                }

                pub fn all_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<#model_name> {
                    let watermark = snap.watermark();
                    let mut rows = Vec::new();
                    for versions in self.id_versions.values() {
                        let pos = versions.partition_point(|&r| r < watermark);
                        if pos == 0 {
                            continue;
                        }
                        rows.push(versions[pos - 1]);
                    }
                    rows.sort_unstable();
                    let mut records = Vec::new();
                    for row in rows {
                        if let Some(record) = self.read_at(row) {
                            records.push(record);
                        }
                    }
                    records
                }
            },
            None => quote! {
                pub fn get_at(&self, snap: &forgedb_storage::Snapshot, id: #id_type) -> Option<#model_name> {
                    let row_index = *self.id_to_row.get(&id)?;
                    if !snap.visible(row_index) {
                        return None;
                    }
                    self.read_at(row_index)
                }

                pub fn all_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<#model_name> {
                    let mut records = Vec::new();
                    for row_index in 0..snap.watermark() {
                        if let Some(record) = self.read_at(row_index) {
                            records.push(record);
                        }
                    }
                    records
                }
            },
        }
    }

    const FK_RESOLVE_DEPTH: u32 = 16;

    pub(crate) fn fk_backing_type(
        schema: &Schema,
        field_type: &forgedb_parser::FieldType,
    ) -> Option<forgedb_parser::FieldType> {
        Self::fk_backing_type_bounded(schema, field_type, Self::FK_RESOLVE_DEPTH)
    }

    fn fk_backing_type_bounded(
        schema: &Schema,
        field_type: &forgedb_parser::FieldType,
        depth: u32,
    ) -> Option<forgedb_parser::FieldType> {
        use forgedb_parser::{FieldType, RelationType};

        if depth == 0 {
            return None;
        }
        let (target, optional) = match field_type {
            FieldType::Relation(RelationType::RequiredReference(t)) => (t, false),
            FieldType::Relation(RelationType::OptionalReference(t)) => (t, true),
            _ => return None,
        };
        let identity = schema.find_model(target)?.identity_field()?;
        let key = match &identity.field_type {
            it @ FieldType::Relation(_) => Self::fk_backing_type_bounded(schema, it, depth - 1)?,
            other => other.clone(),
        };
        let key = match key {
            FieldType::Nullable(inner) => *inner,
            k => k,
        };
        Some(if optional {
            FieldType::Nullable(Box::new(key))
        } else {
            key
        })
    }

    pub(crate) fn resolved_type(
        schema: &Schema,
        field_type: &forgedb_parser::FieldType,
    ) -> forgedb_parser::FieldType {
        use forgedb_parser::{FieldType, RelationType};
        match field_type {
            FieldType::Relation(RelationType::RequiredReference(_)) => {
                Self::fk_backing_type(schema, field_type).unwrap_or(FieldType::Uuid)
            }
            FieldType::Relation(RelationType::OptionalReference(_)) => {
                Self::fk_backing_type(schema, field_type)
                    .unwrap_or_else(|| FieldType::Nullable(Box::new(FieldType::Uuid)))
            }
            other => other.clone(),
        }
    }

    pub(crate) fn identity_type(
        schema: &Schema,
        model: &forgedb_parser::Model,
    ) -> Option<forgedb_parser::FieldType> {
        let identity = model.identity_field()?;
        match &identity.field_type {
            it @ forgedb_parser::FieldType::Relation(_) => Self::fk_backing_type(schema, it),
            other => Some(other.clone()),
        }
    }

    pub(crate) fn junction_key_type(
        schema: &Schema,
        model: &forgedb_parser::Model,
    ) -> Option<forgedb_parser::FieldType> {
        let ty = Self::identity_type(schema, model)?;
        ty.is_junction_key().then_some(ty)
    }

    pub(crate) fn valid_m2m(schema: &Schema) -> Vec<forgedb_parser::ast::ManyToManyRelation> {
        schema
            .detect_many_to_many_relations()
            .into_iter()
            .filter(|m| {
                schema
                    .find_model(&m.model1)
                    .and_then(|md| Self::junction_key_type(schema, md))
                    .is_some()
                    && schema
                        .find_model(&m.model2)
                        .and_then(|md| Self::junction_key_type(schema, md))
                        .is_some()
            })
            .collect()
    }

    fn junction_key_width(ty: &forgedb_parser::FieldType) -> usize {
        use forgedb_parser::FieldType as FT;
        match ty {
            FT::U32 | FT::I32 => 4,
            FT::U64 | FT::I64 | FT::Timestamp(_) => 8,
            FT::StringN { chars, .. } => *chars as usize,
            _ => 16,
        }
    }

    fn junction_key_pair(
        schema: &Schema,
        m: &forgedb_parser::ast::ManyToManyRelation,
    ) -> (forgedb_parser::FieldType, forgedb_parser::FieldType) {
        let of = |name: &str| {
            schema
                .find_model(name)
                .and_then(|md| Self::junction_key_type(schema, md))
                .unwrap_or(forgedb_parser::FieldType::Uuid)
        };
        (of(&m.model1), of(&m.model2))
    }

    pub(crate) fn junction_key_idents(
        schema: &Schema,
        m: &forgedb_parser::ast::ManyToManyRelation,
    ) -> (TokenStream, TokenStream) {
        let (lt, rt) = Self::junction_key_pair(schema, m);
        (
            Self::key_type_ident(schema, &lt),
            Self::key_type_ident(schema, &rt),
        )
    }

    fn junction_append_expr(
        schema: &Schema,
        ty: &forgedb_parser::FieldType,
        col: &TokenStream,
        val: &TokenStream,
    ) -> TokenStream {
        match ty {
            forgedb_parser::FieldType::Uuid => quote! { #col.append_uuid(*#val.as_bytes()) },
            forgedb_parser::FieldType::Timestamp(_) => {
                quote! { #col.append_timestamp(i64::from(#val)) }
            }
            forgedb_parser::FieldType::StringN { chars, .. } => {
                let n = *chars as usize;
                quote! {
                    {
                        let mut __buf = [0u8; #n];
                        let __b = #val.as_bytes();
                        __buf[..__b.len()].copy_from_slice(__b);
                        #col.append_bytes(&__buf)
                    }
                }
            }
            other => {
                let m = Self::get_append_method(schema, other);
                quote! { #col.#m(#val) }
            }
        }
    }

    fn junction_read_expr(
        schema: &Schema,
        ty: &forgedb_parser::FieldType,
        col: &TokenStream,
        row: &TokenStream,
    ) -> TokenStream {
        match ty {
            forgedb_parser::FieldType::Uuid => quote! {
                Uuid::from_bytes(#col.read_uuid(#row).expect("Failed to read link"))
            },
            forgedb_parser::FieldType::Timestamp(_) => quote! {
                Timestamp::from(#col.read_timestamp(#row).expect("Failed to read link"))
            },
            ty @ forgedb_parser::FieldType::StringN { .. } => {
                let key_ty = Self::key_type_ident(schema, ty);
                quote! {
                    {
                        let __raw = #col.read_bytes(#row).expect("Failed to read link");
                        <#key_ty>::try_from(
                            std::str::from_utf8(&__raw)
                                .expect("junction key column holds UTF-8")
                                .trim_end_matches('\0'),
                        )
                        .expect("junction key column is the key width")
                    }
                }
            }
            other => {
                let m = Self::get_read_method(schema, other);
                quote! { #col.#m(#row).expect("Failed to read link") }
            }
        }
    }

    fn junction_frame_decode(
        schema: &Schema,
        ty: &forgedb_parser::FieldType,
        slice: &TokenStream,
    ) -> TokenStream {
        match ty {
            forgedb_parser::FieldType::Uuid => quote! {
                {
                    let mut __b = [0u8; 16];
                    __b.copy_from_slice(#slice);
                    Uuid::from_bytes(__b)
                }
            },
            forgedb_parser::FieldType::Timestamp(_) => quote! {
                Timestamp::from(i64::from_le_bytes(
                    (#slice).try_into().expect("junction frame slot is the key width"),
                ))
            },
            ty @ forgedb_parser::FieldType::StringN { .. } => {
                let key_ty = Self::key_type_ident(schema, ty);
                quote! {
                    <#key_ty>::try_from(
                        std::str::from_utf8(#slice)
                            .expect("junction frame slot holds UTF-8")
                            .trim_end_matches('\0'),
                    )
                    .expect("junction frame slot is the key width")
                }
            }
            other => {
                let ident = Self::key_type_ident(schema, other);
                quote! {
                    <#ident>::from_le_bytes(
                        (#slice).try_into().expect("junction frame slot is the key width"),
                    )
                }
            }
        }
    }

    fn junction_frame_stmt(
        ty: &forgedb_parser::FieldType,
        buf: &TokenStream,
        val: &TokenStream,
    ) -> TokenStream {
        match ty {
            forgedb_parser::FieldType::Uuid => {
                quote! { #buf.extend_from_slice(#val.as_bytes()); }
            }
            forgedb_parser::FieldType::Timestamp(_) => {
                quote! { #buf.extend_from_slice(&i64::from(#val).to_le_bytes()); }
            }
            forgedb_parser::FieldType::StringN { chars, .. } => {
                let n = *chars as usize;
                quote! {
                    {
                        let mut __k = [0u8; #n];
                        let __b = #val.as_bytes();
                        __k[..__b.len()].copy_from_slice(__b);
                        #buf.extend_from_slice(&__k);
                    }
                }
            }
            _ => quote! { #buf.extend_from_slice(&#val.to_le_bytes()); },
        }
    }

    fn junction_struct_ident(m: &forgedb_parser::ast::ManyToManyRelation) -> proc_macro2::Ident {
        format_ident!("{}{}Link", m.model1, m.model2)
    }

    pub(crate) fn junction_field_ident(
        m: &forgedb_parser::ast::ManyToManyRelation,
    ) -> proc_macro2::Ident {
        format_ident!(
            "{}_{}_link",
            Self::to_snake_case(&m.model1),
            Self::to_snake_case(&m.model2)
        )
    }

    pub(crate) fn id_type_tokens(schema: &Schema, model: &forgedb_parser::Model) -> TokenStream {
        match model.identity_field() {
            Some(f) => Self::key_type_ident(schema, &f.field_type),
            None => quote! { Uuid },
        }
    }

    pub(crate) fn identity_field_name(model: &forgedb_parser::Model) -> Option<&str> {
        model.identity_field().map(|f| f.name.as_str())
    }

    pub(crate) fn is_identity(model: &forgedb_parser::Model, field: &forgedb_parser::Field) -> bool {
        Self::identity_field_name(model) == Some(field.name.as_str())
    }

    pub(crate) fn arrow_export_format(
        schema: &Schema,
        ft: &forgedb_parser::FieldType,
    ) -> Option<&'static str> {
        let ft = &Self::resolved_type(schema, ft);
        use forgedb_parser::{FieldType, RelationType};
        match ft {
            FieldType::I32 => Some("i"),
            FieldType::I64 => Some("l"),
            FieldType::U32 => Some("I"),
            FieldType::U64 => Some("L"),
            FieldType::F64 => Some("g"),
            FieldType::Timestamp(_) => Some("l"),
            FieldType::Uuid | FieldType::Relation(RelationType::RequiredReference(_)) => {
                Some("w:16")
            }
            _ => None,
        }
    }

    fn generate_columnar_export(schema: &Schema, model: &forgedb_parser::Model) -> TokenStream {
        let col_gathers: Vec<TokenStream> = model
            .fields
            .iter()
            .filter(|f| Self::arrow_export_format(schema, &f.field_type).is_some())
            .map(|f| {
                let field_col_name = format_ident!("{}_col", f.name);
                let method = format_ident!("export_col_{}", f.name);
                quote! {
                    pub fn #method(&self, indices: &[usize]) -> std::io::Result<forgedb_storage::ColumnExport> {
                        self.#field_col_name.export(indices)
                    }
                }
            })
            .collect();

        if col_gathers.is_empty() {
            return quote! {};
        }

        quote! {
            pub fn export_live_indices(&self) -> Vec<usize> {
                let mut indices = Vec::new();
                for &row in self.id_to_row.values() {
                    if !self.tombstones.is_deleted(row).unwrap_or(true) {
                        indices.push(row);
                    }
                }
                indices.sort_unstable();
                indices
            }

            #(#col_gathers)*
        }
    }

    fn generate_struct(schema: &Schema, struct_def: &forgedb_parser::Struct) -> TokenStream {
        let struct_name = format_ident!("{}", struct_def.name);

        let fields: Vec<_> = struct_def
            .fields
            .iter()
            .map(|field| {
                let field_name = format_ident!("{}", field.name);
                let field_type = Self::map_field_type_ident(schema, &field.field_type);

                let (schema_attr, serde_attr) = if let Some(vt) =
                    Self::timestamp_schema_value_type(schema, &field.field_type)
                {
                    (Self::schema_attr(quote! { #[schema(value_type = #vt)] }), quote! {})
                } else if let Some(attrs) = Self::big_array_attrs(&field.field_type) {
                    attrs
                } else {
                    (quote! {}, quote! {})
                };

                if field.is_nullable() {
                    quote! { #schema_attr #serde_attr pub #field_name: Option<#field_type> }
                } else {
                    quote! { #schema_attr #serde_attr pub #field_name: #field_type }
                }
            })
            .collect();

        let to_schema_derive = Self::to_schema_derive();
        quote! {
            #[repr(C)]
            #[derive(Debug, Clone, PartialEq, Serialize, Deserialize #to_schema_derive)]
            pub struct #struct_name {
                #(#fields),*
            }
        }
    }

    fn generate_enum(enum_def: &forgedb_parser::EnumDef) -> Result<TokenStream> {
        if enum_def.variants.len() > 256 {
            return Err(CodegenError::InvalidSchema(format!(
                "enum '{}' has {} variants; a maximum of 256 is supported (the stored \
                 discriminant is a single byte)",
                enum_def.name,
                enum_def.variants.len()
            )));
        }

        let enum_name = format_ident!("{}", enum_def.name);

        let variant_idents: Vec<_> = enum_def
            .variants
            .iter()
            .map(|v| format_ident!("{}", v))
            .collect();

        let to_arms: Vec<_> = variant_idents
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let disc = i as u8;
                quote! { #enum_name::#v => #disc }
            })
            .collect();

        let from_arms: Vec<_> = variant_idents
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let disc = i as u8;
                quote! { #disc => #enum_name::#v }
            })
            .collect();

        let name_arms: Vec<_> = variant_idents
            .iter()
            .zip(enum_def.variants.iter())
            .map(|(ident, name)| quote! { #enum_name::#ident => #name })
            .collect();

        let name_str = &enum_def.name;

        let to_schema_derive = Self::to_schema_derive();
        Ok(quote! {
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize #to_schema_derive)]
            pub enum #enum_name {
                #(#variant_idents),*
            }

            impl #enum_name {
                fn __to_u8(&self) -> u8 {
                    match self {
                        #(#to_arms),*
                    }
                }

                fn __as_str(&self) -> &'static str {
                    match self {
                        #(#name_arms),*
                    }
                }

                fn __from_u8(__b: u8) -> #enum_name {
                    match __b {
                        #(#from_arms,)*
                        other => panic!(
                            "invalid {} discriminant byte {} (out of range)",
                            #name_str, other
                        ),
                    }
                }
            }
        })
    }

    fn timestamp_key_field(model: &forgedb_parser::Model) -> Option<&forgedb_parser::Field> {
        model.fields.iter().find(|f| {
            f.name == "id"
                && f.auto_generate
                && matches!(f.field_type, forgedb_parser::FieldType::Timestamp(_))
        })
    }

    fn sequence_auto_fields(model: &forgedb_parser::Model) -> Vec<&forgedb_parser::Field> {
        let mut fields = Self::integer_auto_fields(model);
        fields.extend(Self::timestamp_key_field(model));
        fields
    }

    fn autoseq_to_u64(field: &forgedb_parser::Field, expr: TokenStream) -> TokenStream {
        if matches!(field.field_type, forgedb_parser::FieldType::Timestamp(_)) {
            quote! { (#expr).as_micros().max(0) as u64 }
        } else {
            quote! { (#expr) as u64 }
        }
    }

    fn integer_auto_fields(model: &forgedb_parser::Model) -> Vec<&forgedb_parser::Field> {
        model
            .fields
            .iter()
            .filter(|f| {
                f.auto_generate
                    && matches!(
                        f.field_type,
                        forgedb_parser::FieldType::U32 | forgedb_parser::FieldType::U64
                    )
            })
            .collect()
    }

    fn bare_integer_auto_fields(model: &forgedb_parser::Model) -> Vec<&forgedb_parser::Field> {
        let identity = model.identity_field().map(|f| f.name.as_str());
        Self::integer_auto_fields(model)
            .into_iter()
            .filter(|f| !f.unique && Some(f.name.as_str()) != identity)
            .collect()
    }

    fn schema_has_bare_integer_auto(schema: &Schema) -> bool {
        schema.models.iter().any(|m| !Self::bare_integer_auto_fields(m).is_empty())
    }

    fn generate_sequence_claim_staging(model: &forgedb_parser::Model) -> TokenStream {
        let mtag = model.name.as_str();
        let stmts: Vec<TokenStream> = Self::bare_integer_auto_fields(model)
            .iter()
            .map(|f| {
                let fident = format_ident!("{}", f.name);
                let fname = f.name.as_str();
                quote! {
                    self.staged_sequence_keys.insert((
                        #mtag,
                        #fname,
                        record.#fident as u64,
                    ));
                }
            })
            .collect();
        quote! { #(#stmts)* }
    }

    fn generate_sequence_fastforward(schema: &Schema) -> TokenStream {
        if !Self::schema_has_bare_integer_auto(schema) {
            return quote! {};
        }
        quote! {
            {
                fn __forgedb_ws_parts(key: &[u8]) -> Vec<&[u8]> {
                    let mut parts: Vec<&[u8]> = Vec::new();
                    let mut i = 0usize;
                    while i + 4 <= key.len() {
                        let n = u32::from_le_bytes([
                            key[i], key[i + 1], key[i + 2], key[i + 3],
                        ]) as usize;
                        i += 4;
                        if i + n > key.len() {
                            return Vec::new();
                        }
                        parts.push(&key[i..i + n]);
                        i += n;
                    }
                    parts
                }

                if let forgedb_txn::CommitOutcome::Conflict { key: __ckey } = &__outcome {
                    let __parts = __forgedb_ws_parts(__ckey);
                    if __parts.len() == 4 && __parts[0] == b"s" {
                        let mut __db = __inner.write().unwrap();
                        __db.__peer_refresh();
                    }
                }
            }
        }
    }

    fn sequence_claim_plumbing(
        schema: &Schema,
        recv: &TokenStream,
        sink: &TokenStream,
        boxed: bool,
    ) -> (TokenStream, TokenStream, TokenStream) {
        if !Self::schema_has_bare_integer_auto(schema) {
            return (quote! {}, quote! {}, quote! {});
        }
        let field = quote! {
            staged_sequence_keys:
                std::collections::BTreeSet<(&'static str, &'static str, u64)>,
        };
        let init = quote! {
            staged_sequence_keys: std::collections::BTreeSet::new(),
        };
        let push = if boxed {
            quote! {
                #sink.push(
                    __forgedb_ws_key(&[
                        b"s",
                        __mtag.as_bytes(),
                        __fname.as_bytes(),
                        &__val.to_le_bytes(),
                    ])
                    .into_boxed_slice(),
                );
            }
        } else {
            quote! {
                #sink.push(__forgedb_ws_key(&[
                    b"s",
                    __mtag.as_bytes(),
                    __fname.as_bytes(),
                    &__val.to_le_bytes(),
                ]));
            }
        };
        let ws = quote! {
            for (__mtag, __fname, __val) in &#recv.staged_sequence_keys {
                let __val: u64 = *__val;
                #push
            }
        };
        (field, init, ws)
    }

    fn autoseq_field_ident(field: &forgedb_parser::Field) -> proc_macro2::Ident {
        format_ident!("__autoseq_{}", field.name)
    }

    fn autoseq_alloc_ident(field: &forgedb_parser::Field) -> proc_macro2::Ident {
        format_ident!("__alloc_{}", field.name)
    }

    fn indexed_fields(model: &forgedb_parser::Model) -> Vec<&forgedb_parser::Field> {
        let Some(identity) = model.identity_field().map(|f| f.name.as_str()) else {
            return Vec::new();
        };
        model
            .fields
            .iter()
            .filter(|f| {
                let is_fk = matches!(
                    f.field_type,
                    forgedb_parser::FieldType::Relation(
                        forgedb_parser::RelationType::RequiredReference(_)
                            | forgedb_parser::RelationType::OptionalReference(_),
                    )
                );
                let is_explicit = (f.indexed || f.unique)
                    && f.name != identity
                    && Self::is_filterable_scalar(&f.field_type);
                is_explicit || is_fk
            })
            .collect()
    }

    fn is_filterable_scalar(field_type: &forgedb_parser::FieldType) -> bool {
        use forgedb_parser::FieldType;
        match field_type {
            FieldType::U32
            | FieldType::U64
            | FieldType::I32
            | FieldType::I64
            | FieldType::F64
            | FieldType::Bool
            | FieldType::Uuid
            | FieldType::Timestamp(_)
            | FieldType::String
            | FieldType::StringN { .. }
            | FieldType::Decimal
            | FieldType::Enum(_)
            | FieldType::Bytes(_) => true,
            FieldType::Nullable(inner) => Self::is_filterable_scalar(inner),
            _ => false,
        }
    }

    fn is_numeric_type(field_type: &forgedb_parser::FieldType) -> bool {
        use forgedb_parser::FieldType;
        match field_type {
            FieldType::U32
            | FieldType::U64
            | FieldType::I32
            | FieldType::I64
            | FieldType::F64
            | FieldType::Decimal => true,
            FieldType::Nullable(inner) => Self::is_numeric_type(inner),
            _ => false,
        }
    }

    fn constraint_bound(c: &forgedb_parser::ast::Constraint) -> Option<(BoundLiteral, bool)> {
        use forgedb_parser::ast::ConstraintParam as P;
        fn literal(p: &P) -> Option<BoundLiteral> {
            match p {
                P::Number(n) => Some(BoundLiteral::Int(*n)),
                P::Fractional(s) => Some(BoundLiteral::Frac(s.clone())),
                _ => None,
            }
        }
        c.params.iter().find_map(|p| match p {
            P::Exclusive { value, .. } => literal(value).map(|l| (l, true)),
            other => literal(other).map(|l| (l, false)),
        })
    }

    fn numeric_bound_operands(
        field_type: &forgedb_parser::FieldType,
        bound: &BoundLiteral,
    ) -> Option<(TokenStream, TokenStream)> {
        use forgedb_parser::FieldType;
        match (field_type, bound) {
            (
                FieldType::U32 | FieldType::U64 | FieldType::I32 | FieldType::I64,
                BoundLiteral::Int(n),
            ) => Some((quote! { (*__v as i128) }, quote! { (#n as i128) })),
            (FieldType::F64, BoundLiteral::Int(n)) => {
                Some((quote! { (*__v as f64) }, quote! { (#n as f64) }))
            }
            (FieldType::Decimal, BoundLiteral::Int(n)) => Some((
                quote! { (*__v) },
                quote! { rust_decimal::Decimal::from(#n) },
            )),
            (FieldType::F64, BoundLiteral::Frac(lex)) => {
                let v: f64 = lex.parse().ok()?;
                let lit = proc_macro2::Literal::f64_suffixed(v);
                Some((quote! { (*__v as f64) }, quote! { #lit }))
            }
            (FieldType::Decimal, BoundLiteral::Frac(lex)) => {
                let (mantissa, scale) = BoundLiteral::decimal_parts(lex)?;
                Some((
                    quote! { (*__v) },
                    quote! { rust_decimal::Decimal::from_i128_with_scale(#mantissa, #scale) },
                ))
            }
            (
                FieldType::U32 | FieldType::U64 | FieldType::I32 | FieldType::I64,
                BoundLiteral::Frac(_),
            ) => None,
            (FieldType::Nullable(inner), _) => Self::numeric_bound_operands(inner, bound),
            _ => None,
        }
    }

    fn constraint_numbers(c: &forgedb_parser::ast::Constraint) -> Vec<i64> {
        c.params
            .iter()
            .filter_map(|p| match p {
                forgedb_parser::ast::ConstraintParam::Number(n) => Some(*n),
                _ => None,
            })
            .collect()
    }

    fn constraint_named_number(c: &forgedb_parser::ast::Constraint, want: &str) -> Option<i64> {
        c.params.iter().find_map(|p| match p {
            forgedb_parser::ast::ConstraintParam::Named { name, value } if name == want => {
                match value.as_ref() {
                    forgedb_parser::ast::ConstraintParam::Number(n) => Some(*n),
                    _ => None,
                }
            }
            _ => None,
        })
    }

    fn constraint_first_string(c: &forgedb_parser::ast::Constraint) -> Option<&str> {
        c.params.iter().find_map(|p| match p {
            forgedb_parser::ast::ConstraintParam::String(s) => Some(s.as_str()),
            _ => None,
        })
    }

    fn generate_field_validation(model: &forgedb_parser::Model) -> TokenStream {
        let fn_name = format_ident!("validate_{}", Self::to_snake_case(&model.name));
        let model_name = format_ident!("{}", model.name);
        let mtag = model.name.as_str();

        let pattern_statics = std::cell::RefCell::new(Vec::<TokenStream>::new());

        let field_blocks: Vec<_> = model
            .fields
            .iter()
            .filter_map(|field| {
                let is_string = Self::is_string_semantic(&field.field_type);
                let is_numeric = Self::is_numeric_type(&field.field_type);
                let fname_str = field.name.as_str();

                let mut checks: Vec<TokenStream> = Vec::new();

                if let Some((chars, exact)) = Self::inline_string_params(&field.field_type) {
                    let n = chars as usize;
                    if Self::is_identity(model, field) {
                        checks.push(quote! {
                            if __v.is_empty() {
                                return Err(ValidationError::Constraint {
                                    model: #mtag, field: #fname_str, rule: "identity_empty",
                                    message: "a key cannot be the empty string — \
                                              its URL would address the collection, \
                                              not a row"
                                        .to_string(),
                                });
                            }
                        });
                        let msg = "must consist only of characters legal unencoded in a URL                                    path segment (RFC 3986 pchar, excluding %), so the segment                                    is byte-identical to the key";
                        checks.push(quote! {
                            if let Some((__i, __c)) = __v
                                .char_indices()
                                .find(|(_, __c)| !__forgedb_identity_char_ok(*__c))
                            {
                                return Err(ValidationError::Constraint {
                                    model: #mtag, field: #fname_str, rule: "identity_alphabet",
                                    message: format!(
                                        "{} — found {:?} at byte {}",
                                        #msg, __c, __i,
                                    ),
                                });
                            }
                        });
                    } else if !Self::is_utf8_field(field) {
                        let msg = format!(
                            "must contain only ASCII characters (add @utf8 to this field to \
                             allow the rest of Unicode, at four bytes per character)"
                        );
                        checks.push(quote! {
                            if let Some((__i, __c)) =
                                __v.char_indices().find(|(_, __c)| !__c.is_ascii())
                            {
                                return Err(ValidationError::Constraint {
                                    model: #mtag, field: #fname_str, rule: "ascii",
                                    message: format!(
                                        "{} — found {:?} at byte {}",
                                        #msg, __c, __i,
                                    ),
                                });
                            }
                        });
                    }
                    if exact {
                        let msg = format!("must be exactly {n} characters");
                        checks.push(quote! {
                            if __v.chars().count() != #n {
                                return Err(ValidationError::Constraint {
                                    model: #mtag, field: #fname_str, rule: "string_n",
                                    message: #msg.to_string(),
                                });
                            }
                        });
                    } else {
                        let msg = format!("must be at most {n} characters");
                        checks.push(quote! {
                            if __v.chars().count() > #n {
                                return Err(ValidationError::Constraint {
                                    model: #mtag, field: #fname_str, rule: "string_n",
                                    message: #msg.to_string(),
                                });
                            }
                        });
                    }
                }

                for c in &field.constraints {
                    match c.name.as_str() {
                        "min" | "max" if is_numeric => {
                            if let Some((bound, exclusive)) = Self::constraint_bound(c)
                                && let Some((lhs, rhs)) =
                                    Self::numeric_bound_operands(&field.field_type, &bound)
                            {
                                let is_min = c.name == "min";
                                let cmp = match (is_min, exclusive) {
                                    (true, false) => quote! { < },
                                    (true, true) => quote! { <= },
                                    (false, false) => quote! { > },
                                    (false, true) => quote! { >= },
                                };
                                let rel = match (is_min, exclusive) {
                                    (true, false) => ">=",
                                    (true, true) => ">",
                                    (false, false) => "<=",
                                    (false, true) => "<",
                                };
                                let rule = if is_min { "min" } else { "max" };
                                let msg = format!("must be {rel} {}", bound.render());
                                checks.push(quote! {
                                    if #lhs #cmp #rhs {
                                        return Err(ValidationError::Constraint {
                                            model: #mtag, field: #fname_str, rule: #rule,
                                            message: #msg.to_string(),
                                        });
                                    }
                                });
                            }
                        }
                        "length" if is_string => {
                            let named_min = Self::constraint_named_number(c, "min");
                            let named_max = Self::constraint_named_number(c, "max");
                            let nums = Self::constraint_numbers(c);

                            let (min, max, exact) = if named_min.is_some() || named_max.is_some() {
                                (named_min, named_max, None)
                            } else if nums.len() == 1 {
                                (None, None, Some(nums[0]))
                            } else if nums.len() >= 2 {
                                (Some(nums[0]), Some(nums[1]), None)
                            } else {
                                (None, None, None)
                            };

                            match (min, max, exact) {
                                (_, _, Some(n)) => {
                                    let msg = format!("length must be exactly {n}");
                                    checks.push(quote! {
                                        if __v.chars().count() != #n as usize {
                                            return Err(ValidationError::Constraint {
                                                model: #mtag, field: #fname_str, rule: "length",
                                                message: #msg.to_string(),
                                            });
                                        }
                                    });
                                }
                                (Some(min), Some(max), None) => {
                                    let msg =
                                        format!("length must be between {min} and {max}");
                                    checks.push(quote! {
                                        let __len = __v.chars().count();
                                        if __len < #min as usize || __len > #max as usize {
                                            return Err(ValidationError::Constraint {
                                                model: #mtag, field: #fname_str, rule: "length",
                                                message: #msg.to_string(),
                                            });
                                        }
                                    });
                                }
                                (Some(min), None, None) => {
                                    let msg = format!("length must be >= {min}");
                                    checks.push(quote! {
                                        if __v.chars().count() < #min as usize {
                                            return Err(ValidationError::Constraint {
                                                model: #mtag, field: #fname_str, rule: "length",
                                                message: #msg.to_string(),
                                            });
                                        }
                                    });
                                }
                                (None, Some(max), None) => {
                                    let msg = format!("length must be <= {max}");
                                    checks.push(quote! {
                                        if __v.chars().count() > #max as usize {
                                            return Err(ValidationError::Constraint {
                                                model: #mtag, field: #fname_str, rule: "length",
                                                message: #msg.to_string(),
                                            });
                                        }
                                    });
                                }
                                (None, None, None) => {}
                            }
                        }
                        "email" if is_string => {
                            checks.push(quote! {
                                let __parts: Vec<&str> = __v.split('@').collect();
                                let __ok = __parts.len() == 2
                                    && !__parts[0].is_empty()
                                    && __parts[1].contains('.')
                                    && !__parts[1].starts_with('.')
                                    && !__parts[1].ends_with('.')
                                    && !__v.chars().any(|__c| __c.is_whitespace());
                                if !__ok {
                                    return Err(ValidationError::Constraint {
                                        model: #mtag, field: #fname_str, rule: "email",
                                        message: "must be a valid email address".to_string(),
                                    });
                                }
                            });
                        }
                        "url" if is_string => {
                            checks.push(quote! {
                                if !(__v.starts_with("http://") || __v.starts_with("https://")) {
                                    return Err(ValidationError::Constraint {
                                        model: #mtag, field: #fname_str, rule: "url",
                                        message: "must be an http(s) URL".to_string(),
                                    });
                                }
                            });
                        }
                        "pattern" | "regex" if is_string => {
                            if let Some(pat) = Self::constraint_first_string(c) {
                                let rname = format!(
                                    "__PAT_{}_{}",
                                    Self::to_snake_case(&model.name).to_uppercase(),
                                    field.name.to_uppercase()
                                );
                                let rident = format_ident!("{}", rname);
                                let msg = format!("must match pattern {pat}");
                                pattern_statics.borrow_mut().push(quote! {
                                    static #rident: std::sync::LazyLock<regex::Regex> =
                                        std::sync::LazyLock::new(|| {
                                            regex::Regex::new(#pat)
                                                .expect("schema-declared @pattern/@regex is a valid regex")
                                        });
                                });
                                checks.push(quote! {
                                    if !#rident.is_match(__v.as_str()) {
                                        return Err(ValidationError::Constraint {
                                            model: #mtag, field: #fname_str, rule: "pattern",
                                            message: #msg.to_string(),
                                        });
                                    }
                                });
                            }
                        }
                        _ => {}
                    }
                }

                if checks.is_empty() {
                    return None;
                }
                let fident = format_ident!("{}", field.name);
                if field.is_nullable() {
                    Some(quote! {
                        if let Some(__v) = &record.#fident {
                            #(#checks)*
                        }
                    })
                } else {
                    Some(quote! {
                        {
                            let __v = &record.#fident;
                            #(#checks)*
                        }
                    })
                }
            })
            .collect();

        let pattern_statics = pattern_statics.into_inner();

        quote! {
            fn #fn_name(record: &#model_name) -> Result<(), ValidationError> {
                #(#pattern_statics)*
                #(#field_blocks)*
                Ok(())
            }
        }
    }

    fn generate_unique_checks(schema: &Schema, model: &forgedb_parser::Model, exclude_self: bool) -> Vec<TokenStream> {
        let mtag = model.name.as_str();
        Self::indexed_fields(model)
            .iter()
            .filter(|f| f.unique)
            .map(|f| {
                let ident = Self::index_field_ident(f);
                let fident = format_ident!("{}", f.name);
                let fname = f.name.as_str();
                let key = Self::index_key_expr(schema, &f.field_type, Self::index_value_expr(&f.field_type,
                    quote! { record.#fident },
                ));
                if exclude_self {
                    quote! {
                        {
                            let __uk: String = { #key };
                            if let Some(__ids) = self.#ident.get(&__uk) {
                                if __ids.iter().any(|__i| *__i != id) {
                                    return Err(ValidationError::Unique { model: #mtag, field: #fname });
                                }
                            }
                        }
                    }
                } else {
                    quote! {
                        {
                            let __uk: String = { #key };
                            if self.#ident.get(&__uk).is_some_and(|__ids| !__ids.is_empty()) {
                                return Err(ValidationError::Unique { model: #mtag, field: #fname });
                            }
                        }
                    }
                }
            })
            .collect()
    }

    fn index_field_ident(field: &forgedb_parser::Field) -> proc_macro2::Ident {
        format_ident!("{}_index", field.name)
    }

    fn index_param_type(schema: &Schema, field: &forgedb_parser::Field) -> TokenStream {
        use forgedb_parser::FieldType;
        let resolved = Self::resolved_type(schema, &field.field_type);
        match &resolved {
            FieldType::Nullable(inner) if Self::is_string_semantic(inner) => quote! { Option<&str> },
            FieldType::Nullable(inner) => {
                let ty = Self::map_field_type_ident(schema, inner);
                quote! { Option<#ty> }
            }
            _ if Self::is_string_semantic(&resolved) => quote! { &str },
            _ => {
                let ty = Self::map_field_type_ident(schema, &resolved);
                quote! { #ty }
            }
        }
    }

    fn fk_probe_arg(schema: &Schema, parent_model: &str, required: bool) -> TokenStream {
        let borrows = schema
            .find_model(parent_model)
            .and_then(|m| Self::identity_type(schema, m))
            .is_some_and(|t| Self::is_string_semantic(&t));
        let inner = if borrows { quote! { &id } } else { quote! { id } };
        if required {
            inner
        } else {
            quote! { Some(#inner) }
        }
    }

    fn timestamp_floor_quantum(field_type: &forgedb_parser::FieldType) -> Option<i64> {
        let precision = match field_type {
            forgedb_parser::FieldType::Timestamp(p) => p,
            forgedb_parser::FieldType::Nullable(inner) => match &**inner {
                forgedb_parser::FieldType::Timestamp(p) => p,
                _ => return None,
            },
            _ => return None,
        };
        let quantum = precision.quantum_micros();
        (quantum > 1).then_some(quantum)
    }

    fn timestamp_floored(
        field_type: &forgedb_parser::FieldType,
        value_expr: TokenStream,
    ) -> Option<TokenStream> {
        let quantum = Self::timestamp_floor_quantum(field_type)?;
        Some(
            if matches!(field_type, forgedb_parser::FieldType::Nullable(_)) {
                quote! { (#value_expr).map(|__ts| __ts.floor_to_micros(#quantum)) }
            } else {
                quote! { (#value_expr).floor_to_micros(#quantum) }
            },
        )
    }

    fn index_value_expr(field_type: &forgedb_parser::FieldType, value_expr: TokenStream) -> TokenStream {
        match field_type {
            forgedb_parser::FieldType::Decimal => quote! { (#value_expr).normalize() },
            forgedb_parser::FieldType::Nullable(inner) if Self::is_decimal_type(inner) => {
                quote! { (#value_expr).map(|__d| __d.normalize()) }
            }
            _ => Self::timestamp_floored(field_type, value_expr.clone()).unwrap_or(value_expr),
        }
    }

    fn index_key_expr(
        schema: &Schema,
        field_type: &forgedb_parser::FieldType,
        value_expr: TokenStream,
    ) -> TokenStream {
        use forgedb_parser::FieldType;
        let field_type = &Self::resolved_type(schema, field_type);

        let optional_inner: Option<FieldType> = match field_type {
            FieldType::Nullable(inner) => Some((**inner).clone()),
            _ => None,
        };

        if let Some(inner) = optional_inner {
            let some_arm = Self::index_key_body(schema, &inner);
            return quote! {
                match &(#value_expr) {
                    Some(__v) => { #some_arm }
                    None => String::from('\u{0}'),
                }
            };
        }

        let body = Self::index_key_body(schema, field_type);
        quote! {
            {
                let __v = &(#value_expr);
                #body
            }
        }
    }

    fn index_key_body(schema: &Schema, field_type: &forgedb_parser::FieldType) -> TokenStream {
        use forgedb_parser::FieldType;
        let field_type = &Self::resolved_type(schema, field_type);

        match field_type {
            FieldType::String | FieldType::StringN { .. } => quote! {
                let mut __k = String::with_capacity(1 + __v.len());
                __k.push('\u{1}');
                __k.push_str(__v);
                __k
            },
            FieldType::Uuid => quote! {
                let mut __buf = [0u8; 36];
                let __s: &str = __v.hyphenated().encode_lower(&mut __buf);
                let mut __k = String::with_capacity(1 + __s.len());
                __k.push('\u{1}');
                __k.push_str(__s);
                __k
            },
            FieldType::Decimal => quote! {
                use std::fmt::Write as _;
                let mut __k = String::from('\u{1}');
                let _ = write!(__k, "{}", __v);
                __k
            },
            FieldType::Enum(_) => quote! {
                let __s = __v.__as_str();
                let mut __k = String::with_capacity(1 + __s.len());
                __k.push('\u{1}');
                __k.push_str(__s);
                __k
            },

            FieldType::U32 | FieldType::U64 | FieldType::I32 | FieldType::I64 => quote! {
                use std::fmt::Write as _;
                let mut __k = String::from('\u{2}');
                let _ = write!(__k, "{}", __v);
                __k
            },
            FieldType::Timestamp(_) => quote! {
                use std::fmt::Write as _;
                let mut __k = String::from('\u{2}');
                let _ = write!(__k, "{}", __v.as_micros());
                __k
            },
            FieldType::Bool => quote! {
                let mut __k = String::with_capacity(6);
                __k.push('\u{2}');
                __k.push_str(if *__v { "true" } else { "false" });
                __k
            },
            FieldType::F64 => quote! {
                use std::fmt::Write as _;
                let mut __k = String::from('\u{2}');
                let _ = write!(__k, "{}", __forgedb_f64_key(*__v));
                __k
            },
            FieldType::Bytes(_) => quote! {
                use std::fmt::Write as _;
                let mut __k = String::from('\u{2}');
                __k.push('[');
                for (__i, __b) in __v.iter().enumerate() {
                    if __i > 0 {
                        __k.push(',');
                    }
                    let _ = write!(__k, "{}", __b);
                }
                __k.push(']');
                __k
            },

            _ => quote! {
                match serde_json::to_value(__v) {
                    Ok(serde_json::Value::Null) => String::from('\u{0}'),
                    Ok(serde_json::Value::String(__s)) => {
                        let mut __k = String::from('\u{1}');
                        __k.push_str(&__s);
                        __k
                    }
                    Ok(__other) => {
                        let mut __k = String::from('\u{2}');
                        __k.push_str(&__other.to_string());
                        __k
                    }
                    Err(_) => String::from('\u{3}'),
                }
            },
        }
    }

    fn field_key_token(
        schema: &Schema,
        field: &forgedb_parser::Field,
        record_expr: &TokenStream,
        hoisted: &std::collections::HashMap<String, proc_macro2::Ident>,
    ) -> TokenStream {
        if let Some(ident) = hoisted.get(&field.name) {
            quote! { #ident.clone() }
        } else {
            let fname = format_ident!("{}", field.name);
            let val = Self::index_value_expr(&field.field_type, quote! { #record_expr.#fname });
            Self::index_key_expr(schema, &field.field_type, val)
        }
    }

    fn shared_index_key_fields(model: &forgedb_parser::Model) -> Vec<&forgedb_parser::Field> {
        let singles: std::collections::HashSet<&str> =
            Self::indexed_fields(model).iter().map(|f| f.name.as_str()).collect();
        model
            .fields
            .iter()
            .filter(|f| {
                let mut count = if singles.contains(f.name.as_str()) { 1 } else { 0 };
                for (_ident, comps) in Self::composite_indexes(model) {
                    if comps.iter().any(|c| c.name == f.name) {
                        count += 1;
                    }
                }
                count >= 2
            })
            .collect()
    }

    fn hoist_index_keys(
        schema: &Schema,
        model: &forgedb_parser::Model,
        record_expr: &TokenStream,
        prefix: &str,
    ) -> (Vec<TokenStream>, std::collections::HashMap<String, proc_macro2::Ident>) {
        let mut binds = Vec::new();
        let mut map = std::collections::HashMap::new();
        for f in Self::shared_index_key_fields(model) {
            let ident = format_ident!("__ik_{}_{}", prefix, f.name);
            let fname = format_ident!("{}", f.name);
            let val = Self::index_value_expr(&f.field_type, quote! { #record_expr.#fname });
            let key = Self::index_key_expr(schema, &f.field_type, val);
            binds.push(quote! { let #ident: String = { #key }; });
            map.insert(f.name.clone(), ident);
        }
        (binds, map)
    }

    fn index_add_block(
        receiver: &TokenStream,
        index_ident: &proc_macro2::Ident,
        key_token: TokenStream,
        id_expr: &TokenStream,
    ) -> TokenStream {
        quote! {
            {
                let __k: String = { #key_token };
                std::sync::Arc::make_mut(&mut #receiver.#index_ident).entry(__k).or_default().insert(#id_expr);
            }
        }
    }

    fn index_remove_block(
        receiver: &TokenStream,
        index_ident: &proc_macro2::Ident,
        key_token: TokenStream,
        id_expr: &TokenStream,
    ) -> TokenStream {
        quote! {
            {
                let __k: String = { #key_token };
                let mut __empty = false;
                let __map = std::sync::Arc::make_mut(&mut #receiver.#index_ident);
                if let Some(__set) = __map.get_mut(&__k) {
                    __set.remove(&(#id_expr));
                    __empty = __set.is_empty();
                }
                if __empty {
                    __map.remove(&__k);
                }
            }
        }
    }

    fn ordered_key_type(field: &forgedb_parser::Field) -> Option<TokenStream> {
        use forgedb_parser::FieldType;
        if field.is_nullable() {
            return None;
        }
        match &field.field_type {
            FieldType::U32 => Some(quote! { u32 }),
            FieldType::U64 => Some(quote! { u64 }),
            FieldType::I32 => Some(quote! { i32 }),
            FieldType::I64 => Some(quote! { i64 }),
            FieldType::Timestamp(_) => Some(quote! { Timestamp }),
            FieldType::Decimal => Some(quote! { rust_decimal::Decimal }),
            FieldType::F64 => Some(quote! { u64 }),
            _ => None,
        }
    }

    fn ordered_param_type(field: &forgedb_parser::Field) -> Option<TokenStream> {
        match &field.field_type {
            forgedb_parser::FieldType::F64 if !field.is_nullable() => Some(quote! { f64 }),
            _ => Self::ordered_key_type(field),
        }
    }

    fn ordered_index_fields(model: &forgedb_parser::Model) -> Vec<&forgedb_parser::Field> {
        Self::indexed_fields(model)
            .into_iter()
            .filter(|f| Self::ordered_key_type(f).is_some())
            .collect()
    }

    fn ordered_index_ident(field: &forgedb_parser::Field) -> proc_macro2::Ident {
        format_ident!("{}_ordered", field.name)
    }

    fn ordered_key_expr(field_type: &forgedb_parser::FieldType, value_expr: TokenStream) -> TokenStream {
        match field_type {
            forgedb_parser::FieldType::Decimal => quote! { (#value_expr).normalize() },
            forgedb_parser::FieldType::F64 => quote! { __forgedb_f64_key(#value_expr) },
            _ => Self::timestamp_floored(field_type, value_expr.clone()).unwrap_or(value_expr),
        }
    }

    fn ordered_index_add_block(
        receiver: &TokenStream,
        index_ident: &proc_macro2::Ident,
        key_expr: TokenStream,
        id_expr: &TokenStream,
    ) -> TokenStream {
        quote! {
            {
                std::sync::Arc::make_mut(&mut #receiver.#index_ident)
                    .entry(#key_expr)
                    .or_default()
                    .insert(#id_expr);
            }
        }
    }

    fn ordered_index_remove_block(
        receiver: &TokenStream,
        index_ident: &proc_macro2::Ident,
        key_expr: TokenStream,
        id_expr: &TokenStream,
    ) -> TokenStream {
        quote! {
            {
                let __ok = { #key_expr };
                let mut __empty = false;
                let __map = std::sync::Arc::make_mut(&mut #receiver.#index_ident);
                if let Some(__set) = __map.get_mut(&__ok) {
                    __set.remove(&(#id_expr));
                    __empty = __set.is_empty();
                }
                if __empty {
                    __map.remove(&__ok);
                }
            }
        }
    }

    fn ordered_index_add_maint(
        model: &forgedb_parser::Model,
        recv: &TokenStream,
        record_expr: &TokenStream,
        id_tok: &TokenStream,
    ) -> Vec<TokenStream> {
        Self::ordered_index_fields(model)
            .iter()
            .map(|f| {
                let ident = Self::ordered_index_ident(f);
                let fname = format_ident!("{}", f.name);
                let key = Self::ordered_key_expr(&f.field_type, quote! { #record_expr.#fname });
                Self::ordered_index_add_block(recv, &ident, key, id_tok)
            })
            .collect()
    }

    fn ordered_index_remove_maint(
        model: &forgedb_parser::Model,
        recv: &TokenStream,
        record_expr: &TokenStream,
        id_tok: &TokenStream,
    ) -> Vec<TokenStream> {
        Self::ordered_index_fields(model)
            .iter()
            .map(|f| {
                let ident = Self::ordered_index_ident(f);
                let fname = format_ident!("{}", f.name);
                let key = Self::ordered_key_expr(&f.field_type, quote! { #record_expr.#fname });
                Self::ordered_index_remove_block(recv, &ident, key, id_tok)
            })
            .collect()
    }

    fn composite_indexes(
        model: &forgedb_parser::Model,
    ) -> Vec<(proc_macro2::Ident, Vec<&forgedb_parser::Field>)> {
        if !model.has_identity() {
            return Vec::new();
        }
        model
            .composite_indexes
            .iter()
            .filter_map(|ci| {
                if ci.fields.len() < 2 {
                    return None;
                }
                let comps: Option<Vec<&forgedb_parser::Field>> = ci
                    .fields
                    .iter()
                    .map(|name| {
                        model
                            .fields
                            .iter()
                            .find(|f| &f.name == name)
                            .filter(|f| Self::is_composite_component(&f.field_type))
                    })
                    .collect();
                let comps = comps?;
                let ident = format_ident!("{}_index", ci.fields.join("_"));
                Some((ident, comps))
            })
            .collect()
    }

    fn is_composite_component(field_type: &forgedb_parser::FieldType) -> bool {
        Self::is_filterable_scalar(field_type)
            || matches!(
                field_type,
                forgedb_parser::FieldType::Relation(
                    forgedb_parser::RelationType::RequiredReference(_)
                        | forgedb_parser::RelationType::OptionalReference(_),
                )
            )
    }

    fn composite_probe_ident(components: &[&forgedb_parser::Field]) -> proc_macro2::Ident {
        let joined = components
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>()
            .join("_and_");
        format_ident!("find_by_{}", joined)
    }

    fn composite_key_build(part_key_exprs: &[TokenStream]) -> TokenStream {
        let pushes = part_key_exprs.iter().map(|k| {
            quote! {
                {
                    let __p: String = { #k };
                    __ck.push_str(&__p.len().to_string());
                    __ck.push(':');
                    __ck.push_str(&__p);
                }
            }
        });
        quote! {
            {
                let mut __ck = String::new();
                #(#pushes)*
                __ck
            }
        }
    }

    fn composite_add_block(
        receiver: &TokenStream,
        index_ident: &proc_macro2::Ident,
        part_key_tokens: &[TokenStream],
        id_expr: &TokenStream,
    ) -> TokenStream {
        let key = Self::composite_key_build(part_key_tokens);
        quote! {
            {
                let __k: String = #key;
                std::sync::Arc::make_mut(&mut #receiver.#index_ident).entry(__k).or_default().insert(#id_expr);
            }
        }
    }

    fn composite_remove_block(
        receiver: &TokenStream,
        index_ident: &proc_macro2::Ident,
        part_key_tokens: &[TokenStream],
        id_expr: &TokenStream,
    ) -> TokenStream {
        let key = Self::composite_key_build(part_key_tokens);
        quote! {
            {
                let __k: String = #key;
                let mut __empty = false;
                let __map = std::sync::Arc::make_mut(&mut #receiver.#index_ident);
                if let Some(__set) = __map.get_mut(&__k) {
                    __set.remove(&(#id_expr));
                    __empty = __set.is_empty();
                }
                if __empty {
                    __map.remove(&__k);
                }
            }
        }
    }

    fn generate_storage_fields(schema: &Schema, model: &forgedb_parser::Model) -> TokenStream {
        let mut column_fields = Vec::new();

        for field in &model.fields {
            let field_col_name = format_ident!("{}_col", field.name);

            if Self::is_fixed_size_type(schema, &field.field_type) {
                column_fields.push(quote! {
                    #field_col_name: FixedColumn
                });
            } else if Self::is_variable_column_type(&field.field_type) {
                column_fields.push(quote! {
                    #field_col_name: VariableColumn
                });
            }
        }

        let id_type = Self::id_type_tokens(schema, model);

        let index_fields: Vec<_> = Self::indexed_fields(model)
            .iter()
            .map(|f| {
                let ident = Self::index_field_ident(f);
                quote! {
                    #ident: std::sync::Arc<std::collections::HashMap<String, std::collections::HashSet<#id_type>>>
                }
            })
            .collect();

        let composite_fields: Vec<_> = Self::composite_indexes(model)
            .iter()
            .map(|(ident, _comps)| {
                quote! {
                    #ident: std::sync::Arc<std::collections::HashMap<String, std::collections::HashSet<#id_type>>>
                }
            })
            .collect();

        let ordered_fields: Vec<_> = Self::ordered_index_fields(model)
            .iter()
            .map(|f| {
                let ident = Self::ordered_index_ident(f);
                let kty = Self::ordered_key_type(f).expect("ordered_index_fields filtered");
                quote! {
                    #ident: std::sync::Arc<std::collections::BTreeMap<#kty, std::collections::BTreeSet<#id_type>>>
                }
            })
            .collect();

        let id_versions_field = if model.identity_field().is_some() {
            quote! { id_versions: std::sync::Arc<HashMap<#id_type, Vec<usize>>>, }
        } else {
            quote! {}
        };

        let autoseq_fields: Vec<_> = Self::sequence_auto_fields(model)
            .iter()
            .map(|f| {
                let ident = Self::autoseq_field_ident(f);
                quote! { #ident: std::sync::Arc<std::sync::atomic::AtomicU64> }
            })
            .collect();

        quote! {
            id_to_row: std::sync::Arc<HashMap<#id_type, usize>>,
            #id_versions_field
            row_count: usize,
            #(#column_fields,)*
            #(#index_fields,)*
            #(#composite_fields,)*
            #(#ordered_fields,)*
            #(#autoseq_fields,)*
            tombstones: forgedb_storage::Tombstones,
            wal: forgedb_wal::WalManager,
            writes_since_checkpoint: u64,
            root: std::path::PathBuf,
            dead_since_compaction: u64,
            in_transaction: bool,
            checkpoint_deferred: bool,
            compact_deferred: bool,
            compaction_due: bool,
            changefeed: Option<forgedb_changefeed::ChangeFeed>,
            broker: Option<std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>>,
        }
    }

    fn generate_storage_inits(model: &forgedb_parser::Model, schema: &Schema) -> TokenStream {
        let mut inits = Vec::new();
        let mut column_index = 0usize;
        let model_snake = Self::to_snake_case(&model.name);

        for field in &model.fields {
            let field_col_name = format_ident!("{}_col", field.name);

            if Self::is_fixed_size_type(schema, &field.field_type) {
                let col_path = format!(
                    "{}/fixed/{}_{}.bin",
                    model_snake,
                    Self::type_name(schema, &field.field_type),
                    column_index
                );
                let value_size_expr =
                    Self::column_value_size_expr(schema, &field.field_type, Self::is_utf8_field(field));

                inits.push(quote! {
                    #field_col_name: FixedColumn::new(
                        root.join(#col_path),
                        #value_size_expr
                    ).expect("Failed to create fixed column")
                });
                column_index += 1;
            } else if Self::is_variable_column_type(&field.field_type) {
                let data_path = format!("{}/variable/string_data_{}.bin", model_snake, column_index);
                let offsets_path = format!("{}/variable/string_offsets_{}.bin", model_snake, column_index);

                inits.push(quote! {
                    #field_col_name: VariableColumn::new(
                        root.join(#data_path),
                        root.join(#offsets_path)
                    ).expect("Failed to create variable column")
                });
                column_index += 1;
            }
        }

        let tombstones_path = format!("{}/tombstones.bin", model_snake);
        let wal_path = format!("{}/wal.log", model_snake);

        let index_inits: Vec<_> = Self::indexed_fields(model)
            .iter()
            .map(|f| {
                let ident = Self::index_field_ident(f);
                quote! { #ident: std::sync::Arc::new(std::collections::HashMap::new()) }
            })
            .collect();

        let composite_inits: Vec<_> = Self::composite_indexes(model)
            .iter()
            .map(|(ident, _comps)| {
                quote! { #ident: std::sync::Arc::new(std::collections::HashMap::new()) }
            })
            .collect();

        let ordered_inits: Vec<_> = Self::ordered_index_fields(model)
            .iter()
            .map(|f| {
                let ident = Self::ordered_index_ident(f);
                quote! { #ident: std::sync::Arc::new(std::collections::BTreeMap::new()) }
            })
            .collect();

        let wal_fsync = Self::wal_fsync_policy_tokens();

        let id_versions_init = if model.identity_field().is_some() {
            quote! { id_versions: std::sync::Arc::new(HashMap::new()), }
        } else {
            quote! {}
        };

        let autoseq_inits: Vec<_> = Self::sequence_auto_fields(model)
            .iter()
            .map(|f| {
                let ident = Self::autoseq_field_ident(f);
                quote! { #ident: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)) }
            })
            .collect();

        quote! {
            id_to_row: std::sync::Arc::new(HashMap::new()),
            #id_versions_init
            row_count: 0,
            #(#inits,)*
            #(#index_inits,)*
            #(#composite_inits,)*
            #(#ordered_inits,)*
            #(#autoseq_inits,)*
            tombstones: forgedb_storage::Tombstones::new(
                root.join(#tombstones_path)
            ).expect("Failed to create tombstones"),
            wal: forgedb_wal::WalManager::open(
                root.join(#wal_path),
                #wal_fsync,
            ).expect("Failed to open WAL"),
            writes_since_checkpoint: 0,
            root: root.to_path_buf(),
            dead_since_compaction: 0,
            in_transaction: false,
            checkpoint_deferred: false,
            compact_deferred: false,
            compaction_due: false,
            changefeed: None,
            broker: None,
        }
    }

    fn generate_reader_storage_fields(schema: &Schema, model: &forgedb_parser::Model) -> TokenStream {
        let mut column_fields = Vec::new();
        for field in &model.fields {
            let field_col_name = format_ident!("{}_col", field.name);
            if Self::is_fixed_size_type(schema, &field.field_type) {
                column_fields.push(quote! {
                    #field_col_name: forgedb_storage::FixedColumnReader
                });
            } else if Self::is_variable_column_type(&field.field_type) {
                column_fields.push(quote! {
                    #field_col_name: forgedb_storage::VariableColumnReader
                });
            }
        }
        let id_type = Self::id_type_tokens(schema, model);
        let index_fields: Vec<_> = Self::indexed_fields(model)
            .iter()
            .map(|f| {
                let ident = Self::index_field_ident(f);
                quote! {
                    #ident: std::sync::Arc<std::collections::HashMap<String, std::collections::HashSet<#id_type>>>
                }
            })
            .collect();
        let composite_fields: Vec<_> = Self::composite_indexes(model)
            .iter()
            .map(|(ident, _comps)| {
                quote! {
                    #ident: std::sync::Arc<std::collections::HashMap<String, std::collections::HashSet<#id_type>>>
                }
            })
            .collect();
        let id_versions_field = if model.identity_field().is_some() {
            quote! { id_versions: std::sync::Arc<HashMap<#id_type, Vec<usize>>>, }
        } else {
            quote! {}
        };
        quote! {
            id_to_row: std::sync::Arc<HashMap<#id_type, usize>>,
            #id_versions_field
            #(#column_fields,)*
            #(#index_fields,)*
            #(#composite_fields,)*
            tombstones: forgedb_storage::TombstonesReader,
        }
    }

    fn generate_reader_inits(schema: &Schema, model: &forgedb_parser::Model) -> TokenStream {
        let mut inits = Vec::new();
        for field in &model.fields {
            let field_col_name = format_ident!("{}_col", field.name);
            if Self::is_fixed_size_type(schema, &field.field_type)
                || Self::is_variable_column_type(&field.field_type)
            {
                inits.push(quote! {
                    #field_col_name: self.#field_col_name.reader()
                        .expect("Failed to open column reader")
                });
            }
        }
        let index_clones: Vec<_> = Self::indexed_fields(model)
            .iter()
            .map(|f| {
                let ident = Self::index_field_ident(f);
                quote! { #ident: self.#ident.clone() }
            })
            .collect();
        let composite_clones: Vec<_> = Self::composite_indexes(model)
            .iter()
            .map(|(ident, _comps)| {
                quote! { #ident: self.#ident.clone() }
            })
            .collect();
        let id_versions_clone = if model.identity_field().is_some() {
            quote! { id_versions: self.id_versions.clone(), }
        } else {
            quote! {}
        };
        quote! {
            id_to_row: self.id_to_row.clone(),
            #id_versions_clone
            #(#inits,)*
            #(#index_clones,)*
            #(#composite_clones,)*
            tombstones: self.tombstones.reader()
                .expect("Failed to open tombstones reader"),
        }
    }

    fn column_value_size_expr(
        schema: &Schema,
        field_type: &forgedb_parser::FieldType,
        utf8: bool,
    ) -> TokenStream {
        let field_type = &Self::resolved_type(schema, field_type);
        match field_type {
            forgedb_parser::FieldType::StringN { chars, exact } => {
                let (slot, _, _) = Self::inline_string_layout(*chars, *exact, utf8);
                quote! { #slot }
            }
            forgedb_parser::FieldType::Nullable(inner)
                if matches!(**inner, forgedb_parser::FieldType::StringN { .. }) =>
            {
                let forgedb_parser::FieldType::StringN { chars, exact } = **inner else {
                    unreachable!("guarded by the matches! above")
                };
                let (slot, _, _) = Self::inline_string_layout(chars, exact, utf8);
                let slot = slot + 1;
                quote! { #slot }
            }
            _ if Self::is_enum_type(field_type) => {
                if matches!(field_type, forgedb_parser::FieldType::Nullable(_)) {
                    quote! { 2usize }
                } else {
                    quote! { 1usize }
                }
            }
            forgedb_parser::FieldType::StructType(name) => {
                let ident = format_ident!("{}", name);
                quote! { std::mem::size_of::<#ident>() }
            }
            forgedb_parser::FieldType::OptionalStructType(name) => {
                let ident = format_ident!("{}", name);
                quote! { std::mem::size_of::<Option<#ident>>() }
            }
            forgedb_parser::FieldType::Nullable(inner) => {
                let inner_tokens = Self::map_field_type_ident(schema, inner);
                quote! { std::mem::size_of::<Option<#inner_tokens>>() }
            }
            _ => {
                let size = match field_type {
                    forgedb_parser::FieldType::U32 | forgedb_parser::FieldType::I32 => 4usize,
                    forgedb_parser::FieldType::U64 | forgedb_parser::FieldType::I64 => 8,
                    forgedb_parser::FieldType::F64 => 8,
                    forgedb_parser::FieldType::Bool => 1,
                    forgedb_parser::FieldType::Enum(_) => 1,
                    forgedb_parser::FieldType::Uuid => 16,
                    forgedb_parser::FieldType::Decimal => 16,
                    forgedb_parser::FieldType::Timestamp(_) => 8,
                    forgedb_parser::FieldType::Bytes(n) => *n,
                    forgedb_parser::FieldType::FixedArray(inner, count) => {
                        let inner_tokens = Self::map_field_type_ident(schema, inner);
                        return quote! { std::mem::size_of::<[#inner_tokens; #count]>() };
                    }
                    _ => 8,
                };
                quote! { #size }
            }
        }
    }

    fn storage_column_type_tokens(
        schema: &Schema,
        field_type: &forgedb_parser::FieldType,
        utf8: bool,
    ) -> TokenStream {
        use forgedb_parser::FieldType;
        let field_type = &Self::resolved_type(schema, field_type);
        match field_type {
            FieldType::U32 => quote! { forgedb_storage::ColumnType::U32 },
            FieldType::I32 => quote! { forgedb_storage::ColumnType::I32 },
            FieldType::U64 => quote! { forgedb_storage::ColumnType::U64 },
            FieldType::I64 => quote! { forgedb_storage::ColumnType::I64 },
            FieldType::F64 => quote! { forgedb_storage::ColumnType::F64 },
            FieldType::Bool => quote! { forgedb_storage::ColumnType::Bool },
            FieldType::Uuid => quote! { forgedb_storage::ColumnType::Uuid },
            FieldType::Timestamp(_) => quote! { forgedb_storage::ColumnType::Timestamp },
            _ => {
                let size = Self::column_value_size_expr(schema, field_type, utf8);
                quote! { forgedb_storage::ColumnType::FixedBytes(#size) }
            }
        }
    }

    fn generate_autoseq_methods(schema: &Schema, model: &forgedb_parser::Model) -> TokenStream {
        let model_name = &model.name;
        let timestamp_methods: Vec<TokenStream> = Self::timestamp_key_field(model)
            .into_iter()
            .map(|f| {
                let alloc = Self::autoseq_alloc_ident(f);
                let seq = Self::autoseq_field_ident(f);
                quote! {
                    fn #alloc(&self) -> Result<Timestamp, ValidationError> {
                        use std::sync::atomic::Ordering;
                        let mut __cur = self.#seq.load(Ordering::SeqCst);
                        loop {
                            let __now = Timestamp::now().as_micros().max(0) as u64;
                            let __next = __now.max(__cur.saturating_add(1));
                            match self.#seq.compare_exchange_weak(
                                __cur,
                                __next,
                                Ordering::SeqCst,
                                Ordering::SeqCst,
                            ) {
                                Ok(_) => return Ok(Timestamp::from_micros(__next as i64)),
                                Err(__actual) => __cur = __actual,
                            }
                        }
                    }
                }
            })
            .collect();
        let methods: Vec<TokenStream> = Self::integer_auto_fields(model)
            .iter()
            .map(|f| {
                let alloc = Self::autoseq_alloc_ident(f);
                let seq = Self::autoseq_field_ident(f);
                let field_name = &f.name;
                let ty = Self::map_field_type_ident(schema, &f.field_type);
                quote! {
                    fn #alloc(&self) -> Result<#ty, ValidationError> {
                        use std::sync::atomic::Ordering;
                        const __LIMIT: u64 = #ty::MAX as u64;
                        let mut __cur = self.#seq.load(Ordering::SeqCst);
                        loop {
                            if __cur >= __LIMIT {
                                return Err(ValidationError::SequenceExhausted {
                                    model: #model_name,
                                    field: #field_name,
                                });
                            }
                            match self.#seq.compare_exchange_weak(
                                __cur,
                                __cur + 1,
                                Ordering::SeqCst,
                                Ordering::SeqCst,
                            ) {
                                Ok(_) => return Ok((__cur + 1) as #ty),
                                Err(__actual) => __cur = __actual,
                            }
                        }
                    }
                }
            })
            .collect();
        quote! { #(#methods)* #(#timestamp_methods)* }
    }

    fn generate_write_manifest(schema: &Schema, model: &forgedb_parser::Model) -> TokenStream {
        let model_snake = Self::to_snake_case(&model.name);
        let manifest_path = format!("{}/manifest.json", model_snake);

        let mut col_entries = Vec::new();
        let mut column_index = 0usize;
        for field in &model.fields {
            let name = &field.name;
            if Self::is_fixed_size_type(schema, &field.field_type) {
                let rel_path = format!(
                    "fixed/{}_{}.bin",
                    Self::type_name(schema, &field.field_type),
                    column_index
                );
                let __utf8 = Self::is_utf8_field(field);
                let col_type = Self::storage_column_type_tokens(schema, &field.field_type, __utf8);
                let value_size = Self::column_value_size_expr(schema, &field.field_type, __utf8);
                col_entries.push(quote! {
                    forgedb_storage::ColumnMetadata {
                        name: #name.to_string(),
                        column_type: #col_type,
                        column_index: #column_index,
                        value_size: #value_size,
                        kind: forgedb_storage::ColumnKind::Fixed,
                        relative_path: #rel_path.to_string(),
                    }
                });
                column_index += 1;
            } else if Self::is_variable_column_type(&field.field_type) {
                let rel_path = format!("variable/string_data_{}.bin", column_index);
                col_entries.push(quote! {
                    forgedb_storage::ColumnMetadata {
                        name: #name.to_string(),
                        column_type: forgedb_storage::ColumnType::String,
                        column_index: #column_index,
                        value_size: 0usize,
                        kind: forgedb_storage::ColumnKind::Variable,
                        relative_path: #rel_path.to_string(),
                    }
                });
                column_index += 1;
            }
        }

        let autoseq_merges: Vec<TokenStream> = Self::sequence_auto_fields(model)
            .iter()
            .map(|f| {
                let seq = Self::autoseq_field_ident(f);
                let key = &f.name;
                quote! {
                    {
                        let __live = self.#seq.load(std::sync::atomic::Ordering::SeqCst);
                        let __slot = __auto_sequences.entry(#key.to_string()).or_insert(0);
                        if __live > *__slot {
                            *__slot = __live;
                        }
                    }
                }
            })
            .collect();

        quote! {
            fn write_manifest(&self, root: &std::path::Path) -> std::io::Result<()> {
                let columns = vec![ #(#col_entries),* ];
                let __manifest_abs = root.join(#manifest_path);
                let __compaction_epoch = forgedb_storage::Manifest::load_from(&__manifest_abs)
                    .map(|m| m.compaction_epoch)
                    .unwrap_or(0);
                let __schema_version = forgedb_storage::Manifest::load_from(&__manifest_abs)
                    .map(|m| m.schema_version)
                    .unwrap_or(EXPECTED_SCHEMA_VERSION);
                let __persisted_seqs = forgedb_storage::Manifest::load_from(&__manifest_abs)
                    .map(|m| m.auto_sequences)
                    .unwrap_or_default();
                #[allow(unused_mut)]
                let mut __auto_sequences = __persisted_seqs;
                #(#autoseq_merges)*
                let manifest = forgedb_storage::Manifest {
                    row_count: self.row_count,
                    columns,
                    wal_enabled: false,
                    last_checkpoint: self.row_count as u64,
                    compaction_epoch: __compaction_epoch,
                    schema_version: __schema_version,
                    engine_version: EXPECTED_ENGINE_VERSION,
                    row_anchor: Some(forgedb_storage::RowAnchor {
                        relative_path: "tombstones.bin".to_string(),
                        bytes_per_row: 1usize,
                    }),
                    auto_sequences: __auto_sequences,
                };
                manifest.save_to(&__manifest_abs)
            }

            fn bump_compaction_epoch(&self, root: &std::path::Path) {
                let __manifest_abs = root.join(#manifest_path);
                if let Ok(mut __m) = forgedb_storage::Manifest::load_from(&__manifest_abs) {
                    __m.compaction_epoch = __m.compaction_epoch.saturating_add(1);
                    let _ = __m.save_to(&__manifest_abs);
                }
            }
        }
    }

    fn generate_append_statements(schema: &Schema, model: &forgedb_parser::Model) -> Vec<TokenStream> {
        let mut append_statements = Vec::new();

        for field in &model.fields {
            let field_name = format_ident!("{}", field.name);
            let field_col_name = format_ident!("{}_col", field.name);

            if Self::is_json_type(&field.field_type) {
                if field.is_nullable() {
                    append_statements.push(quote! {
                        {
                            match &record.#field_name {
                                Some(v) => {
                                    let s = serde_json::to_string(v)
                                        .expect("Failed to serialize json");
                                    self.#field_col_name.append_tagged(1u8, &s)
                                }
                                None => self.#field_col_name.append_tagged(0u8, ""),
                            }
                            .expect("Failed to append string");
                        }
                    });
                } else {
                    append_statements.push(quote! {
                        {
                            let s = serde_json::to_string(&record.#field_name)
                                .expect("Failed to serialize json");
                            self.#field_col_name.append_string(&s)
                                .expect("Failed to append string");
                        }
                    });
                }
            } else if let Some((chars, exact)) =
                Self::inline_string_column_params(schema, &field.field_type)
            {
                let utf8 = Self::is_utf8_field(field);
                let (slot, _, _) = Self::inline_string_layout(chars, exact, utf8);
                if field.is_nullable() {
                    let slot = slot + 1;
                    let pack = Self::inline_string_pack_body(
                        chars,
                        exact,
                        utf8,
                        1,
                        quote! { __v.as_str() },
                    );
                    append_statements.push(quote! {
                        {
                            let mut __buf = [0u8; #slot];
                            if let Some(__v) = &record.#field_name {
                                __buf[0] = 1u8;
                                #pack
                            }
                            self.#field_col_name.append_bytes(&__buf)
                                .expect("Failed to append to column");
                        }
                    });
                } else {
                    let pack = Self::inline_string_pack_body(
                        chars,
                        exact,
                        utf8,
                        0,
                        quote! { record.#field_name.as_str() },
                    );
                    append_statements.push(quote! {
                        {
                            let mut __buf = [0u8; #slot];
                            #pack
                            self.#field_col_name.append_bytes(&__buf)
                                .expect("Failed to append to column");
                        }
                    });
                }
            } else if Self::is_variable_string_type(&field.field_type) {
                if field.is_nullable() {
                    append_statements.push(quote! {
                        {
                            match &record.#field_name {
                                Some(s) => self.#field_col_name.append_tagged(1u8, s),
                                None => self.#field_col_name.append_tagged(0u8, ""),
                            }
                            .expect("Failed to append string");
                        }
                    });
                } else {
                    append_statements.push(quote! {
                        self.#field_col_name.append_string(&record.#field_name)
                            .expect("Failed to append string");
                    });
                }
            } else if Self::is_enum_type(&field.field_type) {
                if field.is_nullable() {
                    append_statements.push(quote! {
                        {
                            let bytes: [u8; 2] = match &record.#field_name {
                                Some(v) => [1u8, v.__to_u8()],
                                None => [0u8, 0u8],
                            };
                            self.#field_col_name.append_bytes(&bytes)
                                .expect("Failed to append to column");
                        }
                    });
                } else {
                    append_statements.push(quote! {
                        self.#field_col_name.append_bytes(&[record.#field_name.__to_u8()])
                            .expect("Failed to append to column");
                    });
                }
            } else if Self::is_fixed_size_type(schema, &field.field_type) {
                let needs_byte_conversion = matches!(
                    &Self::resolved_type(schema, &field.field_type),
                    forgedb_parser::FieldType::Bytes(_)
                        | forgedb_parser::FieldType::FixedArray(_, _)
                        | forgedb_parser::FieldType::StructType(_)
                        | forgedb_parser::FieldType::OptionalStructType(_)
                        | forgedb_parser::FieldType::Nullable(_)
                );

                if needs_byte_conversion {
                    append_statements.push(quote! {
                        {
                            let bytes = unsafe {
                                std::slice::from_raw_parts(
                                    &record.#field_name as *const _ as *const u8,
                                    std::mem::size_of_val(&record.#field_name)
                                )
                            };
                            self.#field_col_name.append_bytes(bytes)
                                .expect("Failed to append to column");
                        }
                    });
                } else {
                    let append_method = Self::get_append_method(schema, &field.field_type);

                    let is_uuid_like = matches!(
                        &Self::resolved_type(schema, &field.field_type),
                        forgedb_parser::FieldType::Uuid
                    );
                    let is_timestamp =
                        matches!(&Self::resolved_type(schema, &field.field_type), forgedb_parser::FieldType::Timestamp(_));
                    let is_decimal =
                        matches!(&Self::resolved_type(schema, &field.field_type), forgedb_parser::FieldType::Decimal);

                    if is_decimal {
                        append_statements.push(quote! {
                            self.#field_col_name.append_uuid(record.#field_name.serialize())
                                .expect("Failed to append to column");
                        });
                    } else if is_uuid_like {
                        append_statements.push(quote! {
                            self.#field_col_name.#append_method(*record.#field_name.as_bytes())
                                .expect("Failed to append to column");
                        });
                    } else if is_timestamp {
                        append_statements.push(quote! {
                            self.#field_col_name.#append_method(i64::from(record.#field_name))
                                .expect("Failed to append to column");
                        });
                    } else {
                        append_statements.push(quote! {
                            self.#field_col_name.#append_method(record.#field_name)
                                .expect("Failed to append to column");
                        });
                    }
                }
            }
        }

        append_statements
    }

    fn id_field_ident(model: &forgedb_parser::Model) -> proc_macro2::Ident {
        match model.identity_field() {
            Some(f) => format_ident!("{}", f.name),
            None => format_ident!("id"),
        }
    }

    fn generate_id_read_expr(
        schema: &Schema,
        model: &forgedb_parser::Model,
        receiver: &TokenStream,
        row_var: &proc_macro2::Ident,
    ) -> Option<TokenStream> {
        let f = model.identity_field()?;
        let col = format_ident!("{}_col", f.name);

        let is_uuid_like = matches!(
            &Self::resolved_type(schema, &f.field_type),
            forgedb_parser::FieldType::Uuid
        );
        let is_timestamp = matches!(&Self::resolved_type(schema, &f.field_type), forgedb_parser::FieldType::Timestamp(_));
        let is_string = Self::is_variable_string_type(&Self::resolved_type(schema, &f.field_type));

        let expr = if is_uuid_like {
            quote! {
                {
                    let bytes = #receiver.#col.read_uuid(#row_var).expect("Failed to read id column");
                    Uuid::from_bytes(bytes)
                }
            }
        } else if is_timestamp {
            quote! {
                Timestamp::from(
                    #receiver.#col.read_timestamp(#row_var).expect("Failed to read id column"),
                )
            }
        } else if is_string {
            quote! {
                #receiver.#col.read_string(#row_var).expect("Failed to read id column")
            }
        } else if let Some((chars, exact)) = Self::inline_string_params(&f.field_type) {
            let bytes = Self::inline_string_bytes_expr(
                chars,
                exact,
                Self::is_utf8_field(f),
                0,
                quote! { __slot_bytes },
            );
            let key_ty = Self::key_type_ident(schema, &f.field_type);
            quote! {
                {
                    let __slot_bytes = #receiver.#col.read_bytes(#row_var)
                        .expect("Failed to read id column");
                    <#key_ty>::try_from(
                        std::str::from_utf8(#bytes)
                            .expect("inline string column holds UTF-8"),
                    )
                    .unwrap_or_default()
                }
            }
        } else {
            let read_method = Self::get_read_method(schema, &f.field_type);
            quote! {
                #receiver.#col.#read_method(#row_var).expect("Failed to read id column")
            }
        };
        Some(expr)
    }

    fn id_versions_push_stmt(
        model: &forgedb_parser::Model,
        receiver: &TokenStream,
        id_expr: &TokenStream,
        row_expr: &TokenStream,
    ) -> TokenStream {
        if model.identity_field().is_some() {
            quote! {
                std::sync::Arc::make_mut(&mut #receiver.id_versions)
                    .entry(#id_expr).or_default().push(#row_expr);
            }
        } else {
            quote! {}
        }
    }

    fn generate_wal_record_write(model: &forgedb_parser::Model, deleted: bool) -> TokenStream {
        Self::generate_wal_record_write_impl(model, deleted, false)
    }

    fn generate_wal_record_write_buffered(model: &forgedb_parser::Model, deleted: bool) -> TokenStream {
        Self::generate_wal_record_write_impl(model, deleted, true)
    }

    fn generate_wal_record_write_impl(
        model: &forgedb_parser::Model,
        deleted: bool,
        buffered: bool,
    ) -> TokenStream {
        let model_name_str = model.name.clone();
        let deleted_tok = if deleted {
            quote! { 1u8 }
        } else {
            quote! { 0u8 }
        };
        let write_call = if buffered {
            quote! { write_buffered }
        } else {
            quote! { write }
        };
        quote! {
            {
                let mut __wal_payload = Vec::new();
                __wal_payload.extend_from_slice(&(self.row_count as u64).to_le_bytes());
                __wal_payload.push(#deleted_tok);
                __wal_payload.extend_from_slice(&__record_json);
                self.wal
                    .#write_call(&forgedb_wal::WalEntry::raw(#model_name_str, __wal_payload))
                    .expect("Failed to write WAL record");
            }
        }
    }

    fn generate_shared_record_json() -> TokenStream {
        quote! {
            let __record_json = serde_json::to_vec(&record)
                .expect("Failed to serialize record");
        }
    }

    fn generate_broker_record(model: &forgedb_parser::Model, kind: TokenStream) -> TokenStream {
        let model_name_str = model.name.clone();
        quote! {
            if let Some(__broker) = &self.broker {
                if let Ok(mut __b) = __broker.lock() {
                    let _ = __b.record(
                        #model_name_str,
                        row_index as u64,
                        #kind,
                        __record_json,
                    );
                }
            }
        }
    }

    fn backfill_append_of(
        schema: &Schema,
        field: &forgedb_parser::Field,
        fill: &crate::default_fill::FillValue,
    ) -> TokenStream {
        use crate::default_fill::FillValue;
        let col = format_ident!("{}_col", field.name);
        let append_method = Self::get_append_method(schema, &field.field_type);
        match fill {
            FillValue::Bool(b) => quote! {
                self.#col.#append_method(#b).expect("Failed to backfill column default");
            },
            FillValue::Int(n) => {
                let lit = proc_macro2::Literal::i64_unsuffixed(*n);
                quote! {
                    self.#col.#append_method(#lit).expect("Failed to backfill column default");
                }
            }
            FillValue::Float(f) => quote! {
                self.#col.#append_method(#f).expect("Failed to backfill column default");
            },
            FillValue::Str(s) => quote! {
                self.#col.append_string(#s).expect("Failed to backfill column default");
            },
            FillValue::Json(raw) => quote! {
                self.#col.append_string(#raw).expect("Failed to backfill column default");
            },
            FillValue::Enum { discriminant, .. } => quote! {
                self.#col.append_bytes(&[#discriminant]).expect("Failed to backfill column default");
            },
            FillValue::Decimal(lexeme) => quote! {
                self.#col
                    .append_uuid(
                        <rust_decimal::Decimal as std::str::FromStr>::from_str(#lexeme)
                            .expect("schema @default is not a decimal")
                            .serialize(),
                    )
                    .expect("Failed to backfill column default");
            },
        }
    }

    fn generate_backfill_appends(schema: &Schema, model: &forgedb_parser::Model) -> Vec<(proc_macro2::Ident, TokenStream)> {
        let mut out = Vec::new();
        for field in &model.fields {
            let col = format_ident!("{}_col", field.name);
            if let Some(fill) = crate::default_fill::default_fill(schema, field) {
                out.push((col, Self::backfill_append_of(schema, field, &fill)));
                continue;
            }
            if Self::is_json_type(&field.field_type) {
                let one = if field.is_nullable() {
                    quote! {
                        self.#col.append_tagged(0u8, "")
                            .expect("Failed to backfill json column");
                    }
                } else {
                    quote! {
                        self.#col.append_string("null")
                            .expect("Failed to backfill json column");
                    }
                };
                out.push((col, one));
            } else if Self::is_enum_type(&field.field_type) {
                let one = if field.is_nullable() {
                    quote! {
                        self.#col.append_bytes(&[0u8, 0u8])
                            .expect("Failed to backfill enum column");
                    }
                } else {
                    quote! {
                        self.#col.append_bytes(&[0u8])
                            .expect("Failed to backfill enum column");
                    }
                };
                out.push((col, one));
            } else if Self::is_inline_string_type(&Self::resolved_type(schema, &field.field_type)) {
                let value_size = Self::column_value_size_expr(schema,
                    &field.field_type,
                    Self::is_utf8_field(field),
                );
                let one = quote! {
                    self.#col.append_bytes(&vec![0u8; #value_size])
                        .expect("Failed to backfill inline string column");
                };
                out.push((col, one));
            } else if Self::is_variable_string_type(&field.field_type) {
                let one = if field.is_nullable() {
                    quote! {
                        self.#col.append_tagged(0u8, "")
                            .expect("Failed to backfill string column");
                    }
                } else {
                    quote! {
                        self.#col.append_string("")
                            .expect("Failed to backfill string column");
                    }
                };
                out.push((col, one));
            } else if Self::is_fixed_size_type(schema, &field.field_type) {
                let needs_byte_conversion = matches!(
                    &Self::resolved_type(schema, &field.field_type),
                    forgedb_parser::FieldType::Bytes(_)
                        | forgedb_parser::FieldType::FixedArray(_, _)
                        | forgedb_parser::FieldType::StructType(_)
                        | forgedb_parser::FieldType::OptionalStructType(_)
                        | forgedb_parser::FieldType::Nullable(_)
                );
                let one = if needs_byte_conversion {
                    let value_size =
                        Self::column_value_size_expr(schema, &field.field_type, Self::is_utf8_field(field));
                    quote! {
                        self.#col.append_bytes(&vec![0u8; #value_size])
                            .expect("Failed to backfill column");
                    }
                } else {
                    let append_method = Self::get_append_method(schema, &field.field_type);
                    let is_uuid_like = matches!(
                        &Self::resolved_type(schema, &field.field_type),
                        forgedb_parser::FieldType::Uuid
                    );
                    let is_timestamp =
                        matches!(&Self::resolved_type(schema, &field.field_type), forgedb_parser::FieldType::Timestamp(_));
                    let is_decimal =
                        matches!(&Self::resolved_type(schema, &field.field_type), forgedb_parser::FieldType::Decimal);
                    if is_decimal {
                        quote! {
                            self.#col.append_uuid(rust_decimal::Decimal::ZERO.serialize())
                                .expect("Failed to backfill column");
                        }
                    } else if is_uuid_like {
                        quote! {
                            self.#col.#append_method([0u8; 16])
                                .expect("Failed to backfill column");
                        }
                    } else if is_timestamp {
                        quote! {
                            self.#col.#append_method(0i64)
                                .expect("Failed to backfill column");
                        }
                    } else if matches!(&Self::resolved_type(schema, &field.field_type), forgedb_parser::FieldType::F64) {
                        quote! {
                            self.#col.#append_method(0.0)
                                .expect("Failed to backfill column");
                        }
                    } else if matches!(&Self::resolved_type(schema, &field.field_type), forgedb_parser::FieldType::Bool) {
                        quote! {
                            self.#col.#append_method(false)
                                .expect("Failed to backfill column");
                        }
                    } else {
                        quote! {
                            self.#col.#append_method(0)
                                .expect("Failed to backfill column");
                        }
                    }
                };
                out.push((col, one));
            }
        }
        out
    }

    fn generate_recover_method(schema: &Schema, model: &forgedb_parser::Model) -> TokenStream {
        let model_name = format_ident!("{}", model.name);
        let append_statements = Self::generate_append_statements(schema, model);
        let col_idents: Vec<_> = model
            .fields
            .iter()
            .filter(|f| {
                Self::is_fixed_size_type(schema, &f.field_type)
                    || Self::is_variable_column_type(&f.field_type)
            })
            .map(|f| format_ident!("{}_col", f.name))
            .collect();

        let backfill_loops: Vec<_> = Self::generate_backfill_appends(schema, model)
            .into_iter()
            .map(|(col, append_default)| {
                quote! {
                    while self.#col.len() < __anchor {
                        #append_default
                    }
                }
            })
            .collect();

        quote! {
            fn recover_from_wal(&mut self) {
                let __anchor = self.tombstones.len();
                #(#backfill_loops)*
                #(
                    self.#col_idents
                        .truncate_to_rows(__anchor)
                        .expect("Failed to truncate column on recovery");
                )*
                self.row_count = __anchor;

                let __entries = self
                    .wal
                    .replay(|_| -> std::io::Result<()> { Ok(()) })
                    .expect("Failed to replay WAL on recovery");
                for __entry in &__entries {
                    if let forgedb_wal::WalOperation::Raw { payload } = &__entry.operation {
                        if payload.len() < 9 {
                            continue;
                        }
                        let mut __ri = [0u8; 8];
                        __ri.copy_from_slice(&payload[0..8]);
                        let __row_index = u64::from_le_bytes(__ri) as usize;
                        if __row_index < self.row_count {
                            continue;
                        }
                        let __deleted = payload[8] != 0;
                        let record: #model_name = match serde_json::from_slice(&payload[9..]) {
                            Ok(r) => r,
                            Err(_) => continue,
                        };
                        #(#append_statements)*
                        self.tombstones
                            .append(__deleted)
                            .expect("Failed to append tombstone on recovery");
                        self.row_count += 1;
                    }
                }
            }
        }
    }

    fn generate_apply_method(model: &forgedb_parser::Model) -> Option<TokenStream> {
        model.identity_field()?;
        let model_name = format_ident!("{}", model.name);
        let id_field = Self::id_field_ident(model);
        Some(quote! {
            pub fn apply(
                &mut self,
                kind: forgedb_changefeed::ChangeKind,
                bytes: &[u8],
            ) -> Result<(), ApplyError> {
                match kind {
                    forgedb_changefeed::ChangeKind::Inserted => {
                        let record: #model_name = serde_json::from_slice(bytes)
                            .map_err(|e| ApplyError::Decode(e.to_string()))?;
                        self.insert(record)?;
                    }
                    forgedb_changefeed::ChangeKind::Updated => {
                        let record: #model_name = serde_json::from_slice(bytes)
                            .map_err(|e| ApplyError::Decode(e.to_string()))?;
                        let id = record.#id_field;
                        self.update(id, record)?;
                    }
                    forgedb_changefeed::ChangeKind::Deleted => {
                        let record: #model_name = serde_json::from_slice(bytes)
                            .map_err(|e| ApplyError::Decode(e.to_string()))?;
                        let id = record.#id_field;
                        self.delete(id);
                    }
                    forgedb_changefeed::ChangeKind::Linked => {
                    }
                }
                Ok(())
            }
        })
    }

    fn generate_commit_method(schema: &Schema, model: &forgedb_parser::Model) -> TokenStream {
        let col_idents: Vec<_> = model
            .fields
            .iter()
            .filter(|f| {
                Self::is_fixed_size_type(schema, &f.field_type)
                    || Self::is_variable_column_type(&f.field_type)
            })
            .map(|f| format_ident!("{}_col", f.name))
            .collect();
        quote! {
            pub fn commit(&mut self) -> std::io::Result<()> {
                #(
                    self.#col_idents.sync_to_drive()?;
                )*
                self.tombstones.sync_to_drive()?;
                self.tombstones.barrier()?;
                Ok(())
            }
        }
    }

    fn generate_checkpoint_method(schema: &Schema, model: &forgedb_parser::Model) -> TokenStream {
        let col_idents: Vec<_> = model
            .fields
            .iter()
            .filter(|f| {
                Self::is_fixed_size_type(schema, &f.field_type)
                    || Self::is_variable_column_type(&f.field_type)
            })
            .map(|f| format_ident!("{}_col", f.name))
            .collect();

        quote! {
            pub fn checkpoint(&mut self) {
                #(
                    self.#col_idents
                        .sync_to_drive()
                        .expect("Failed to sync column to drive on checkpoint");
                )*
                self.tombstones
                    .sync_to_drive()
                    .expect("Failed to sync tombstones to drive on checkpoint");
                self.tombstones
                    .barrier()
                    .expect("Failed to issue checkpoint device barrier");
                self.wal
                    .truncate()
                    .expect("Failed to truncate WAL on checkpoint");
                self.writes_since_checkpoint = 0;
            }
        }
    }

    fn generate_compact_method(model: &forgedb_parser::Model) -> TokenStream {
        let model_snake = Self::to_snake_case(&model.name);

        let index_idents: Vec<proc_macro2::Ident> = Self::indexed_fields(model)
            .iter()
            .map(|f| Self::index_field_ident(f))
            .chain(Self::composite_indexes(model).into_iter().map(|(ident, _)| ident))
            .chain(Self::ordered_index_fields(model).iter().map(|f| Self::ordered_index_ident(f)))
            .collect();
        let save_indexes: Vec<_> = index_idents
            .iter()
            .map(|ident| {
                let saved = format_ident!("__saved_{}", ident);
                quote! { let #saved = std::sync::Arc::clone(&self.#ident); }
            })
            .collect();
        let reinstall_indexes: Vec<_> = index_idents
            .iter()
            .map(|ident| {
                let saved = format_ident!("__saved_{}", ident);
                quote! { self.#ident = #saved; }
            })
            .collect();

        let autoseq_idents: Vec<_> = Self::sequence_auto_fields(model)
            .iter()
            .map(|f| Self::autoseq_field_ident(f))
            .collect();
        let save_autoseqs: Vec<_> = autoseq_idents
            .iter()
            .map(|ident| {
                let saved = format_ident!("__saved_{}", ident);
                quote! { let #saved = std::sync::Arc::clone(&self.#ident); }
            })
            .collect();
        let reinstall_autoseqs: Vec<_> = autoseq_idents
            .iter()
            .map(|ident| {
                let saved = format_ident!("__saved_{}", ident);
                quote! { self.#ident = #saved; }
            })
            .collect();
        let repersist_autoseqs = if autoseq_idents.is_empty() {
            quote! {}
        } else {
            quote! {
                if let Err(__e) = self.write_manifest(&__root) {
                    eprintln!(
                        "forgedb: could not re-persist the auto-increment floor for \
                         '{}' after compaction ({__e}); the pre-compaction floor \
                         still covers every value issued before this call.",
                        #model_snake
                    );
                }
            }
        };

        let prepersist_autoseqs = if autoseq_idents.is_empty() {
            quote! {}
        } else {
            quote! {
                let __root_pre = self.root.clone();
                if let Err(__e) = self.write_manifest(&__root_pre) {
                    eprintln!(
                        "forgedb: refusing to compact '{}' — the auto-increment floor \
                         could not be persisted first ({__e}). Compaction would drop \
                         the rows that are the only other record of the values \
                         already issued, so a crash could re-issue them.",
                        #model_snake
                    );
                    return;
                }
            }
        };

        let remap_versions = if model.identity_field().is_some() {
            quote! { __new_id_versions.insert(__id, vec![__new_row]); }
        } else {
            quote! {}
        };
        let assign_versions = if model.identity_field().is_some() {
            quote! { self.id_versions = std::sync::Arc::new(__new_id_versions); }
        } else {
            quote! {}
        };
        let decl_versions = if model.identity_field().is_some() {
            quote! {
                let mut __new_id_versions: std::collections::HashMap<_, Vec<usize>> =
                    std::collections::HashMap::with_capacity(__old_id_to_row.len());
            }
        } else {
            quote! {}
        };

        quote! {
            pub fn compact(&mut self) {
                if self.in_transaction {
                    self.compact_deferred = true;
                    return;
                }
                self.checkpoint();
                let mut __keep: Vec<usize> = Vec::with_capacity(self.id_to_row.len());
                for &__row in self.id_to_row.values() {
                    if !self.tombstones.is_deleted(__row).unwrap_or(false) {
                        __keep.push(__row);
                    }
                }
                #prepersist_autoseqs
                let __config = forgedb_compaction::CompactionConfig::default();
                let __compactor = forgedb_compaction::Compactor::new(&self.root, __config);
                if __compactor.compact_model_keeping(#model_snake, &__keep).is_ok() {
                    let mut __keep_sorted = __keep;
                    __keep_sorted.sort_unstable();
                    let __old_id_to_row = std::sync::Arc::clone(&self.id_to_row);
                    #(#save_indexes)*
                    #(#save_autoseqs)*
                    let __root = self.root.clone();
                    let __feed = self.changefeed.take();
                    let __broker = self.broker.take();
                    *self = Self::new_at_no_rehydrate(&__root);
                    self.changefeed = __feed;
                    self.broker = __broker;
                    #(#reinstall_indexes)*
                    #(#reinstall_autoseqs)*
                    #repersist_autoseqs
                    let mut __new_id_to_row: std::collections::HashMap<_, usize> =
                        std::collections::HashMap::with_capacity(__old_id_to_row.len());
                    #decl_versions
                    for (&__id, &__old_row) in __old_id_to_row.iter() {
                        if __keep_sorted.binary_search(&__old_row).is_ok() {
                            let __new_row = __keep_sorted.partition_point(|&__r| __r < __old_row);
                            __new_id_to_row.insert(__id, __new_row);
                            #remap_versions
                        }
                    }
                    self.id_to_row = std::sync::Arc::new(__new_id_to_row);
                    #assign_versions
                    self.bump_compaction_epoch(&__root);
                }
                self.dead_since_compaction = 0;
                self.compaction_due = false;
            }

            pub fn maintain(&mut self) {
                if self.compaction_due && !self.in_transaction {
                    self.compaction_due = false;
                    self.compact();
                }
            }
        }
    }

    fn generate_txn_storage_methods(schema: &Schema, model: &forgedb_parser::Model) -> TokenStream {
        if !model.has_identity() {
            return quote! {};
        }
        let model_name = format_ident!("{}", model.name);
        let append_statements = Self::generate_append_statements(schema, model);
        let wal_write_live = Self::generate_wal_record_write_buffered(model, false);
        let wal_write_deleted = Self::generate_wal_record_write_buffered(model, true);
        let shared_record_json = Self::generate_shared_record_json();
        let col_idents: Vec<_> = model
            .fields
            .iter()
            .filter(|f| {
                Self::is_fixed_size_type(schema, &f.field_type)
                    || Self::is_variable_column_type(&f.field_type)
            })
            .map(|f| format_ident!("{}_col", f.name))
            .collect();
        let clear_indexes: Vec<_> = Self::indexed_fields(model)
            .iter()
            .map(|f| {
                let ident = Self::index_field_ident(f);
                quote! { std::sync::Arc::make_mut(&mut self.#ident).clear(); }
            })
            .chain(Self::composite_indexes(model).iter().map(|(ident, _)| {
                quote! { std::sync::Arc::make_mut(&mut self.#ident).clear(); }
            }))
            .collect();
        let rehydrate_self = Self::generate_rehydrate_body(schema, model, &quote! { self });
        let clear_versions = if model.identity_field().is_some() {
            quote! { std::sync::Arc::make_mut(&mut self.id_versions).clear(); }
        } else {
            quote! {}
        };
        let reindex_delta = Self::generate_reindex_delta_method(schema, model);
        quote! {
            pub fn __reindex_committed(&mut self) {
                std::sync::Arc::make_mut(&mut self.id_to_row).clear();
                #clear_versions
                #(#clear_indexes)*
                #rehydrate_self
            }

            #reindex_delta

            pub fn __sync_columns_from_disk(&mut self) {
                #(
                    let _ = self.#col_idents.sync_from_disk();
                )*
                let _ = self.tombstones.sync_from_disk();
                self.row_count = self.tombstones.len();
            }

            pub fn __truncate_all_to(&mut self, rows: usize) -> std::io::Result<()> {
                #(
                    self.#col_idents.truncate_to_rows(rows)?;
                )*
                self.tombstones.truncate_to_rows(rows)?;
                Ok(())
            }

            pub fn __recover_to_committed(&mut self, committed_len: usize) {
                if committed_len >= self.row_count {
                    return;
                }
                #(
                    self.#col_idents
                        .truncate_to_rows(committed_len)
                        .expect("Failed to truncate column on journal recovery");
                )*
                self.tombstones
                    .truncate_to_rows(committed_len)
                    .expect("Failed to truncate tombstones on journal recovery");
                self.row_count = committed_len;
                self.wal
                    .truncate()
                    .expect("Failed to truncate WAL on journal recovery");
                let __root = self.root.clone();
                let __feed = self.changefeed.take();
                let __broker = self.broker.take();
                *self = Self::new_at(&__root);
                self.changefeed = __feed;
                self.broker = __broker;
            }

            pub fn __stage_append(&mut self, record: #model_name, deleted: bool) -> usize {
                let row_index = self.row_count;
                #shared_record_json
                if deleted {
                    #wal_write_deleted
                } else {
                    #wal_write_live
                }
                #(#append_statements)*
                self.tombstones
                    .append(deleted)
                    .expect("Failed to append tombstone");
                self.row_count += 1;
                row_index
            }

            pub fn run_deferred_maintenance(&mut self) {
                if self.compact_deferred {
                    self.compact_deferred = false;
                    self.checkpoint_deferred = false;
                    self.compact();
                } else if self.checkpoint_deferred {
                    self.checkpoint_deferred = false;
                    self.checkpoint();
                }
            }
        }
    }

    fn generate_reindex_delta_method(schema: &Schema, model: &forgedb_parser::Model) -> TokenStream {
        let row_var = format_ident!("__r");
        let id_read = match Self::generate_id_read_expr(schema, model, &quote! { self }, &row_var) {
            Some(expr) => expr,
            None => return quote! {},
        };
        let recv = quote! { self };
        let id_tok = quote! { id };
        let indexed = Self::indexed_fields(model);
        let composites = Self::composite_indexes(model);
        let has_index = !indexed.is_empty() || !composites.is_empty();

        let old = quote! { __old_rec };
        let (rem_hoist, rem_map) = Self::hoist_index_keys(schema, model, &old, "rem");
        let single_removes: Vec<_> = indexed
            .iter()
            .map(|f| {
                let ident = Self::index_field_ident(f);
                let key = Self::field_key_token(schema, f, &old, &rem_map);
                Self::index_remove_block(&recv, &ident, key, &id_tok)
            })
            .collect();
        let composite_removes: Vec<_> = composites
            .iter()
            .map(|(ident, comps)| {
                let parts: Vec<_> =
                    comps.iter().map(|c| Self::field_key_token(schema, c, &old, &rem_map)).collect();
                Self::composite_remove_block(&recv, ident, &parts, &id_tok)
            })
            .collect();
        let ordered_removes: Vec<_> = Self::ordered_index_remove_maint(model, &recv, &old, &id_tok);

        let rec = quote! { __new_rec };
        let (add_hoist, add_map) = Self::hoist_index_keys(schema, model, &rec, "add");
        let single_adds: Vec<_> = indexed
            .iter()
            .map(|f| {
                let ident = Self::index_field_ident(f);
                let key = Self::field_key_token(schema, f, &rec, &add_map);
                Self::index_add_block(&recv, &ident, key, &id_tok)
            })
            .collect();
        let ordered_adds: Vec<_> = Self::ordered_index_add_maint(model, &recv, &rec, &id_tok);
        let composite_adds: Vec<_> = composites
            .iter()
            .map(|(ident, comps)| {
                let parts: Vec<_> =
                    comps.iter().map(|c| Self::field_key_token(schema, c, &rec, &add_map)).collect();
                Self::composite_add_block(&recv, ident, &parts, &id_tok)
            })
            .collect();

        let remove_old = if has_index {
            quote! {
                if let Some(__old_rec) = self.get(id) {
                    #(#rem_hoist)*
                    #(#single_removes)*
                    #(#ordered_removes)*
                    #(#composite_removes)*
                }
            }
        } else {
            quote! {}
        };
        let add_new = if has_index {
            quote! {
                if let Some(__new_rec) = self.read_at(__r) {
                    #(#add_hoist)*
                    #(#single_adds)*
                    #(#ordered_adds)*
                    #(#composite_adds)*
                }
            }
        } else {
            quote! {}
        };
        let versions_push =
            Self::id_versions_push_stmt(model, &recv, &id_tok, &quote! { __r });

        let autoseq_delta = {
            let identity = model.identity_field().map(|f| f.name.as_str());
            let folds: Vec<TokenStream> = Self::sequence_auto_fields(model)
                .iter()
                .map(|f| {
                    let seq = Self::autoseq_field_ident(f);
                    if Some(f.name.as_str()) == identity {
                        let as_u64 = Self::autoseq_to_u64(f, quote! { id });
                        quote! {
                            self.#seq.fetch_max(#as_u64, std::sync::atomic::Ordering::SeqCst);
                        }
                    } else {
                        let col = format_ident!("{}_col", f.name);
                        let read_method = Self::get_read_method(schema, &f.field_type);
                        let as_u64 = Self::autoseq_to_u64(f, quote! { __v });
                        quote! {
                            if let Ok(__v) = self.#col.#read_method(__r) {
                                self.#seq.fetch_max(
                                    #as_u64,
                                    std::sync::atomic::Ordering::SeqCst,
                                );
                            }
                        }
                    }
                })
                .collect();
            quote! { #(#folds)* }
        };

        quote! {
            pub fn __reindex_delta(&mut self, from: usize) {
                let __n = self.row_count;
                for __r in from..__n {
                    let id = #id_read;
                    #remove_old
                    let __deleted = self.tombstones.is_deleted(__r).unwrap_or(false);
                    std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, __r);
                    #versions_push
                    #autoseq_delta
                    if !__deleted {
                        #add_new
                    }
                }
            }
        }
    }

    fn generate_maybe_checkpoint() -> TokenStream {
        quote! {
            self.writes_since_checkpoint += 1;
            if self.writes_since_checkpoint >= WAL_CHECKPOINT_INTERVAL {
                if self.in_transaction {
                    self.checkpoint_deferred = true;
                } else {
                    self.checkpoint();
                }
            }
        }
    }

    fn generate_maybe_compact() -> TokenStream {
        if !Self::active_cfg().compaction {
            return quote! {
                self.dead_since_compaction += 1;
            };
        }
        quote! {
            self.dead_since_compaction += 1;
            if self.dead_since_compaction >= COMPACTION_DEAD_THRESHOLD {
                if self.in_transaction {
                    self.compact_deferred = true;
                } else if self.dead_since_compaction
                    >= COMPACTION_DEAD_THRESHOLD * COMPACTION_DEAD_CEILING_FACTOR
                {
                    self.compaction_due = false;
                    self.compact();
                } else {
                    self.compaction_due = true;
                }
            }
        }
    }

    fn generate_insert_logic(schema: &Schema, model: &forgedb_parser::Model) -> TokenStream {
        let append_statements = Self::generate_append_statements(schema, model);
        let id_field_name = Self::id_field_ident(model);
        let model_name_str = model.name.clone();
        let wal_write = Self::generate_wal_record_write(model, false);
        let shared_record_json = Self::generate_shared_record_json();
        let broker_record =
            Self::generate_broker_record(model, quote! { forgedb_changefeed::ChangeKind::Inserted });
        let maybe_checkpoint = Self::generate_maybe_checkpoint();
        let validate_fn = format_ident!("validate_{}", Self::to_snake_case(&model.name));
        let unique_checks = Self::generate_unique_checks(schema, model, false);
        let ts_gate = Self::generate_timestamp_write_gate(schema, model);

        let recv = quote! { self };
        let id_tok = quote! { id };
        let rec = quote! { record };
        let (add_hoist, add_map) = Self::hoist_index_keys(schema, model, &rec, "add");
        let index_adds: Vec<_> = Self::indexed_fields(model)
            .iter()
            .map(|f| {
                let ident = Self::index_field_ident(f);
                let key = Self::field_key_token(schema, f, &rec, &add_map);
                Self::index_add_block(&recv, &ident, key, &id_tok)
            })
            .collect();
        let ordered_adds: Vec<_> = Self::ordered_index_add_maint(model, &recv, &rec, &id_tok);
        let composite_adds: Vec<_> = Self::composite_indexes(model)
            .iter()
            .map(|(ident, comps)| {
                let parts: Vec<_> = comps
                    .iter()
                    .map(|c| Self::field_key_token(schema, c, &rec, &add_map))
                    .collect();
                Self::composite_add_block(&recv, ident, &parts, &id_tok)
            })
            .collect();
        let versions_push = Self::id_versions_push_stmt(model, &recv, &id_tok, &quote! { row_index });

        quote! {
            #ts_gate
            #validate_fn(&record)?;
            #(#unique_checks)*

            let row_index = self.row_count;
            let id = record.#id_field_name;

            #shared_record_json

            #wal_write

            #(#append_statements)*

            self.tombstones.append(false).expect("Failed to append tombstone");

            std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, row_index);
            #versions_push
            self.row_count += 1;

            #(#add_hoist)*
            #(#index_adds)*
            #(#ordered_adds)*
            #(#composite_adds)*

            if let Some(feed) = &self.changefeed {
                feed.emit(#model_name_str, row_index, forgedb_changefeed::ChangeKind::Inserted);
            }
            #broker_record

            #maybe_checkpoint

            Ok(id)
        }
    }

    fn generate_update_logic(schema: &Schema, model: &forgedb_parser::Model) -> Option<TokenStream> {
        model.identity_field()?;
        let append_statements = Self::generate_append_statements(schema, model);
        let model_name_str = model.name.clone();
        let wal_write = Self::generate_wal_record_write(model, false);
        let shared_record_json = Self::generate_shared_record_json();
        let broker_record =
            Self::generate_broker_record(model, quote! { forgedb_changefeed::ChangeKind::Updated });
        let maybe_checkpoint = Self::generate_maybe_checkpoint();
        let maybe_compact = Self::generate_maybe_compact();
        let validate_fn = format_ident!("validate_{}", Self::to_snake_case(&model.name));
        let unique_checks = Self::generate_unique_checks(schema, model, true);
        let ts_gate = Self::generate_timestamp_write_gate(schema, model);

        let indexed = Self::indexed_fields(model);
        let composites = Self::composite_indexes(model);
        let fetch_old = if indexed.is_empty() && composites.is_empty() {
            quote! {}
        } else {
            quote! { let __old = self.get(id); }
        };
        let recv = quote! { self };
        let id_tok = quote! { id };
        let rec = quote! { record };
        let old = quote! { __old_rec };
        let (add_hoist, add_map) = Self::hoist_index_keys(schema, model, &rec, "add");
        let (rem_hoist, rem_map) = Self::hoist_index_keys(schema, model, &old, "rem");
        let single_removes: Vec<_> = indexed
            .iter()
            .map(|f| {
                let ident = Self::index_field_ident(f);
                let key = Self::field_key_token(schema, f, &old, &rem_map);
                Self::index_remove_block(&recv, &ident, key, &id_tok)
            })
            .collect();
        let single_adds: Vec<_> = indexed
            .iter()
            .map(|f| {
                let ident = Self::index_field_ident(f);
                let key = Self::field_key_token(schema, f, &rec, &add_map);
                Self::index_add_block(&recv, &ident, key, &id_tok)
            })
            .collect();
        let ordered_removes: Vec<_> = Self::ordered_index_remove_maint(model, &recv, &old, &id_tok);
        let ordered_adds: Vec<_> = Self::ordered_index_add_maint(model, &recv, &rec, &id_tok);
        let composite_removes: Vec<_> = composites
            .iter()
            .map(|(ident, comps)| {
                let parts: Vec<_> = comps
                    .iter()
                    .map(|c| Self::field_key_token(schema, c, &old, &rem_map))
                    .collect();
                Self::composite_remove_block(&recv, ident, &parts, &id_tok)
            })
            .collect();
        let composite_adds: Vec<_> = composites
            .iter()
            .map(|(ident, comps)| {
                let parts: Vec<_> = comps
                    .iter()
                    .map(|c| Self::field_key_token(schema, c, &rec, &add_map))
                    .collect();
                Self::composite_add_block(&recv, ident, &parts, &id_tok)
            })
            .collect();
        let index_remove_maint = if indexed.is_empty() && composites.is_empty() {
            quote! {}
        } else {
            quote! {
                if let Some(__old_rec) = &__old {
                    #(#rem_hoist)*
                    #(#single_removes)*
                    #(#ordered_removes)*
                    #(#composite_removes)*
                }
            }
        };
        let versions_push = Self::id_versions_push_stmt(model, &recv, &id_tok, &quote! { row_index });

        Some(quote! {
            if !self.id_to_row.contains_key(&id) {
                return Ok(false);
            }
            #ts_gate
            #validate_fn(&record)?;
            #(#unique_checks)*
            #fetch_old
            let row_index = self.row_count;

            #shared_record_json

            #wal_write

            #(#append_statements)*

            self.tombstones.append(false).expect("Failed to append tombstone");

            std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, row_index);
            #versions_push
            self.row_count += 1;

            #index_remove_maint
            #(#add_hoist)*
            #(#single_adds)*
            #(#ordered_adds)*
            #(#composite_adds)*

            if let Some(feed) = &self.changefeed {
                feed.emit(#model_name_str, row_index, forgedb_changefeed::ChangeKind::Updated);
            }
            #broker_record

            #maybe_checkpoint

            #maybe_compact

            Ok(true)
        })
    }

    fn generate_delete_logic(schema: &Schema, model: &forgedb_parser::Model) -> Option<TokenStream> {
        model.identity_field()?;
        let append_statements = Self::generate_append_statements(schema, model);
        let model_name_str = model.name.clone();
        let wal_write = Self::generate_wal_record_write(model, true);
        let shared_record_json = Self::generate_shared_record_json();
        let broker_record =
            Self::generate_broker_record(model, quote! { forgedb_changefeed::ChangeKind::Deleted });
        let maybe_checkpoint = Self::generate_maybe_checkpoint();
        let maybe_compact = Self::generate_maybe_compact();

        let recv = quote! { self };
        let id_tok = quote! { id };
        let rec = quote! { record };
        let (rem_hoist, rem_map) = Self::hoist_index_keys(schema, model, &rec, "rem");
        let index_removes: Vec<_> = Self::indexed_fields(model)
            .iter()
            .map(|f| {
                let ident = Self::index_field_ident(f);
                let key = Self::field_key_token(schema, f, &rec, &rem_map);
                Self::index_remove_block(&recv, &ident, key, &id_tok)
            })
            .collect();
        let ordered_removes: Vec<_> = Self::ordered_index_remove_maint(model, &recv, &rec, &id_tok);
        let composite_removes: Vec<_> = Self::composite_indexes(model)
            .iter()
            .map(|(ident, comps)| {
                let parts: Vec<_> = comps
                    .iter()
                    .map(|c| Self::field_key_token(schema, c, &rec, &rem_map))
                    .collect();
                Self::composite_remove_block(&recv, ident, &parts, &id_tok)
            })
            .collect();
        let versions_push = Self::id_versions_push_stmt(model, &recv, &id_tok, &quote! { row_index });

        Some(quote! {
            let record = match self.get(id) {
                Some(r) => r,
                None => return false,
            };
            let deleted_row = *self
                .id_to_row
                .get(&id)
                .expect("id present: get succeeded above");

            let row_index = self.row_count;

            #shared_record_json

            #wal_write

            #(#append_statements)*

            self.tombstones.append(true).expect("Failed to append tombstone");

            std::sync::Arc::make_mut(&mut self.id_to_row).insert(id, row_index);
            #versions_push
            self.row_count += 1;

            #(#rem_hoist)*
            #(#index_removes)*
            #(#ordered_removes)*
            #(#composite_removes)*

            if let Some(feed) = &self.changefeed {
                feed.emit(#model_name_str, deleted_row, forgedb_changefeed::ChangeKind::Deleted);
            }
            #broker_record

            #maybe_checkpoint

            #maybe_compact

            true
        })
    }

    fn generate_index_lookups(schema: &Schema, model: &forgedb_parser::Model) -> TokenStream {
        let probes = Self::generate_index_probes(schema, model, true);
        let ranges = Self::generate_ordered_range_methods(schema, model);
        quote! { #probes #ranges }
    }

    fn generate_ordered_range_methods(schema: &Schema, model: &forgedb_parser::Model) -> TokenStream {
        let model_name = format_ident!("{}", model.name);
        let methods: Vec<TokenStream> = Self::ordered_index_fields(model)
            .iter()
            .map(|f| {
                let ident = Self::ordered_index_ident(f);
                let kty = Self::ordered_key_type(f).expect("ordered_index_fields filtered");
                let pty = Self::ordered_param_type(f).expect("ordered_index_fields filtered");
                let id_type = Self::id_type_tokens(schema, model);
                let range_fn = format_ident!("find_by_{}_range", f.name);
                let lo_norm = Self::ordered_key_expr(&f.field_type, quote! { __v });
                let hi_norm = Self::ordered_key_expr(&f.field_type, quote! { __v });
                quote! {
                    pub fn #range_fn(
                        &self,
                        min: Option<#pty>,
                        max: Option<#pty>,
                        descending: bool,
                        limit: Option<usize>,
                    ) -> Vec<#model_name> {
                        let __lo_v: Option<#kty> = min.map(|__v| #lo_norm);
                        let __hi_v: Option<#kty> = max.map(|__v| #hi_norm);
                        if let (Some(__l), Some(__h)) = (__lo_v.as_ref(), __hi_v.as_ref()) {
                            if __l > __h {
                                return Vec::new();
                            }
                        }
                        let __lo = match __lo_v {
                            Some(__v) => std::ops::Bound::Included(__v),
                            None => std::ops::Bound::Unbounded,
                        };
                        let __hi = match __hi_v {
                            Some(__v) => std::ops::Bound::Included(__v),
                            None => std::ops::Bound::Unbounded,
                        };
                        let mut __out: Vec<#model_name> = Vec::new();
                        let __iter: Box<dyn Iterator<Item = (&#kty, &std::collections::BTreeSet<#id_type>)> + '_> =
                            if descending {
                                Box::new(self.#ident.range((__lo, __hi)).rev())
                            } else {
                                Box::new(self.#ident.range((__lo, __hi)))
                            };
                        'outer: for (_k, __ids) in __iter {
                            for &__id in __ids {
                                if let Some(__r) = self.get(__id) {
                                    __out.push(__r);
                                    if let Some(__n) = limit {
                                        if __out.len() >= __n {
                                            break 'outer;
                                        }
                                    }
                                }
                            }
                        }
                        __out
                    }
                }
            })
            .collect();
        quote! { #(#methods)* }
    }

    fn generate_index_probes(schema: &Schema, model: &forgedb_parser::Model, include_live: bool) -> TokenStream {
        let model_name = format_ident!("{}", model.name);
        let mut methods: Vec<TokenStream> = Vec::new();

        for f in Self::indexed_fields(model) {
            let ident = Self::index_field_ident(f);
            let fname = format_ident!("{}", f.name);
            let param_ty = Self::index_param_type(schema, f);
            let find_fn = format_ident!("find_by_{}", f.name);
            let find_at_fn = format_ident!("find_by_{}_at", f.name);
            let params = quote! { value: #param_ty };
            let key_from_arg =
                Self::index_key_expr(schema, &f.field_type, Self::index_value_expr(&f.field_type, quote! { value }));
            let key_from_rec = Self::index_key_expr(schema, &f.field_type, Self::index_value_expr(&f.field_type,
                quote! { __rec.#fname },
            ));
            let unique = if f.unique {
                Some((
                    format_ident!("get_by_{}", f.name),
                    format_ident!("get_by_{}_at", f.name),
                ))
            } else {
                None
            };
            methods.push(Self::emit_probe(
                &model_name,
                &ident,
                &find_fn,
                &find_at_fn,
                &params,
                &key_from_arg,
                &key_from_rec,
                include_live,
                unique.as_ref(),
            ));
        }

        for (ident, comps) in Self::composite_indexes(model) {
            let find_fn = Self::composite_probe_ident(&comps);
            let find_at_fn = format_ident!("{}_at", find_fn);
            let params_list: Vec<_> = comps
                .iter()
                .map(|c| {
                    let pn = format_ident!("{}", c.name);
                    let pt = Self::index_param_type(schema, c);
                    quote! { #pn: #pt }
                })
                .collect();
            let params = quote! { #(#params_list),* };
            let arg_keys: Vec<_> = comps
                .iter()
                .map(|c| {
                    let pn = format_ident!("{}", c.name);
                    Self::index_key_expr(schema, &c.field_type, Self::index_value_expr(&c.field_type, quote! { #pn }))
                })
                .collect();
            let rec_keys: Vec<_> = comps
                .iter()
                .map(|c| {
                    let cf = format_ident!("{}", c.name);
                    Self::index_key_expr(schema, &c.field_type, Self::index_value_expr(&c.field_type,
                        quote! { __rec.#cf },
                    ))
                })
                .collect();
            let key_from_arg = Self::composite_key_build(&arg_keys);
            let key_from_rec = Self::composite_key_build(&rec_keys);
            methods.push(Self::emit_probe(
                &model_name,
                &ident,
                &find_fn,
                &find_at_fn,
                &params,
                &key_from_arg,
                &key_from_rec,
                include_live,
                None,
            ));
        }

        quote! { #(#methods)* }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_probe(
        model_name: &proc_macro2::Ident,
        index_ident: &proc_macro2::Ident,
        find_fn: &proc_macro2::Ident,
        find_at_fn: &proc_macro2::Ident,
        params: &TokenStream,
        key_from_arg: &TokenStream,
        key_from_rec: &TokenStream,
        include_live: bool,
        unique: Option<&(proc_macro2::Ident, proc_macro2::Ident)>,
    ) -> TokenStream {
        let mut m = TokenStream::new();

        if include_live {
            m.extend(quote! {
                pub fn #find_fn(&self, #params) -> Vec<#model_name> {
                    let __k: String = { #key_from_arg };
                    let __ids = match self.#index_ident.get(&__k) {
                        Some(__s) => __s,
                        None => return Vec::new(),
                    };
                    __ids.iter().filter_map(|&__id| self.get(__id)).collect()
                }
            });
        }

        m.extend(quote! {
            pub fn #find_at_fn(
                &self,
                snap: &forgedb_storage::Snapshot,
                #params
            ) -> Vec<#model_name> {
                let __k: String = { #key_from_arg };
                let __ids = match self.#index_ident.get(&__k) {
                    Some(__s) => __s,
                    None => return Vec::new(),
                };
                let mut __out = Vec::new();
                for &__id in __ids {
                    if let Some(__rec) = self.get_at(snap, __id) {
                        let __rk: String = { #key_from_rec };
                        if __rk == __k {
                            __out.push(__rec);
                        }
                    }
                }
                __out
            }
        });

        if let Some((get_fn, get_at_fn)) = unique {
            if include_live {
                m.extend(quote! {
                    pub fn #get_fn(&self, #params) -> Option<#model_name> {
                        let __k: String = { #key_from_arg };
                        let __ids = self.#index_ident.get(&__k)?;
                        __ids.iter().find_map(|&__id| self.get(__id))
                    }
                });
            }
            m.extend(quote! {
                pub fn #get_at_fn(
                    &self,
                    snap: &forgedb_storage::Snapshot,
                    #params
                ) -> Option<#model_name> {
                    let __k: String = { #key_from_arg };
                    let __ids = match self.#index_ident.get(&__k) {
                        Some(__s) => __s,
                        None => return None,
                    };
                    for &__id in __ids {
                        if let Some(__rec) = self.get_at(snap, __id) {
                            let __rk: String = { #key_from_rec };
                            if __rk == __k {
                                return Some(__rec);
                            }
                        }
                    }
                    None
                }
            });
        }

        m
    }

    fn field_read_stmt(
        schema: &Schema,
        field: &forgedb_parser::Field,
        receiver: &TokenStream,
        row_index: &TokenStream,
        borrowed: bool,
        as_key: bool,
    ) -> Option<(proc_macro2::Ident, TokenStream)> {
        let field_col_name = format_ident!("{}_col", field.name);
        let field_value_name = format_ident!("{}_value", field.name);

        if Self::is_enum_type(&field.field_type) {
            let enum_ident = Self::enum_type_ident(&field.field_type)
                .expect("enum field has an enum type");
            let stmt = if field.is_nullable() {
                quote! {
                    let #field_value_name = {
                        let bytes = #receiver.#field_col_name.read_bytes(#row_index)
                            .expect("Failed to read from column");
                        if bytes.first() == Some(&1u8) {
                            Some(#enum_ident::__from_u8(bytes[1]))
                        } else {
                            None
                        }
                    };
                }
            } else {
                quote! {
                    let #field_value_name = {
                        let bytes = #receiver.#field_col_name.read_bytes(#row_index)
                            .expect("Failed to read from column");
                        #enum_ident::__from_u8(bytes[0])
                    };
                }
            };
            Some((field_value_name, stmt))
        } else if Self::is_json_type(&field.field_type) {
            let stmt = if field.is_nullable() {
                quote! {
                    let #field_value_name = {
                        let raw = #receiver.#field_col_name.read_string(#row_index)
                            .expect("Failed to read string");
                        if raw.as_bytes().first() == Some(&1u8) {
                            Some(serde_json::from_str(&raw[1..])
                                .expect("Failed to deserialize json"))
                        } else {
                            None
                        }
                    };
                }
            } else {
                quote! {
                    let #field_value_name = {
                        let raw = #receiver.#field_col_name.read_string(#row_index)
                            .expect("Failed to read string");
                        serde_json::from_str(&raw)
                            .expect("Failed to deserialize json")
                    };
                }
            };
            Some((field_value_name, stmt))
        } else if Self::is_variable_string_type(&field.field_type) && borrowed {
            let stmt = if field.is_nullable() {
                quote! {
                    let #field_value_name = {
                        let raw = #receiver.#field_col_name.read_str(#row_index)
                            .expect("Failed to read string");
                        if raw.as_bytes().first() == Some(&1u8) {
                            Some(&raw[1..])
                        } else {
                            None
                        }
                    };
                }
            } else {
                quote! {
                    let #field_value_name = #receiver.#field_col_name.read_str(#row_index)
                        .expect("Failed to read string");
                }
            };
            Some((field_value_name, stmt))
        } else if let Some((chars, exact)) =
            Self::inline_string_column_params(schema, &field.field_type)
        {
            let as_key =
                as_key || Self::fk_backing_type(schema, &field.field_type).is_some();
            let utf8 = Self::is_utf8_field(field);
            let (_, prefix, _) = Self::inline_string_layout(chars, exact, utf8);
            let base = if field.is_nullable() { 1usize } else { 0 };
            let key_ty = if as_key {
                Some(Self::key_type_ident(schema, &field.field_type))
            } else {
                None
            };
            let materialize = |s: TokenStream| match &key_ty {
                Some(kt) => quote! {
                    <#kt>::try_from(#s).unwrap_or_default()
                },
                None => s,
            };
            let stmt = if borrowed {
                if !field.is_nullable() && prefix == 0 {
                    if let Some(kt) = &key_ty {
                        quote! {
                            let #field_value_name = <#kt>::try_from(
                                #receiver.#field_col_name
                                    .read_str(#row_index)
                                    .expect("Failed to read inline string"),
                            )
                            .unwrap_or_default();
                        }
                    } else {
                        quote! {
                            let #field_value_name = #receiver.#field_col_name
                                .read_str(#row_index)
                                .expect("Failed to read inline string");
                        }
                    }
                } else {
                    let bytes = Self::inline_string_bytes_expr(
                        chars,
                        exact,
                        utf8,
                        base,
                        quote! { __slot_bytes },
                    );
                    let decode = materialize(quote! {
                        std::str::from_utf8(#bytes)
                            .expect("inline string column holds UTF-8")
                    });
                    if field.is_nullable() {
                        quote! {
                            let #field_value_name = {
                                let __slot_bytes = #receiver.#field_col_name
                                    .read_slice(#row_index)
                                    .expect("Failed to read inline string");
                                if __slot_bytes[0] == 1u8 { Some(#decode) } else { None }
                            };
                        }
                    } else {
                        quote! {
                            let #field_value_name = {
                                let __slot_bytes = #receiver.#field_col_name
                                    .read_slice(#row_index)
                                    .expect("Failed to read inline string");
                                #decode
                            };
                        }
                    }
                }
            } else {
                let bytes = Self::inline_string_bytes_expr(
                    chars,
                    exact,
                    utf8,
                    base,
                    quote! { __slot_bytes },
                );
                let decode = if as_key {
                    let key_ty = Self::key_type_ident(schema, &field.field_type);
                    quote! {
                        <#key_ty>::try_from(
                            std::str::from_utf8(#bytes)
                                .expect("inline string column holds UTF-8"),
                        )
                        .unwrap_or_default()
                    }
                } else {
                    quote! {
                        std::str::from_utf8(#bytes)
                            .expect("inline string column holds UTF-8")
                            .to_string()
                    }
                };
                if field.is_nullable() {
                    quote! {
                        let #field_value_name = {
                            let __slot_bytes = #receiver.#field_col_name
                                .read_bytes(#row_index)
                                .expect("Failed to read inline string");
                            if __slot_bytes[0] == 1u8 { Some(#decode) } else { None }
                        };
                    }
                } else {
                    quote! {
                        let #field_value_name = {
                            let __slot_bytes = #receiver.#field_col_name
                                .read_bytes(#row_index)
                                .expect("Failed to read inline string");
                            #decode
                        };
                    }
                }
            };
            Some((field_value_name, stmt))
        } else if Self::is_variable_string_type(&field.field_type) {
            let stmt = if field.is_nullable() {
                quote! {
                    let #field_value_name = {
                        let raw = #receiver.#field_col_name.read_string(#row_index)
                            .expect("Failed to read string");
                        if raw.as_bytes().first() == Some(&1u8) {
                            Some(raw[1..].to_string())
                        } else {
                            None
                        }
                    };
                }
            } else {
                quote! {
                    let #field_value_name = #receiver.#field_col_name.read_string(#row_index)
                        .expect("Failed to read string");
                }
            };
            Some((field_value_name, stmt))
        } else if Self::is_fixed_size_type(schema, &field.field_type) {
            let needs_byte_conversion = matches!(
                &Self::resolved_type(schema, &field.field_type),
                forgedb_parser::FieldType::Bytes(_)
                    | forgedb_parser::FieldType::FixedArray(_, _)
                    | forgedb_parser::FieldType::StructType(_)
                    | forgedb_parser::FieldType::OptionalStructType(_)
                    | forgedb_parser::FieldType::Nullable(_)
            );

            let stmt = if needs_byte_conversion {
                let stored_type = Self::stored_type_tokens(schema, &field.field_type, field.is_nullable());
                quote! {
                    let #field_value_name: #stored_type = {
                        let bytes = #receiver.#field_col_name.read_bytes(#row_index)
                            .expect("Failed to read from column");
                        unsafe {
                            std::ptr::read_unaligned(bytes.as_ptr() as *const #stored_type)
                        }
                    };
                }
            } else {
                let read_method = Self::get_read_method(schema, &field.field_type);

                let is_uuid_like = matches!(
                    &Self::resolved_type(schema, &field.field_type),
                    forgedb_parser::FieldType::Uuid
                );
                let is_timestamp =
                    matches!(&Self::resolved_type(schema, &field.field_type), forgedb_parser::FieldType::Timestamp(_));
                let is_decimal =
                    matches!(&Self::resolved_type(schema, &field.field_type), forgedb_parser::FieldType::Decimal);

                if is_decimal {
                    quote! {
                        let #field_value_name = {
                            let bytes = #receiver.#field_col_name.read_uuid(#row_index)
                                .expect("Failed to read from column");
                            rust_decimal::Decimal::deserialize(bytes)
                        };
                    }
                } else if is_uuid_like {
                    quote! {
                        let #field_value_name = {
                            let bytes = #receiver.#field_col_name.#read_method(#row_index)
                                .expect("Failed to read from column");
                            Uuid::from_bytes(bytes)
                        };
                    }
                } else if is_timestamp {
                    quote! {
                        let #field_value_name = Timestamp::from(
                            #receiver.#field_col_name.#read_method(#row_index)
                                .expect("Failed to read from column"),
                        );
                    }
                } else {
                    quote! {
                        let #field_value_name = #receiver.#field_col_name.#read_method(#row_index)
                            .expect("Failed to read from column");
                    }
                }
            };
            Some((field_value_name, stmt))
        } else {
            None
        }
    }

    pub(crate) fn timestamp_schema_value_type(
        schema: &Schema,
        field_type: &forgedb_parser::FieldType,
    ) -> Option<TokenStream> {
        Self::schema_value_type(schema, field_type, false)
    }

    pub(crate) fn schema_value_type(
        schema: &Schema,
        field_type: &forgedb_parser::FieldType,
        is_key: bool,
    ) -> Option<TokenStream> {
        use forgedb_parser::FieldType;
        match &Self::resolved_type(schema, field_type) {
            FieldType::Timestamp(_) => Some(quote! { String }),
            FieldType::StringN { .. } if is_key => Some(quote! { String }),
            FieldType::Nullable(inner) => {
                let inner = Self::schema_value_type(schema, inner, is_key)?;
                Some(quote! { Option<#inner> })
            }
            FieldType::FixedArray(inner, _) => {
                let inner = Self::schema_value_type(schema, inner, is_key)?;
                Some(quote! { Vec<#inner> })
            }
            _ => None,
        }
    }

    fn model_struct_field(
        schema: &Schema,
        field: &forgedb_parser::Field,
        auto_default: bool,
        is_key: bool,
    ) -> TokenStream {
        let field_name = format_ident!("{}", field.name);
        let field_type = if is_key {
            Self::key_type_ident(schema, &field.field_type)
        } else {
            Self::map_field_type_ident(schema, &field.field_type)
        };

        let (schema_attr, serde_attr) = Self::field_wire_attrs(schema, field, is_key);

        let serde_default = if auto_default && field.auto_generate {
            match &field.field_type {
                forgedb_parser::FieldType::Uuid
                | forgedb_parser::FieldType::U32
                | forgedb_parser::FieldType::U64 => quote! { #[serde(default)] },
                forgedb_parser::FieldType::Timestamp(_) => {
                    quote! { #[serde(default = "__forgedb_default_ts")] }
                }
                _ => quote! {},
            }
        } else {
            quote! {}
        };

        if field.is_nullable() {
            quote! { #schema_attr #serde_attr #serde_default pub #field_name: Option<#field_type> }
        } else {
            quote! { #schema_attr #serde_attr #serde_default pub #field_name: #field_type }
        }
    }

    fn field_wire_attrs(
        schema: &Schema,
        field: &forgedb_parser::Field,
        is_key: bool,
    ) -> (TokenStream, TokenStream) {
        if let Some(vt) = Self::schema_value_type(schema, &field.field_type, is_key) {
            (Self::schema_attr(quote! { #[schema(value_type = #vt)] }), quote! {})
        } else if Self::is_decimal_type(&field.field_type) {
            if field.is_nullable() {
                (
                    Self::schema_attr(quote! { #[schema(value_type = Option<String>)] }),
                    quote! { #[serde(with = "rust_decimal::serde::str_option")] },
                )
            } else {
                (
                    Self::schema_attr(quote! { #[schema(value_type = String)] }),
                    quote! { #[serde(with = "rust_decimal::serde::str")] },
                )
            }
        } else if let Some(attrs) = Self::big_array_attrs(&field.field_type) {
            attrs
        } else {
            (quote! {}, quote! {})
        }
    }

    fn generate_auto_synthesis(model: &forgedb_parser::Model, recv: &TokenStream) -> TokenStream {
        let stmts: Vec<TokenStream> = model
            .fields
            .iter()
            .filter(|f| f.auto_generate)
            .filter_map(|f| {
                let name = format_ident!("{}", f.name);
                match &f.field_type {
                    forgedb_parser::FieldType::Uuid => Some(quote! {
                        if record.#name.is_nil() {
                            record.#name = Uuid::new_v4();
                        }
                    }),
                    forgedb_parser::FieldType::Timestamp(precision) => {
                        Some(Self::timestamp_auto_synthesis(model, f, *precision, recv))
                    }
                    forgedb_parser::FieldType::U32 | forgedb_parser::FieldType::U64 => {
                        let alloc = Self::autoseq_alloc_ident(f);
                        let seq = Self::autoseq_field_ident(f);
                        Some(quote! {
                            if record.#name == 0 {
                                record.#name = #recv.#alloc()?;
                            } else {
                                #recv.#seq.fetch_max(
                                    record.#name as u64,
                                    std::sync::atomic::Ordering::SeqCst,
                                );
                            }
                        })
                    }
                    _ => None,
                }
            })
            .collect();
        quote! { #(#stmts)* }
    }

    fn timestamp_auto_synthesis(
        model: &forgedb_parser::Model,
        field: &forgedb_parser::Field,
        precision: forgedb_parser::TimestampPrecision,
        recv: &TokenStream,
    ) -> TokenStream {
        let name = format_ident!("{}", field.name);
        if Self::timestamp_key_field(model).map(|f| f.name.as_str()) == Some(field.name.as_str()) {
            let alloc = Self::autoseq_alloc_ident(field);
            let seq = Self::autoseq_field_ident(field);
            return quote! {
                if record.#name.as_micros() == 0 {
                    record.#name = #recv.#alloc()?;
                } else {
                    #recv.#seq.fetch_max(
                        record.#name.as_micros().max(0) as u64,
                        std::sync::atomic::Ordering::SeqCst,
                    );
                }
            };
        }
        let quantum = proc_macro2::Literal::i64_unsuffixed(precision.quantum_micros());
        quote! {
            if record.#name.as_micros() == 0 {
                record.#name = Timestamp::now().floor_to_micros(#quantum);
            }
        }
    }

    fn generate_timestamp_write_gate(schema: &Schema, model: &forgedb_parser::Model) -> TokenStream {
        let mtag = model.name.as_str();
        let mut stmts: Vec<TokenStream> = Vec::new();
        for field in &model.fields {
            let fident = format_ident!("{}", field.name);
            Self::timestamp_gate_walk(
                schema,
                &field.field_type,
                quote! { record.#fident },
                mtag,
                &field.name,
                &mut stmts,
                0,
            );
        }
        if stmts.is_empty() {
            return quote! {};
        }
        quote! {
            #[allow(unused_mut)]
            let mut record = record;
            #(#stmts)*
        }
    }

    fn timestamp_gate_walk(
        schema: &Schema,
        ty: &forgedb_parser::FieldType,
        place: TokenStream,
        mtag: &str,
        path: &str,
        out: &mut Vec<TokenStream>,
        depth: usize,
    ) {
        use forgedb_parser::FieldType;
        if depth > 8 {
            return;
        }
        match ty {
            FieldType::Timestamp(precision) => {
                let path_str = path.to_string();
                if precision.quantum_micros() > 1 {
                    let quantum =
                        proc_macro2::Literal::i64_unsuffixed(precision.quantum_micros());
                    out.push(quote! { #place = #place.floor_to_micros(#quantum); });
                }
                let msg = format!(
                    "must be an instant RFC 3339 can name (years 0000 through 9999); \
                     the value is storable but would fail to serialize on every read"
                );
                out.push(quote! {
                    if !#place.is_rfc3339_representable() {
                        Err(ValidationError::Constraint {
                            model: #mtag,
                            field: #path_str,
                            rule: "timestamp_range",
                            message: format!(
                                "{} — got {} microseconds since the epoch",
                                #msg,
                                #place.as_micros(),
                            ),
                        })?;
                    }
                });
            }
            FieldType::Nullable(inner) => {
                let mut nested = Vec::new();
                Self::timestamp_gate_walk(
                    schema,
                    inner,
                    quote! { (*__ts_opt) },
                    mtag,
                    path,
                    &mut nested,
                    depth + 1,
                );
                if !nested.is_empty() {
                    out.push(quote! {
                        if let Some(__ts_opt) = &mut #place { #(#nested)* }
                    });
                }
            }
            FieldType::FixedArray(inner, _) => {
                let mut nested = Vec::new();
                Self::timestamp_gate_walk(
                    schema,
                    inner,
                    quote! { (*__ts_elem) },
                    mtag,
                    path,
                    &mut nested,
                    depth + 1,
                );
                if !nested.is_empty() {
                    out.push(quote! {
                        for __ts_elem in #place.iter_mut() { #(#nested)* }
                    });
                }
            }
            FieldType::StructType(name) => {
                let Some(def) = schema.find_struct(name) else {
                    return;
                };
                for f in &def.fields {
                    let fident = format_ident!("{}", f.name);
                    Self::timestamp_gate_walk(
                        schema,
                        &f.field_type,
                        quote! { #place.#fident },
                        mtag,
                        &format!("{path}.{}", f.name),
                        out,
                        depth + 1,
                    );
                }
            }
            FieldType::OptionalStructType(name) => {
                let Some(def) = schema.find_struct(name) else {
                    return;
                };
                let mut nested = Vec::new();
                for f in &def.fields {
                    let fident = format_ident!("{}", f.name);
                    Self::timestamp_gate_walk(
                        schema,
                        &f.field_type,
                        quote! { __ts_struct.#fident },
                        mtag,
                        &format!("{path}.{}", f.name),
                        &mut nested,
                        depth + 1,
                    );
                }
                if !nested.is_empty() {
                    out.push(quote! {
                        if let Some(__ts_struct) = &mut #place { #(#nested)* }
                    });
                }
            }
            _ => {}
        }
    }

    pub(crate) fn is_server_synthesized(field: &forgedb_parser::Field) -> bool {
        field.auto_generate
            && matches!(
                field.field_type,
                forgedb_parser::FieldType::Uuid
                    | forgedb_parser::FieldType::Timestamp(_)
                    | forgedb_parser::FieldType::U32
                    | forgedb_parser::FieldType::U64
            )
    }

    pub(crate) fn creatable_fields(model: &forgedb_parser::Model) -> Vec<&forgedb_parser::Field> {
        model
            .fields
            .iter()
            .filter(|f| !Self::is_server_synthesized(f))
            .collect()
    }

    pub(crate) fn projection_pascal(name: &str) -> String {
        name.split('_')
            .filter(|s| !s.is_empty())
            .map(|s| {
                let mut c = s.chars();
                match c.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            })
            .collect()
    }

    pub(crate) fn projected_field_set<'a>(
        model: &'a forgedb_parser::Model,
        proj: &forgedb_parser::Projection,
    ) -> Vec<&'a forgedb_parser::Field> {
        let mut out: Vec<&forgedb_parser::Field> = Vec::new();
        if let Some(id_field) = model.identity_field() {
            out.push(id_field);
        }
        for fname in &proj.fields {
            if out.iter().any(|f| &f.name == fname) {
                continue;
            }
            if let Some(f) = model.fields.iter().find(|f| &f.name == fname) {
                out.push(f);
            }
        }
        out
    }

    fn generate_list_scan(
        schema: &Schema,
        model: &forgedb_parser::Model,
    ) -> (TokenStream, TokenStream) {
        if model.identity_field().is_none() {
            return (quote! {}, quote! {});
        }
        let scan_fields = Self::scan_field_set(model);
        let pushdown: Vec<_> = Self::scan_pushdown_fields(model)
            .into_iter()
            .map(|f| Self::generate_rows_by_index(schema, f))
            .collect();

        let scan_ref_ident = format_ident!("{}ScanRef", model.name);
        let key_name = Self::identity_field_name(model);
        let ref_field_decls = scan_fields.iter().map(|f| {
            let fname = format_ident!("{}", f.name);
            let fty = Self::scan_ref_field_type(schema, f, Some(f.name.as_str()) == key_name);
            quote! { pub #fname: #fty }
        });
        let ref_borrow_anchor = match Self::scan_ref_anchor(&scan_fields, key_name) {
            None => quote! {},
            Some(anchor) => quote! {
                pub #anchor: ::std::marker::PhantomData<&'a ()>,
            },
        };
        let slot_field = Self::scan_slot_field(&scan_fields);
        let ref_struct_tokens = quote! {
            #[derive(Debug, Clone)]
            pub struct #scan_ref_ident<'a> {
                pub #slot_field: usize,
                #(#ref_field_decls,)*
                #ref_borrow_anchor
            }
        };

        let buf_holder = format_ident!("__{}ScanBufs", model.name);
        let with_scan_method = Self::generate_scan_scope_method(
            schema,
            &scan_ref_ident,
            &buf_holder,
            &scan_fields,
            key_name,
            &slot_field,
        );

        let page_ref_tokens = Self::generate_page_ref_struct(schema, model, &scan_fields, key_name);
        let with_page_method = Self::generate_page_scope_method(
            schema,
            model,
            &scan_ref_ident,
            &format_ident!("__{}PageScanBufs", model.name),
            &scan_fields,
            key_name,
            &slot_field,
        );

        let with_fast_page_method =
            Self::generate_fast_page_scope_method(schema, model, &scan_fields, key_name);

        let methods = quote! {
            #with_scan_method
            #with_page_method
            #with_fast_page_method
            #(#pushdown)*
        };
        let structs = quote! {
            #ref_struct_tokens
            #page_ref_tokens
        };
        (structs, methods)
    }

    fn scan_slot_field(fields: &[&forgedb_parser::Field]) -> proc_macro2::Ident {
        let mut name = String::from("__slot");
        while fields.iter().any(|f| f.name == name) {
            name.push('_');
        }
        format_ident!("{}", name)
    }

    fn slot_field_init(slot_field: &proc_macro2::Ident) -> TokenStream {
        if slot_field == "__slot" {
            quote! { #slot_field, }
        } else {
            quote! { #slot_field: __slot, }
        }
    }

    fn scan_ref_anchor(
        fields: &[&forgedb_parser::Field],
        key_name: Option<&str>,
    ) -> Option<proc_macro2::Ident> {
        if fields
            .iter()
            .any(|f| Self::is_string_semantic(&f.field_type) && Some(f.name.as_str()) != key_name)
        {
            return None;
        }
        let mut name = String::from("__borrow");
        while fields.iter().any(|f| f.name == name) {
            name.push('_');
        }
        Some(format_ident!("{}", name))
    }

    fn scan_ref_field_type(
        schema: &Schema,
        field: &forgedb_parser::Field,
        is_key: bool,
    ) -> TokenStream {
        if is_key {
            return Self::key_type_ident(schema, &field.field_type);
        }
        if Self::is_string_semantic(&field.field_type) {
            return if field.is_nullable() {
                quote! { Option<&'a str> }
            } else {
                quote! { &'a str }
            };
        }
        let base = Self::map_field_type_ident(schema, &field.field_type);
        if field.is_nullable() {
            quote! { Option<#base> }
        } else {
            base
        }
    }

    fn page_field_from_scan(
        scan_fields: &[&forgedb_parser::Field],
        field: &forgedb_parser::Field,
    ) -> bool {
        scan_fields.iter().any(|s| s.name == field.name)
    }

    fn page_ref_field_type(
        schema: &Schema,
        model: &forgedb_parser::Model,
        field: &forgedb_parser::Field,
        from_scan: bool,
        key_name: Option<&str>,
    ) -> TokenStream {
        if from_scan {
            return Self::scan_ref_field_type(schema, field, Some(field.name.as_str()) == key_name);
        }
        let base = if Self::is_inline_key_field(schema, model, field) {
            Self::key_type_ident(schema, &field.field_type)
        } else {
            Self::map_field_type_ident(schema, &field.field_type)
        };
        if field.is_nullable() {
            quote! { Option<#base> }
        } else {
            base
        }
    }

    fn page_ref_anchor(
        model: &forgedb_parser::Model,
        scan_fields: &[&forgedb_parser::Field],
        key_name: Option<&str>,
    ) -> Option<proc_macro2::Ident> {
        Self::scan_ref_anchor(scan_fields, key_name).map(|_| {
            let mut name = String::from("__borrow");
            while model.fields.iter().any(|f| f.name == name) {
                name.push('_');
            }
            format_ident!("{}", name)
        })
    }

    fn generate_page_ref_struct(
        schema: &Schema,
        model: &forgedb_parser::Model,
        scan_fields: &[&forgedb_parser::Field],
        key_name: Option<&str>,
    ) -> TokenStream {
        let page_ref_ident = format_ident!("{}PageRef", model.name);
        let field_decls = model.fields.iter().map(|f| {
            let fname = format_ident!("{}", f.name);
            let from_scan = Self::page_field_from_scan(scan_fields, f);
            let fty = Self::page_ref_field_type(schema, model, f, from_scan, key_name);
            let (_schema_attr, serde_attr) =
                Self::field_wire_attrs(schema, f, Self::is_inline_key_field(schema, model, f));
            quote! { #serde_attr pub #fname: #fty }
        });
        let anchor = match Self::page_ref_anchor(model, scan_fields, key_name) {
            None => quote! {},
            Some(anchor) => quote! {
                #[serde(skip)]
                pub #anchor: ::std::marker::PhantomData<&'a ()>,
            },
        };
        quote! {
            #[derive(serde::Serialize)]
            pub struct #page_ref_ident<'a> {
                #(#field_decls,)*
                #anchor
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn buffered_gather_pieces(
        schema: &Schema,
        fields: &[&forgedb_parser::Field],
        recv: &TokenStream,
        rows: &TokenStream,
        slot: &TokenStream,
        borrowed: bool,
        key_name: Option<&str>,
        gather_expect: &str,
    ) -> BufferedGather {
        let mut out = BufferedGather {
            decls: Vec::new(),
            inits: Vec::new(),
            reads: Vec::new(),
            values: Vec::new(),
        };
        for field in fields {
            let fname = format_ident!("{}", field.name);
            let col_ident = format_ident!("{}_col", field.name);
            let buffered_ty = if Self::is_variable_string_type(&field.field_type)
                || Self::is_json_type(&field.field_type)
            {
                Some(quote! { forgedb_storage::BufferedVariableColumn })
            } else if Self::is_enum_type(&field.field_type)
                || Self::is_fixed_size_type(schema, &field.field_type)
            {
                Some(quote! { forgedb_storage::BufferedFixedColumn })
            } else {
                None
            };
            let as_key = Some(field.name.as_str()) == key_name;
            match (
                buffered_ty,
                Self::field_read_stmt(schema, field, recv, slot, borrowed, as_key),
            ) {
                (Some(ty), Some((value_ident, stmt))) => {
                    out.decls.push(quote! { #col_ident: #ty });
                    out.inits.push(quote! {
                        #col_ident: self.#col_ident.gather_buffered(&#rows)
                            .expect(#gather_expect)
                    });
                    out.reads.push(stmt);
                    out.values.push(quote! { #fname: #value_ident });
                }
                _ => {
                    let default_val = Self::default_for_unstored_field(&field.field_type);
                    out.values.push(quote! { #fname: #default_val });
                }
            }
        }
        out
    }

    fn scan_row_selection() -> TokenStream {
        let live = Self::live_row_selection_expr();
        quote! {
            let __rows: Vec<usize> = match sel {
                Some(mut __c) => {
                    __c.sort_unstable();
                    __c
                }
                None => #live
            };
        }
    }

    fn live_row_selection_expr() -> TokenStream {
        quote! {
            {
                let mut __all: Vec<usize> = self.id_to_row.values().copied().collect();
                __all.sort_unstable();
                self.tombstones.live_indices(&__all)
                    .expect("Failed to read tombstone liveness")
            }
        }
    }

    fn generate_page_scope_method(
        schema: &Schema,
        model: &forgedb_parser::Model,
        ref_ident: &proc_macro2::Ident,
        scan_holder: &proc_macro2::Ident,
        scan_fields: &[&forgedb_parser::Field],
        key_name: Option<&str>,
        slot_field: &proc_macro2::Ident,
    ) -> TokenStream {
        let page_ref_ident = format_ident!("{}PageRef", model.name);
        let page_holder = format_ident!("__{}PageBufs", model.name);

        let scan = Self::buffered_gather_pieces(
            schema,
            scan_fields,
            &quote! { __bufs },
            &quote! { __rows },
            &quote! { __slot },
            true,
            key_name,
            "Failed to bulk-load scan column",
        );
        let scan_decls = &scan.decls;
        let scan_inits = &scan.inits;
        let scan_reads = &scan.reads;
        let scan_values = &scan.values;
        let ref_borrow_init = match Self::scan_ref_anchor(scan_fields, key_name) {
            None => quote! {},
            Some(anchor) => quote! { #anchor: ::std::marker::PhantomData, },
        };
        let slot_init = Self::slot_field_init(slot_field);

        let page_only: Vec<&forgedb_parser::Field> = model
            .fields
            .iter()
            .filter(|f| !Self::page_field_from_scan(scan_fields, f))
            .collect();
        let page = Self::buffered_gather_pieces(
            schema,
            &page_only,
            &quote! { __page_bufs },
            &quote! { __page_rows },
            &quote! { __pslot },
            false,
            key_name,
            "Failed to bulk-load page column",
        );
        let page_decls = &page.decls;
        let page_inits = &page.inits;
        let page_reads = &page.reads;

        let page_value_by_name: std::collections::HashMap<&str, &TokenStream> = page_only
            .iter()
            .map(|f| f.name.as_str())
            .zip(page.values.iter())
            .collect();
        let page_view_values: Vec<TokenStream> = model
            .fields
            .iter()
            .map(|f| {
                if Self::page_field_from_scan(scan_fields, f) {
                    let fname = format_ident!("{}", f.name);
                    quote! { #fname: __ref.#fname }
                } else {
                    let v = page_value_by_name
                        .get(f.name.as_str())
                        .expect("every non-scan field is in the page gather");
                    quote! { #v }
                }
            })
            .collect();
        let page_borrow_init = match Self::page_ref_anchor(model, scan_fields, key_name) {
            None => quote! {},
            Some(anchor) => quote! { #anchor: ::std::marker::PhantomData, },
        };

        let row_selection = Self::scan_row_selection();

        quote! {
            pub fn __with_page<R>(
                &self,
                sel: Option<Vec<usize>>,
                keep: impl Fn(&#ref_ident<'_>) -> bool,
                sort: impl FnOnce(&mut Vec<#ref_ident<'_>>),
                offset: usize,
                limit: usize,
                f: impl FnOnce(usize, &[#page_ref_ident<'_>]) -> R,
            ) -> R {
                #row_selection
                let __n = __rows.len();

                #[allow(non_camel_case_types)]
                struct #scan_holder {
                    #(#scan_decls,)*
                }
                let __bufs = #scan_holder {
                    #(#scan_inits,)*
                };

                let mut __refs: Vec<#ref_ident<'_>> = Vec::with_capacity(__n);
                for __slot in 0..__n {
                    #(#scan_reads)*
                    let __row_ref = #ref_ident {
                        #slot_init
                        #(#scan_values,)*
                        #ref_borrow_init
                    };
                    if keep(&__row_ref) {
                        __refs.push(__row_ref);
                    }
                }

                let __total = __refs.len();
                sort(&mut __refs);

                let __start = offset.min(__total);
                let __end = offset.saturating_add(limit).min(__total);
                let __page = &__refs[__start..__end];

                let __page_rows: Vec<usize> = __page
                    .iter()
                    .map(|__r| __rows[__r.#slot_field])
                    .collect();

                #[allow(non_camel_case_types)]
                struct #page_holder {
                    #(#page_decls,)*
                }
                let __page_bufs = #page_holder {
                    #(#page_inits,)*
                };

                let mut __views: Vec<#page_ref_ident<'_>> = Vec::with_capacity(__page.len());
                for (__pslot, __ref) in __page.iter().enumerate() {
                    #(#page_reads)*
                    __views.push(#page_ref_ident {
                        #(#page_view_values,)*
                        #page_borrow_init
                    });
                }
                f(__total, &__views)
            }
        }
    }

    fn generate_fast_page_scope_method(
        schema: &Schema,
        model: &forgedb_parser::Model,
        scan_fields: &[&forgedb_parser::Field],
        key_name: Option<&str>,
    ) -> TokenStream {
        let page_ref_ident = format_ident!("{}PageRef", model.name);
        let holder = format_ident!("__{}FastPageBufs", model.name);

        let all_fields: Vec<&forgedb_parser::Field> = model.fields.iter().collect();
        let gather = Self::buffered_gather_pieces(
            schema,
            &all_fields,
            &quote! { __bufs },
            &quote! { __rows[__start..__end] },
            &quote! { __pslot },
            true,
            key_name,
            "Failed to bulk-load fast-page column",
        );
        let decls = &gather.decls;
        let inits = &gather.inits;
        let reads = &gather.reads;
        let values = &gather.values;

        let page_borrow_init = match Self::page_ref_anchor(model, scan_fields, key_name) {
            None => quote! {},
            Some(anchor) => quote! { #anchor: ::std::marker::PhantomData, },
        };
        let live = Self::live_row_selection_expr();

        quote! {
            pub fn __with_fast_page<R>(
                &self,
                offset: usize,
                limit: usize,
                f: impl FnOnce(usize, &[#page_ref_ident<'_>]) -> R,
            ) -> R {
                let __rows: Vec<usize> = #live;
                let __total = __rows.len();
                let __start = offset.min(__total);
                let __end = offset.saturating_add(limit).min(__total);

                #[allow(non_camel_case_types)]
                struct #holder {
                    #(#decls,)*
                }
                let __bufs = #holder {
                    #(#inits,)*
                };

                let mut __views: Vec<#page_ref_ident<'_>> = Vec::with_capacity(__end - __start);
                for __pslot in 0..(__end - __start) {
                    #(#reads)*
                    __views.push(#page_ref_ident {
                        #(#values,)*
                        #page_borrow_init
                    });
                }
                f(__total, &__views)
            }
        }
    }

    fn generate_scan_scope_method(
        schema: &Schema,
        ref_ident: &proc_macro2::Ident,
        holder: &proc_macro2::Ident,
        fields: &[&forgedb_parser::Field],
        key_name: Option<&str>,
        slot_field: &proc_macro2::Ident,
    ) -> TokenStream {
        let gathered = Self::buffered_gather_pieces(
            schema,
            fields,
            &quote! { __bufs },
            &quote! { __rows },
            &quote! { __slot },
            true,
            key_name,
            "Failed to bulk-load scan column",
        );
        let buf_field_decls = &gathered.decls;
        let buf_inits = &gathered.inits;
        let buf_read_stmts = &gathered.reads;
        let buf_field_values = &gathered.values;

        let ref_borrow_init = match Self::scan_ref_anchor(fields, key_name) {
            None => quote! {},
            Some(anchor) => quote! { #anchor: ::std::marker::PhantomData, },
        };
        let slot_init = Self::slot_field_init(slot_field);
        let row_selection = Self::scan_row_selection();

        quote! {
            pub fn __with_scan<R>(
                &self,
                sel: Option<Vec<usize>>,
                keep: impl Fn(&#ref_ident<'_>) -> bool,
                f: impl FnOnce(&mut Vec<#ref_ident<'_>>) -> R,
            ) -> R {
                #row_selection
                let __n = __rows.len();

                #[allow(non_camel_case_types)]
                struct #holder {
                    #(#buf_field_decls,)*
                }
                let __bufs = #holder {
                    #(#buf_inits,)*
                };

                let mut __refs: Vec<#ref_ident<'_>> = Vec::with_capacity(__n);
                for __slot in 0..__n {
                    #(#buf_read_stmts)*
                    let __row_ref = #ref_ident {
                        #slot_init
                        #(#buf_field_values,)*
                        #ref_borrow_init
                    };
                    if keep(&__row_ref) {
                        __refs.push(__row_ref);
                    }
                }
                f(&mut __refs)
            }
        }
    }

    fn generate_buffered_scan_method(
        schema: &Schema,
        struct_ident: &proc_macro2::Ident,
        method: &proc_macro2::Ident,
        holder: &proc_macro2::Ident,
        fields: &[&forgedb_parser::Field],
        key_name: Option<&str>,
    ) -> TokenStream {
        let gathered = Self::buffered_gather_pieces(
            schema,
            fields,
            &quote! { __bufs },
            &quote! { __rows },
            &quote! { __slot },
            false,
            key_name,
            "Failed to bulk-load scan column",
        );
        let buf_field_decls = &gathered.decls;
        let buf_inits = &gathered.inits;
        let buf_read_stmts = &gathered.reads;
        let buf_field_values = &gathered.values;

        quote! {
            pub fn #method(&self) -> Vec<#struct_ident> {
                let mut __rows: Vec<usize> = self.id_to_row.values().copied().collect();
                __rows.sort_unstable();
                let __rows = self.tombstones.live_indices(&__rows)
                    .expect("Failed to read tombstone liveness");
                let __n = __rows.len();

                #[allow(non_camel_case_types)]
                struct #holder {
                    #(#buf_field_decls,)*
                }
                let __bufs = #holder {
                    #(#buf_inits,)*
                };

                let mut rows = Vec::with_capacity(__n);
                for __slot in 0..__n {
                    #(#buf_read_stmts)*
                    rows.push(#struct_ident {
                        #(#buf_field_values),*
                    });
                }
                rows
            }
        }
    }

    pub(crate) fn scan_pushdown_fields(model: &forgedb_parser::Model) -> Vec<&forgedb_parser::Field> {
        use forgedb_parser::{FieldType, RelationType};
        Self::indexed_fields(model)
            .into_iter()
            .filter(|f| crate::api::ApiGenerator::is_filterable_field(&f.field_type))
            .filter(|f| {
                matches!(
                    &f.field_type,
                    FieldType::String
                        | FieldType::Uuid
                        | FieldType::U32
                        | FieldType::U64
                        | FieldType::I32
                        | FieldType::I64
                        | FieldType::Bool
                        | FieldType::Decimal
                        | FieldType::Timestamp(_)
                        | FieldType::Enum(_)
                        | FieldType::Relation(RelationType::RequiredReference(_))
                )
            })
            .collect()
    }

    fn generate_rows_by_index(schema: &Schema, field: &forgedb_parser::Field) -> TokenStream {
        use forgedb_parser::{FieldType, RelationType};
        let index_ident = Self::index_field_ident(field);
        let rows_by = format_ident!("__rows_by_{}", field.name);
        let (parse_stmt, key_value): (TokenStream, TokenStream) = match &field.field_type {
            FieldType::String => (quote! {}, quote! { value }),
            FieldType::Uuid | FieldType::Relation(RelationType::RequiredReference(_)) => (
                quote! { let __typed = value.parse::<Uuid>().ok()?; },
                quote! { __typed },
            ),
            FieldType::U32 => (quote! { let __typed = value.parse::<u32>().ok()?; }, quote! { __typed }),
            FieldType::U64 => (quote! { let __typed = value.parse::<u64>().ok()?; }, quote! { __typed }),
            FieldType::I32 => (quote! { let __typed = value.parse::<i32>().ok()?; }, quote! { __typed }),
            FieldType::I64 => (quote! { let __typed = value.parse::<i64>().ok()?; }, quote! { __typed }),
            FieldType::Bool => (quote! { let __typed = value.parse::<bool>().ok()?; }, quote! { __typed }),
            FieldType::Decimal => (
                quote! { let __typed = value.parse::<rust_decimal::Decimal>().ok()?; },
                quote! { __typed },
            ),
            FieldType::Timestamp(_) => (
                quote! { let __typed = value.parse::<Timestamp>().ok()?; },
                quote! { __typed },
            ),
            FieldType::Enum(name) => {
                let en = format_ident!("{}", name);
                (
                    quote! {
                        let __typed = serde_json::from_value::<#en>(
                            serde_json::Value::String(value.to_string())
                        ).ok()?;
                    },
                    quote! { __typed },
                )
            }
            _ => return quote! {},
        };
        let key = Self::index_key_expr(schema, &field.field_type, Self::index_value_expr(&field.field_type, key_value));
        quote! {
            pub fn #rows_by(&self, value: &str) -> Option<Vec<usize>> {
                #parse_stmt
                let __k: String = { #key };
                let __ids = match self.#index_ident.get(&__k) {
                    Some(__s) => __s,
                    None => return Some(Vec::new()),
                };
                let mut __out = Vec::with_capacity(__ids.len());
                for &__id in __ids {
                    if let Some(&__row) = self.id_to_row.get(&__id) {
                        __out.push(__row);
                    }
                }
                Some(__out)
            }
        }
    }

    pub(crate) fn scan_field_set(model: &forgedb_parser::Model) -> Vec<&forgedb_parser::Field> {
        let mut out: Vec<&forgedb_parser::Field> = Vec::new();
        if let Some(id_field) = model.identity_field() {
            out.push(id_field);
        }
        for f in &model.fields {
            if out.iter().any(|o| o.name == f.name) {
                continue;
            }
            if crate::api::ApiGenerator::is_filterable_field(&f.field_type) {
                out.push(f);
            }
        }
        out
    }

    fn generate_projections(
        schema: &Schema,
        model: &forgedb_parser::Model,
        id_type: &TokenStream,
        id_read: Option<&TokenStream>,
    ) -> (TokenStream, TokenStream) {
        if model.projections.is_empty() {
            return (quote! {}, quote! {});
        }

        let resolvers = match id_read {
            Some(_id_read) => quote! {
                fn __proj_resolve_at(&self, snap: &forgedb_storage::Snapshot, id: #id_type) -> Option<usize> {
                    let watermark = snap.watermark();
                    let versions = self.id_versions.get(&id)?;
                    let pos = versions.partition_point(|&r| r < watermark);
                    if pos == 0 {
                        return None;
                    }
                    Some(versions[pos - 1])
                }

                fn __proj_live_rows_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<usize> {
                    let watermark = snap.watermark();
                    let mut rows = Vec::new();
                    for versions in self.id_versions.values() {
                        let pos = versions.partition_point(|&r| r < watermark);
                        if pos == 0 {
                            continue;
                        }
                        rows.push(versions[pos - 1]);
                    }
                    rows.sort_unstable();
                    rows
                }
            },
            None => quote! {
                fn __proj_resolve_at(&self, snap: &forgedb_storage::Snapshot, id: #id_type) -> Option<usize> {
                    let row_index = *self.id_to_row.get(&id)?;
                    if !snap.visible(row_index) { return None; }
                    Some(row_index)
                }

                fn __proj_live_rows_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<usize> {
                    (0..snap.watermark()).collect()
                }
            },
        };

        let mut structs = Vec::new();
        let mut methods = Vec::new();

        for proj in &model.projections {
            let proj_ident = format_ident!("{}{}", model.name, Self::projection_pascal(&proj.name));
            let fields = Self::projected_field_set(model, proj);
            let struct_field_defs: Vec<_> =
                fields
                    .iter()
                    .map(|f| Self::model_struct_field(schema, f, false, Self::is_inline_key_field(schema, model, f)))
                    .collect();

            let read_at_name = format_ident!("read_{}_at", proj.name);
            let get_name = format_ident!("get_{}", proj.name);
            let all_name = format_ident!("all_{}", proj.name);
            let get_at_name = format_ident!("get_{}_at", proj.name);
            let all_at_name = format_ident!("all_{}_at", proj.name);

            let read_body =
                Self::generate_row_read_body(schema, &proj_ident, &fields, Self::identity_field_name(model));

            let proj_holder =
                format_ident!("__{}{}ScanBufs", model.name, Self::projection_pascal(&proj.name));
            let all_method =
                Self::generate_buffered_scan_method(
                    schema,
                    &proj_ident,
                    &all_name,
                    &proj_holder,
                    &fields,
                    Self::identity_field_name(model),
                );

            let to_schema_derive = Self::to_schema_derive();
            structs.push(quote! {
                #[derive(Debug, Clone, Serialize, Deserialize #to_schema_derive)]
                pub struct #proj_ident {
                    #(#struct_field_defs),*
                }
            });

            methods.push(quote! {
                pub fn #read_at_name(&self, row_index: usize) -> Option<#proj_ident> {
                    #read_body
                }

                pub fn #get_name(&self, id: #id_type) -> Option<#proj_ident> {
                    let row_index = *self.id_to_row.get(&id)?;
                    self.#read_at_name(row_index)
                }

                #all_method

                pub fn #get_at_name(&self, snap: &forgedb_storage::Snapshot, id: #id_type) -> Option<#proj_ident> {
                    let row_index = self.__proj_resolve_at(snap, id)?;
                    self.#read_at_name(row_index)
                }

                pub fn #all_at_name(&self, snap: &forgedb_storage::Snapshot) -> Vec<#proj_ident> {
                    let mut records = Vec::new();
                    for row_index in self.__proj_live_rows_at(snap) {
                        if let Some(record) = self.#read_at_name(row_index) {
                            records.push(record);
                        }
                    }
                    records
                }
            });
        }

        let structs = quote! { #(#structs)* };
        let methods = quote! {
            #resolvers
            #(#methods)*
        };
        (structs, methods)
    }

    fn generate_row_read_body(
        schema: &Schema,
        struct_ident: &proc_macro2::Ident,
        fields: &[&forgedb_parser::Field],
        key_name: Option<&str>,
    ) -> TokenStream {
        let mut read_statements = Vec::new();
        let mut field_values = Vec::new();
        let recv = quote! { self };
        let row = quote! { row_index };

        for field in fields {
            let field_name = format_ident!("{}", field.name);
            match Self::field_read_stmt(schema, field, &recv, &row, false, Some(field.name.as_str()) == key_name) {
                Some((value_ident, stmt)) => {
                    read_statements.push(stmt);
                    field_values.push(quote! { #field_name: #value_ident });
                }
                None => {
                    let default_val = Self::default_for_unstored_field(&field.field_type);
                    field_values.push(quote! { #field_name: #default_val });
                }
            }
        }

        quote! {
            if self.tombstones.is_deleted(row_index).unwrap_or(true) {
                return None;
            }

            #(#read_statements)*

            Some(#struct_ident {
                #(#field_values),*
            })
        }
    }

    fn generate_read_at_logic(schema: &Schema, model: &forgedb_parser::Model) -> TokenStream {
        let model_name = format_ident!("{}", model.name);
        let fields: Vec<&forgedb_parser::Field> = model.fields.iter().collect();
        Self::generate_row_read_body(schema, &model_name, &fields, Self::identity_field_name(model))
    }

    fn stored_type_tokens(
        schema: &Schema,
        field_type: &forgedb_parser::FieldType,
        is_nullable: bool,
    ) -> TokenStream {
        let base = Self::map_field_type_ident(schema, field_type);
        if is_nullable {
            quote! { Option<#base> }
        } else {
            base
        }
    }

    fn default_for_unstored_field(field_type: &forgedb_parser::FieldType) -> TokenStream {
        match field_type {
            forgedb_parser::FieldType::Relation(_) => quote! { () },
            forgedb_parser::FieldType::Component(_) => quote! { Default::default() },
            _ => quote! { Default::default() },
        }
    }

    fn is_fixed_size_type(schema: &Schema, field_type: &forgedb_parser::FieldType) -> bool {
        let field_type = &Self::resolved_type(schema, field_type);
        match field_type {
            forgedb_parser::FieldType::U32
            | forgedb_parser::FieldType::U64
            | forgedb_parser::FieldType::I32
            | forgedb_parser::FieldType::I64
            | forgedb_parser::FieldType::F64
            | forgedb_parser::FieldType::Bool
            | forgedb_parser::FieldType::Uuid
            | forgedb_parser::FieldType::Timestamp(_)
            | forgedb_parser::FieldType::Decimal
            | forgedb_parser::FieldType::Enum(_)
            | forgedb_parser::FieldType::StringN { .. }
            | forgedb_parser::FieldType::Bytes(_)
            | forgedb_parser::FieldType::FixedArray(_, _)
            | forgedb_parser::FieldType::StructType(_)
            | forgedb_parser::FieldType::OptionalStructType(_) => true,
            forgedb_parser::FieldType::Nullable(inner) => Self::is_fixed_size_type(schema, inner),
            _ => false,
        }
    }

    const SERDE_ARRAY_CEILING: usize = 32;

    fn big_array_serde_path(field_type: &forgedb_parser::FieldType) -> Option<&'static str> {
        use forgedb_parser::FieldType;
        match field_type {
            FieldType::Bytes(n) if *n > Self::SERDE_ARRAY_CEILING => Some("__forgedb_big_bytes"),
            FieldType::FixedArray(inner, _) if matches!(**inner, FieldType::Bytes(n) if n > Self::SERDE_ARRAY_CEILING) => {
                Some("__forgedb_big_bytes::array")
            }
            FieldType::FixedArray(_, m) if *m > Self::SERDE_ARRAY_CEILING => {
                Some("__forgedb_big_array")
            }
            FieldType::Nullable(inner) => match Self::big_array_serde_path(inner)? {
                "__forgedb_big_bytes" => Some("__forgedb_big_bytes::option"),
                "__forgedb_big_array" => Some("__forgedb_big_array::option"),
                _ => None,
            },
            _ => None,
        }
    }

    fn schema_needs_big_array_serde(schema: &Schema) -> bool {
        let model_fields = schema.models.iter().flat_map(|m| m.fields.iter());
        let struct_fields = schema.structs.iter().flat_map(|s| s.fields.iter());
        model_fields
            .chain(struct_fields)
            .any(|f| Self::big_array_serde_path(&f.field_type).is_some())
    }

    fn schema_needs_f64_key(schema: &Schema) -> bool {
        schema.models.iter().any(|m| {
            let singles = Self::indexed_fields(m).into_iter();
            let composites = Self::composite_indexes(m)
                .into_iter()
                .flat_map(|(_ident, comps)| comps.into_iter());
            singles
                .chain(composites)
                .any(|f| Self::is_f64_type(&f.field_type))
        })
    }

    fn is_f64_type(field_type: &forgedb_parser::FieldType) -> bool {
        use forgedb_parser::FieldType;
        match field_type {
            FieldType::F64 => true,
            FieldType::Nullable(inner) => Self::is_f64_type(inner),
            _ => false,
        }
    }

    fn big_array_attrs(
        field_type: &forgedb_parser::FieldType,
    ) -> Option<(TokenStream, TokenStream)> {
        let path = Self::big_array_serde_path(field_type)?;
        let nullable = matches!(field_type, forgedb_parser::FieldType::Nullable(_));
        let element_is_bytes = path.starts_with("__forgedb_big_bytes");
        let inner_schema = if element_is_bytes && path.ends_with("::array") {
            quote! { Vec<Vec<u8>> }
        } else if element_is_bytes {
            quote! { Vec<u8> }
        } else {
            quote! { Vec<serde_json::Value> }
        };
        let schema_attr = if nullable {
            Self::schema_attr(quote! { #[schema(value_type = Option<#inner_schema>)] })
        } else {
            Self::schema_attr(quote! { #[schema(value_type = #inner_schema)] })
        };
        Some((schema_attr, quote! { #[serde(with = #path)] }))
    }

    fn is_variable_string_type(field_type: &forgedb_parser::FieldType) -> bool {
        match field_type {
            forgedb_parser::FieldType::String => true,
            forgedb_parser::FieldType::Nullable(inner) => Self::is_variable_string_type(inner),
            _ => false,
        }
    }

    fn inline_string_layout(chars: u8, exact: bool, utf8: bool) -> (usize, usize, usize) {
        let payload = chars as usize * if utf8 { 4 } else { 1 };
        let prefix = if exact && !utf8 {
            0
        } else if payload <= u8::MAX as usize {
            1
        } else {
            2
        };
        (payload + prefix, prefix, payload)
    }

    fn inline_string_pack_body(
        chars: u8,
        exact: bool,
        utf8: bool,
        base: usize,
        value_expr: TokenStream,
    ) -> TokenStream {
        let (_, prefix, payload) = Self::inline_string_layout(chars, exact, utf8);
        let start = base + prefix;
        let write_prefix = match prefix {
            0 => quote! {},
            1 => quote! { __buf[#base] = __n as u8; },
            _ => quote! {
                __buf[#base..#base + 2].copy_from_slice(&(__n as u16).to_le_bytes());
            },
        };
        quote! {
            let __s: &str = #value_expr;
            let __b = __s.as_bytes();
            let __n = __b.len().min(#payload);
            #write_prefix
            __buf[#start..#start + __n].copy_from_slice(&__b[..__n]);
        }
    }

    fn inline_string_bytes_expr(
        chars: u8,
        exact: bool,
        utf8: bool,
        base: usize,
        raw_expr: TokenStream,
    ) -> TokenStream {
        let (_, prefix, payload) = Self::inline_string_layout(chars, exact, utf8);
        let start = base + prefix;
        if prefix == 0 {
            return quote! { &#raw_expr[#start..#start + #payload] };
        }
        quote! {
            {
                let __raw = &#raw_expr;
                let __n = __forgedb_inline_len(&__raw[#base..], #prefix).min(#payload);
                &__raw[#start..#start + __n]
            }
        }
    }

    pub(crate) fn needs_inline_str(schema: &Schema) -> bool {
        schema.models.iter().any(|m| {
            matches!(
                Self::identity_type(schema, m),
                Some(forgedb_parser::FieldType::StringN { .. })
            )
        })
    }

    pub(crate) fn inline_str_import(schema: &Schema) -> TokenStream {
        if Self::needs_inline_str(schema) {
            quote! { use forgedb_types::InlineStr; }
        } else {
            quote! {}
        }
    }

    fn needs_inline_len_helper(schema: &forgedb_parser::Schema) -> bool {
        schema.models.iter().flat_map(|m| &m.fields).any(|f| {
            Self::inline_string_column_params(schema, &f.field_type)
                .map(|(chars, exact)| {
                    Self::inline_string_layout(chars, exact, Self::is_utf8_field(f)).1 > 0
                })
                .unwrap_or(false)
        })
    }

    fn generate_identity_alphabet_helper() -> TokenStream {
        quote! {
            #[inline]
            fn __forgedb_identity_char_ok(__c: char) -> bool {
                __c.is_ascii_alphanumeric()
                    || matches!(
                        __c,
                        '-' | '.' | '_' | '~'
                        | '!' | '$' | '&' | '\'' | '(' | ')' | '*' | '+' | ',' | ';' | '='
                        | ':' | '@'
                    )
            }
        }
    }

    fn generate_inline_len_helper() -> TokenStream {
        quote! {
            #[inline]
            fn __forgedb_inline_len(__raw: &[u8], __prefix: usize) -> usize {
                if __prefix == 1 {
                    __raw[0] as usize
                } else {
                    u16::from_le_bytes([__raw[0], __raw[1]]) as usize
                }
            }
        }
    }

    fn is_utf8_field(field: &forgedb_parser::Field) -> bool {
        field.has_constraint("utf8")
    }

    fn is_inline_string_type(field_type: &forgedb_parser::FieldType) -> bool {
        match field_type {
            forgedb_parser::FieldType::StringN { .. } => true,
            forgedb_parser::FieldType::Nullable(inner) => Self::is_inline_string_type(inner),
            _ => false,
        }
    }

    fn inline_string_params(field_type: &forgedb_parser::FieldType) -> Option<(u8, bool)> {
        match field_type {
            forgedb_parser::FieldType::StringN { chars, exact } => Some((*chars, *exact)),
            forgedb_parser::FieldType::Nullable(inner) => Self::inline_string_params(inner),
            _ => None,
        }
    }

    fn inline_string_column_params(
        schema: &Schema,
        field_type: &forgedb_parser::FieldType,
    ) -> Option<(u8, bool)> {
        Self::inline_string_params(&Self::resolved_type(schema, field_type))
    }

    fn is_inline_key_field(
        schema: &Schema,
        model: &forgedb_parser::Model,
        field: &forgedb_parser::Field,
    ) -> bool {
        (Self::is_identity(model, field)
            || Self::fk_backing_type(schema, &field.field_type).is_some())
            && Self::is_inline_string_type(&Self::resolved_type(schema, &field.field_type))
    }

    fn is_string_semantic(field_type: &forgedb_parser::FieldType) -> bool {
        Self::is_variable_string_type(field_type) || Self::is_inline_string_type(field_type)
    }

    fn is_json_type(field_type: &forgedb_parser::FieldType) -> bool {
        match field_type {
            forgedb_parser::FieldType::Json => true,
            forgedb_parser::FieldType::Nullable(inner) => Self::is_json_type(inner),
            _ => false,
        }
    }

    fn is_decimal_type(field_type: &forgedb_parser::FieldType) -> bool {
        match field_type {
            forgedb_parser::FieldType::Decimal => true,
            forgedb_parser::FieldType::Nullable(inner) => Self::is_decimal_type(inner),
            _ => false,
        }
    }

    fn is_enum_type(field_type: &forgedb_parser::FieldType) -> bool {
        match field_type {
            forgedb_parser::FieldType::Enum(_) => true,
            forgedb_parser::FieldType::Nullable(inner) => Self::is_enum_type(inner),
            _ => false,
        }
    }

    fn enum_type_ident(field_type: &forgedb_parser::FieldType) -> Option<proc_macro2::Ident> {
        field_type
            .enum_name()
            .map(|n| format_ident!("{}", n))
    }

    fn is_variable_column_type(field_type: &forgedb_parser::FieldType) -> bool {
        match field_type {
            forgedb_parser::FieldType::String | forgedb_parser::FieldType::Json => true,
            forgedb_parser::FieldType::Nullable(inner) => Self::is_variable_column_type(inner),
            _ => false,
        }
    }

    fn type_name(schema: &Schema, field_type: &forgedb_parser::FieldType) -> &'static str {
        let field_type = &Self::resolved_type(schema, field_type);
        match field_type {
            forgedb_parser::FieldType::U32 => "u32",
            forgedb_parser::FieldType::U64 => "u64",
            forgedb_parser::FieldType::I32 => "i32",
            forgedb_parser::FieldType::I64 => "i64",
            forgedb_parser::FieldType::F64 => "f64",
            forgedb_parser::FieldType::Bool => "bool",
            forgedb_parser::FieldType::Uuid => "uuid",
            forgedb_parser::FieldType::Timestamp(_) => "timestamp",
            forgedb_parser::FieldType::Decimal => "decimal",
            forgedb_parser::FieldType::Enum(_) => "enum",
            forgedb_parser::FieldType::StringN { .. } => "inline_string",
            forgedb_parser::FieldType::Bytes(_) => "bytes",
            forgedb_parser::FieldType::FixedArray(_, _) => "bytes",
            forgedb_parser::FieldType::StructType(_) => "bytes",
            forgedb_parser::FieldType::OptionalStructType(_) => "bytes",
            forgedb_parser::FieldType::Nullable(inner) => Self::type_name(schema, inner),
            _ => "unknown",
        }
    }

    fn get_append_method(schema: &Schema, field_type: &forgedb_parser::FieldType) -> proc_macro2::Ident {
        format_ident!("append_{}", Self::type_name(schema, field_type))
    }

    fn get_read_method(schema: &Schema, field_type: &forgedb_parser::FieldType) -> proc_macro2::Ident {
        format_ident!("read_{}", Self::type_name(schema, field_type))
    }

    fn generate_rehydrate_logic(schema: &Schema, model: &forgedb_parser::Model) -> TokenStream {
        Self::generate_rehydrate_body(schema, model, &quote! { db })
    }

    fn generate_rehydrate_body(
        schema: &Schema,
        model: &forgedb_parser::Model,
        recv: &TokenStream,
    ) -> TokenStream {
        let i = format_ident!("i");
        let versions_push = Self::id_versions_push_stmt(model, recv, &quote! { id }, &quote! { i });
        let id_scan = match Self::generate_id_read_expr(schema, model, recv, &i) {
            Some(read_expr) => quote! {
                for i in 0..n {
                    let id = #read_expr;
                    std::sync::Arc::make_mut(&mut #recv.id_to_row).insert(id, i);
                    #versions_push
                }
            },
            None => quote! {},
        };

        let id_tok = quote! { __id };
        let index_rebuild = {
            let indexed = Self::indexed_fields(model);
            let composites = Self::composite_indexes(model);
            if indexed.is_empty() && composites.is_empty() {
                quote! {}
            } else {
                let id_type = Self::id_type_tokens(schema, model);

                let mut read_fields: Vec<&forgedb_parser::Field> = Vec::new();
                for f in indexed.iter().copied() {
                    read_fields.push(f);
                }
                for (_, comps) in &composites {
                    for c in comps.iter().copied() {
                        if !read_fields.iter().any(|g| g.name == c.name) {
                            read_fields.push(c);
                        }
                    }
                }
                let row_tok = quote! { __row };
                let field_reads: Vec<_> = read_fields
                    .iter()
                    .filter_map(|f| {
                        Self::field_read_stmt(schema, f, recv, &row_tok, false, false).map(|(_, stmt)| stmt)
                    })
                    .collect();

                let mut adds: Vec<_> = indexed
                    .iter()
                    .map(|f| {
                        let ident = Self::index_field_ident(f);
                        let val = format_ident!("{}_value", f.name);
                        let key = Self::index_key_expr(schema, &f.field_type, Self::index_value_expr(&f.field_type, quote! { #val }),
                        );
                        Self::index_add_block(recv, &ident, key, &id_tok)
                    })
                    .collect();
                adds.extend(Self::ordered_index_fields(model).iter().map(|f| {
                    let ident = Self::ordered_index_ident(f);
                    let val = format_ident!("{}_value", f.name);
                    let key = Self::ordered_key_expr(&f.field_type, quote! { #val });
                    Self::ordered_index_add_block(recv, &ident, key, &id_tok)
                }));
                adds.extend(composites.iter().map(|(ident, comps)| {
                    let parts: Vec<_> = comps
                        .iter()
                        .map(|c| {
                            let val = format_ident!("{}_value", c.name);
                            Self::index_key_expr(schema, &c.field_type, Self::index_value_expr(&c.field_type, quote! { #val }),
                            )
                        })
                        .collect();
                    Self::composite_add_block(recv, ident, &parts, &id_tok)
                }));
                quote! {
                    let __ids: Vec<#id_type> = #recv.id_to_row.keys().copied().collect();
                    for __id in __ids {
                        let __row = match #recv.id_to_row.get(&__id) {
                            Some(__r) => *__r,
                            None => continue,
                        };
                        if #recv.tombstones.is_deleted(__row).unwrap_or(true) {
                            continue;
                        }
                        #(#field_reads)*
                        #(#adds)*
                    }
                }
            }
        };

        let autoseq_seed = {
            let identity = model.identity_field().map(|f| f.name.as_str());
            let folds: Vec<TokenStream> = Self::sequence_auto_fields(model)
                .iter()
                .map(|f| {
                    let seq = Self::autoseq_field_ident(f);
                    if Some(f.name.as_str()) == identity {
                        let as_u64 = Self::autoseq_to_u64(f, quote! { __k });
                        quote! {
                            {
                                let mut __max: u64 = 0;
                                for (&__k, _) in #recv.id_to_row.iter() {
                                    let __v = #as_u64;
                                    if __v > __max { __max = __v; }
                                }
                                #recv.#seq.fetch_max(__max, std::sync::atomic::Ordering::SeqCst);
                            }
                        }
                    } else {
                        let col = format_ident!("{}_col", f.name);
                        let read_method = Self::get_read_method(schema, &f.field_type);
                        quote! {
                            {
                                let mut __max: u64 = 0;
                                for __r in 0..n {
                                    if let Ok(__v) = #recv.#col.#read_method(__r) {
                                        if (__v as u64) > __max { __max = __v as u64; }
                                    }
                                }
                                #recv.#seq.fetch_max(__max, std::sync::atomic::Ordering::SeqCst);
                            }
                        }
                    }
                })
                .collect();
            quote! { #(#folds)* }
        };

        quote! {
            let n = #recv.tombstones.len();
            #recv.row_count = n;
            #id_scan
            #index_rebuild
            #autoseq_seed
        }
    }

    fn generate_database(schema: &Schema) -> Result<TokenStream> {
        let db_fields: Vec<_> = schema
            .models
            .iter()
            .map(|model| {
                let field_name = format_ident!("{}", Self::to_snake_case(&model.name));
                let storage_type = format_ident!("{}Storage", model.name);
                quote! { pub #field_name: #storage_type }
            })
            .collect();

        let m2m = Self::valid_m2m(schema);
        let junction_fields: Vec<_> = m2m
            .iter()
            .map(|m| {
                let field = Self::junction_field_ident(m);
                let ty = Self::junction_struct_ident(m);
                quote! { pub #field: #ty }
            })
            .collect();

        let attach_stmts: Vec<_> = schema
            .models
            .iter()
            .map(|model| {
                let field = format_ident!("{}", Self::to_snake_case(&model.name));
                let ty = format_ident!("{}Storage", model.name);
                quote! {
                    let mut #field = #ty::new();
                    #field.attach_changefeed(changefeed.clone());
                }
            })
            .chain(m2m.iter().map(|m| {
                let field = Self::junction_field_ident(m);
                let ty = Self::junction_struct_ident(m);
                quote! {
                    let mut #field = #ty::new();
                    #field.attach_changefeed(changefeed.clone());
                }
            }))
            .collect();
        let attach_stmts_at: Vec<_> = schema
            .models
            .iter()
            .map(|model| {
                let field = format_ident!("{}", Self::to_snake_case(&model.name));
                let ty = format_ident!("{}Storage", model.name);
                quote! {
                    let mut #field = #ty::new_at(&root);
                    #field.attach_changefeed(changefeed.clone());
                    #field.attach_broker(broker.clone());
                }
            })
            .chain(m2m.iter().map(|m| {
                let field = Self::junction_field_ident(m);
                let ty = Self::junction_struct_ident(m);
                quote! {
                    let mut #field = #ty::new_at(&root);
                    #field.attach_changefeed(changefeed.clone());
                    #field.attach_broker(broker.clone());
                }
            }))
            .collect();
        let field_idents: Vec<_> = schema
            .models
            .iter()
            .map(|model| format_ident!("{}", Self::to_snake_case(&model.name)))
            .chain(m2m.iter().map(Self::junction_field_ident))
            .collect();

        let version_guard_stmts: Vec<_> = schema
            .models
            .iter()
            .map(|model| format!("{}/manifest.json", Self::to_snake_case(&model.name)))
            .chain(m2m.iter().map(|m| {
                format!(
                    "{}_{}_link/manifest.json",
                    Self::to_snake_case(&m.model1),
                    Self::to_snake_case(&m.model2)
                )
            }))
            .map(|manifest_rel| {
                quote! {
                    {
                        let __mf = root.join(#manifest_rel);
                        if let Ok(__m) = forgedb_storage::Manifest::load_from(&__mf) {
                            if __m.schema_version != EXPECTED_SCHEMA_VERSION {
                                panic!(
                                    "ForgeDB: data dir at {} is at schema version v{}, \
                                     but this binary expects v{} — the schema changed \
                                     since this dir was written.  Run the migration bin \
                                     to evolve the data (the app never migrates in \
                                     place); do NOT open stale data with mismatched code.",
                                    __mf.display(),
                                    __m.schema_version,
                                    EXPECTED_SCHEMA_VERSION,
                                );
                            }
                            if __m.engine_version != EXPECTED_ENGINE_VERSION {
                                panic!(
                                    "ForgeDB: data dir at {} was written by engine \
                                     format generation {}, but this binary is \
                                     generation {} — ForgeDB's on-disk format \
                                     changed, your schema did not.  Run \
                                     `forgedb migrate engine --src <dir> --dest <new-dir>` \
                                     with the app STOPPED; do NOT open it with \
                                     mismatched code.",
                                    __mf.display(),
                                    __m.engine_version,
                                    EXPECTED_ENGINE_VERSION,
                                );
                            }
                        }
                    }
                }
            })
            .collect();

        let model_field_idents: Vec<_> = schema
            .models
            .iter()
            .map(|model| format_ident!("{}", Self::to_snake_case(&model.name)))
            .collect();

        let snapshot_fields: Vec<_> = schema
            .models
            .iter()
            .map(|model| {
                let field_name = format_ident!("{}", Self::to_snake_case(&model.name));
                quote! { pub #field_name: forgedb_storage::Snapshot }
            })
            .chain(m2m.iter().map(|m| {
                let field = Self::junction_field_ident(m);
                quote! { pub #field: forgedb_storage::Snapshot }
            }))
            .collect();
        let snapshot_inits: Vec<_> = schema
            .models
            .iter()
            .map(|model| {
                let field_name = format_ident!("{}", Self::to_snake_case(&model.name));
                quote! { #field_name: self.#field_name.snapshot() }
            })
            .chain(m2m.iter().map(|m| {
                let field = Self::junction_field_ident(m);
                quote! { #field: self.#field.snapshot() }
            }))
            .collect();

        let reader_fields: Vec<_> = schema
            .models
            .iter()
            .map(|model| {
                let field_name = format_ident!("{}", Self::to_snake_case(&model.name));
                let reader_type = format_ident!("{}StorageReader", model.name);
                quote! { pub #field_name: #reader_type }
            })
            .chain(m2m.iter().map(|m| {
                let field = Self::junction_field_ident(m);
                let ty = format_ident!("{}Reader", Self::junction_struct_ident(m));
                quote! { pub #field: #ty }
            }))
            .collect();
        let reader_inits: Vec<_> = field_idents
            .iter()
            .map(|field| quote! { #field: self.#field.reader() })
            .collect();

        let apply_model_arms: Vec<_> = schema
            .models
            .iter()
            .filter(|m| m.has_identity())
            .map(|model| {
                let field = format_ident!("{}", Self::to_snake_case(&model.name));
                let name = model.name.as_str();
                quote! { #name => self.#field.apply(ev.kind, &ev.bytes), }
            })
            .collect();
        let apply_junction_arms: Vec<_> = m2m
            .iter()
            .map(|m| {
                let field = Self::junction_field_ident(m);
                let base = format!(
                    "{}_{}_link",
                    Self::to_snake_case(&m.model1),
                    Self::to_snake_case(&m.model2)
                );
                let (lt, rt) = Self::junction_key_pair(schema, m);
                let lwv = Self::junction_key_width(&lt);
                let sumv = lwv + Self::junction_key_width(&rt);
                let lw = proc_macro2::Literal::usize_unsuffixed(lwv);
                let sw = proc_macro2::Literal::usize_unsuffixed(sumv);
                let (lw, sw) = (quote! { #lw }, quote! { #sw });
                let l_dec = Self::junction_frame_decode(schema, &lt, &quote! { &ev.bytes[0..#lw] });
                let r_dec =
                    Self::junction_frame_decode(schema, &rt, &quote! { &ev.bytes[#lw..#sw] });
                quote! {
                    #base => {
                        if ev.bytes.len() == #sw {
                            let __l = #l_dec;
                            let __r = #r_dec;
                            self.#field.link(__l, __r);
                        }
                        Ok(())
                    }
                }
            })
            .collect();
        let junction_field_idents: Vec<_> = m2m.iter().map(Self::junction_field_ident).collect();

        let journal_recover_arms: Vec<_> = schema
            .models
            .iter()
            .filter(|m| m.has_identity())
            .map(|model| {
                let field = format_ident!("{}", Self::to_snake_case(&model.name));
                let name = model.name.as_str();
                quote! {
                    #name => __db.#field.__recover_to_committed(__len as usize),
                }
            })
            .collect();

        let cfg = Self::active_cfg();

        let __replication_prune = if cfg.replication && cfg.replication_log_retention > 0 {
            let __retention_lit =
                proc_macro2::Literal::u64_unsuffixed(cfg.replication_log_retention);
            quote! {
                if let Some(__broker) = &self.broker {
                    if let Ok(mut __b) = __broker.lock() {
                        let __wm = __b.watermark();
                        let _ = __b.prune_through(__wm.saturating_sub(#__retention_lit));
                    }
                }
            }
        } else {
            quote! {}
        };

        let __changefeed_capacity = proc_macro2::Literal::usize_unsuffixed(cfg.changefeed_capacity);
        let __journal_fsync = Self::wal_fsync_policy_tokens();
        let __broker_fsync = {
            let variant = format_ident!("{}", cfg.fsync.wal_policy_variant());
            quote! { forgedb_changefeed::durable::FsyncPolicy::#variant }
        };

        let __broker_open = if cfg.replication {
            quote! {
                let broker = match forgedb_changefeed::durable::DurableBroker::open(
                    root.join("_replication.log"),
                    #__broker_fsync,
                    #__changefeed_capacity,
                ) {
                    Ok(b) => Some(std::sync::Arc::new(std::sync::Mutex::new(b))),
                    Err(_) => None,
                };
            }
        } else {
            quote! {
                let broker: Option<
                    std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>,
                > = None;
            }
        };

        let tokens = quote! {
            pub struct Database {
                #(#db_fields,)*
                #(#junction_fields,)*
                pub changefeed: forgedb_changefeed::ChangeFeed,
                pub broker: Option<std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>>,
                _lock: Option<forgedb_storage::DirLock>,
                _txn_journal: Option<forgedb_wal::WalManager>,
                seq: std::sync::Arc<std::sync::Mutex<forgedb_txn::CommitSequencer>>,
            }

            pub struct DatabaseReader {
                #(#reader_fields,)*
            }

            pub struct DatabaseSnapshot {
                #(#snapshot_fields,)*
            }

            impl Database {
                pub fn new() -> Self {
                    let changefeed = forgedb_changefeed::ChangeFeed::new(#__changefeed_capacity);
                    #(#attach_stmts)*
                    Self {
                        #(#field_idents,)*
                        changefeed,
                        broker: None,
                        _lock: None,
                        _txn_journal: None,
                        seq: std::sync::Arc::new(std::sync::Mutex::new(
                            forgedb_txn::CommitSequencer::new(0),
                        )),
                    }
                }

                pub fn open_at(root: std::path::PathBuf) -> Self {
                    let _lock = Some(
                        forgedb_storage::DirLock::acquire(&root).expect(
                            "another writer already holds this data dir \
                             (ForgeDB is single-writer-per-process, #89)",
                        ),
                    );
                    Self::__open_with_lock(root, _lock)
                }

                fn __open_with_lock(
                    root: std::path::PathBuf,
                    _lock: Option<forgedb_storage::DirLock>,
                ) -> Self {
                    #(#version_guard_stmts)*
                    let changefeed = forgedb_changefeed::ChangeFeed::new(#__changefeed_capacity);
                    #__broker_open
                    let __seq_start = match &broker {
                        Some(b) => b.lock().map(|b| b.watermark()).unwrap_or(0),
                        None => 0,
                    };
                    let seq = std::sync::Arc::new(std::sync::Mutex::new(
                        forgedb_txn::CommitSequencer::new(__seq_start),
                    ));
                    let mut _txn_journal = forgedb_wal::WalManager::open(
                        root.join("_txn_journal.log"),
                        #__journal_fsync,
                    ).expect("Failed to open transaction journal");
                    let __journal_committed: Vec<(String, u64)> = {
                        let __entries = _txn_journal
                            .replay(|_| -> std::io::Result<()> { Ok(()) })
                            .unwrap_or_default();
                        match __entries.last() {
                            Some(__e) => {
                                if let forgedb_wal::WalOperation::Raw { payload } = &__e.operation {
                                    serde_json::from_slice(payload).unwrap_or_default()
                                } else {
                                    Vec::new()
                                }
                            }
                            None => Vec::new(),
                        }
                    };
                    #(#attach_stmts_at)*
                    let mut __db = Self {
                        #(#field_idents,)*
                        changefeed,
                        broker,
                        _lock,
                        _txn_journal: Some(_txn_journal),
                        seq,
                    };
                    for (__model, __len) in &__journal_committed {
                        let __len = *__len;
                        match __model.as_str() {
                            #(#journal_recover_arms)*
                            _ => {}
                        }
                    }
                    __db
                }

                pub fn snapshot(&self) -> DatabaseSnapshot {
                    DatabaseSnapshot {
                        #(#snapshot_inits,)*
                    }
                }

                pub fn checkpoint(&mut self) {
                    #(self.#field_idents.checkpoint();)*
                }

                pub fn compact(&mut self) {
                    if let Ok(__seq) = self.seq.lock() {
                        if __seq.oldest_live_snapshot().as_u64() > 0 {
                            return;
                        }
                    }
                    #(self.#model_field_idents.compact();)*
                }

                pub fn maintain(&mut self) {
                    if let Ok(__seq) = self.seq.lock() {
                        if __seq.oldest_live_snapshot().as_u64() > 0 {
                            return;
                        }
                    }
                    #(self.#model_field_idents.maintain();)*
                    #__replication_prune
                }

                pub fn reader(&self) -> DatabaseReader {
                    DatabaseReader {
                        #(#reader_inits,)*
                    }
                }

                pub fn apply_frame(
                    &mut self,
                    ev: &forgedb_changefeed::durable::PersistedEvent,
                ) -> Result<(), ApplyError> {
                    match ev.model.as_str() {
                        #(#apply_model_arms)*
                        #(#apply_junction_arms)*
                        _ => Ok(()),
                    }
                }

                pub fn recover_to(
                    &mut self,
                    base_offset: u64,
                    target_offset: u64,
                ) -> Result<u64, ApplyError> {
                    let broker = self
                        .broker
                        .take()
                        .ok_or_else(|| ApplyError::Decode(
                            "no replication broker: recover_to needs a data dir \
                             opened via open_at with a _replication.log present"
                                .to_string(),
                        ))?;
                    #(self.#field_idents.attach_broker(None);)*

                    const BATCH: usize = 512;
                    let mut after = base_offset;
                    let mut applied = base_offset;
                    let result: Result<u64, ApplyError> = (|| {
                        loop {
                            let frames = {
                                let guard = broker.lock().unwrap();
                                guard
                                    .read_from(after, BATCH)
                                    .map_err(|e| ApplyError::Decode(e.to_string()))?
                            };
                            if frames.is_empty() {
                                break;
                            }
                            for ev in &frames {
                                if ev.offset > target_offset {
                                    return Ok(applied);
                                }
                                self.apply_frame(ev)?;
                                applied = ev.offset;
                                after = ev.offset;
                            }
                        }
                        Ok(applied)
                    })();

                    #(self.#field_idents.attach_broker(Some(broker.clone()));)*
                    self.broker = Some(broker);
                    let applied = result?;
                    self.commit().map_err(|e| ApplyError::Decode(e.to_string()))?;
                    Ok(applied)
                }

                pub fn commit(&mut self) -> std::io::Result<()> {
                    #(
                        self.#model_field_idents.commit()?;
                    )*
                    #(
                        self.#junction_field_idents.checkpoint();
                    )*
                    Ok(())
                }
            }
        };

        Ok(tokens)
    }

    fn generate_validated_writes(schema: &Schema) -> TokenStream {
        let mut methods = Vec::new();
        for model in &schema.models {
            if !model.has_identity() {
                continue;
            }
            let snake = Self::to_snake_case(&model.name);
            let storage_field = format_ident!("{}", snake);
            let model_ident = format_ident!("{}", model.name);
            let id_type = Self::id_type_tokens(schema, model);
            let create_fn = format_ident!("create_{}", snake);
            let update_fn = format_ident!("update_{}", snake);

            let fk_checks: Vec<_> = model
                .fields
                .iter()
                .filter_map(|field| {
                    let (target_name, optional) = match &field.field_type {
                        forgedb_parser::FieldType::Relation(
                            forgedb_parser::RelationType::RequiredReference(t),
                        ) => (t, false),
                        forgedb_parser::FieldType::Relation(
                            forgedb_parser::RelationType::OptionalReference(t),
                        ) => (t, true),
                        _ => return None,
                    };
                    let target = schema.find_model(target_name)?;
                    let target_storage = format_ident!("{}", Self::to_snake_case(&target.name));
                    let fk_field = format_ident!("{}", field.name);
                    let fname = field.name.as_str();
                    let tname = target.name.as_str();
                    let mtag = model.name.as_str();
                    Some(if optional {
                        quote! {
                            if let Some(__fk) = record.#fk_field {
                                if self.#target_storage.get(__fk).is_none() {
                                    return Err(ValidationError::DanglingReference {
                                        model: #mtag, field: #fname, target: #tname,
                                    });
                                }
                            }
                        }
                    } else {
                        quote! {
                            if self.#target_storage.get(record.#fk_field).is_none() {
                                return Err(ValidationError::DanglingReference {
                                    model: #mtag, field: #fname, target: #tname,
                                });
                            }
                        }
                    })
                })
                .collect();

            let auto_synth =
                Self::generate_auto_synthesis(model, &quote! { self.#storage_field });
            methods.push(quote! {
                pub fn #create_fn(&mut self, mut record: #model_ident) -> Result<#id_type, ValidationError> {
                    #auto_synth
                    #(#fk_checks)*
                    self.#storage_field.insert(record)
                }

                pub fn #update_fn(&mut self, id: #id_type, record: #model_ident) -> Result<bool, ValidationError> {
                    #(#fk_checks)*
                    self.#storage_field.update(id, record)
                }
            });
        }

        let delete_methods = Self::generate_delete_wrappers(schema);
        for m in delete_methods {
            methods.push(m);
        }

        if methods.is_empty() {
            return quote! {};
        }
        let __max_cascade_depth =
            proc_macro2::Literal::u32_unsuffixed(Self::active_cfg().max_cascade_depth);
        quote! {
            const MAX_CASCADE_DEPTH: u32 = #__max_cascade_depth;

            impl Database {
                #(#methods)*
            }
        }
    }

    fn generate_delete_wrappers(schema: &Schema) -> Vec<TokenStream> {
        let mut out = Vec::new();

        for parent in &schema.models {
            if !parent.has_identity() {
                continue;
            }
            let parent_snake = Self::to_snake_case(&parent.name);
            let parent_storage = format_ident!("{}", parent_snake);
            let public_fn = format_ident!("delete_{}", parent_snake);
            let cascade_fn = format_ident!("delete_{}_cascade", parent_snake);
            let parent_name_str = parent.name.as_str();
            let id_type = Self::id_type_tokens(schema, parent);

            let mut restrict_checks: Vec<TokenStream> = Vec::new();
            let mut mutations: Vec<TokenStream> = Vec::new();
            for child in &schema.models {
                let child_snake = Self::to_snake_case(&child.name);
                let child_storage = format_ident!("{}", child_snake);
                let child_delete = format_ident!("delete_{}_cascade", child_snake);
                let child_update = format_ident!("update_{}", child_snake);

                for field in &child.fields {
                    let (target_name, optional) = match &field.field_type {
                        forgedb_parser::FieldType::Relation(
                            forgedb_parser::RelationType::RequiredReference(t),
                        ) => (t, false),
                        forgedb_parser::FieldType::Relation(
                            forgedb_parser::RelationType::OptionalReference(t),
                        ) => (t, true),
                        _ => continue,
                    };
                    if target_name != &parent.name {
                        continue;
                    }
                    let fk_field = format_ident!("{}", field.name);
                    let fk_probe = format_ident!("find_by_{}", field.name);
                    let child_field_str = field.name.as_str();
                    let child_name_str = child.name.as_str();
                    let probe_arg = Self::fk_probe_arg(schema, &parent.name, !optional);
                    let child_indexed = child.has_identity();
                    let find_children = if child_indexed {
                        quote! { self.#child_storage.#fk_probe(#probe_arg) }
                    } else {
                        let pred = if optional {
                            quote! { __c.#fk_field == Some(id) }
                        } else {
                            quote! { __c.#fk_field == id }
                        };
                        quote! {
                            self.#child_storage.all().into_iter().filter(|__c| #pred).collect::<Vec<_>>()
                        }
                    };

                    match Self::on_delete_policy(field) {
                        OnDeletePolicy::Restrict => {
                            restrict_checks.push(quote! {
                                {
                                    let __children = #find_children;
                                    if !__children.is_empty() {
                                        return Err(ValidationError::ReferencedByChildren {
                                            model: #child_name_str,
                                            field: #child_field_str,
                                        });
                                    }
                                }
                            });
                        }
                        OnDeletePolicy::Cascade => {
                            mutations.push(quote! {
                                {
                                    let __children = #find_children;
                                    for __c in __children {
                                        self.#child_delete(__c.id, __depth + 1)?;
                                    }
                                }
                            });
                        }
                        OnDeletePolicy::SetNull => {
                            mutations.push(quote! {
                                {
                                    let __children = #find_children;
                                    for mut __c in __children {
                                        let __cid = __c.id;
                                        __c.#fk_field = None;
                                        self.#child_update(__cid, __c)?;
                                    }
                                }
                            });
                        }
                    }
                }
            }

            let mut m2m_unlinks: Vec<TokenStream> = Vec::new();
            for m in Self::valid_m2m(schema) {
                let junction_field = Self::junction_field_ident(&m);
                if m.model1 == parent.name {
                    m2m_unlinks.push(quote! {
                        self.#junction_field.unlink_all_left(id);
                    });
                }
                if m.model2 == parent.name {
                    let junction_field2 = Self::junction_field_ident(&m);
                    m2m_unlinks.push(quote! {
                        self.#junction_field2.unlink_all_right(id);
                    });
                }
            }


            out.push(quote! {
                pub fn #public_fn(&mut self, id: #id_type) -> Result<bool, ValidationError> {
                    self.#cascade_fn(id, 0)
                }

                fn #cascade_fn(&mut self, id: #id_type, __depth: u32) -> Result<bool, ValidationError> {
                    if __depth > MAX_CASCADE_DEPTH {
                        return Err(ValidationError::ReferencedByChildren {
                            model: #parent_name_str,
                            field: "on_delete cascade depth exceeded",
                        });
                    }
                    if self.#parent_storage.get(id).is_none() {
                        return Ok(false);
                    }
                    #(#restrict_checks)*
                    #(#mutations)*
                    #(#m2m_unlinks)*
                    Ok(self.#parent_storage.delete(id))
                }
            });
        }

        out
    }

    fn generate_transaction_impl(schema: &Schema) -> TokenStream {
        let tx_models: Vec<&forgedb_parser::Model> = schema
            .models
            .iter()
            .filter(|m| m.has_identity())
            .collect();
        if tx_models.is_empty() {
            return quote! {};
        }

        let __txn_max_retries =
            proc_macro2::Literal::u32_unsuffixed(Self::active_cfg().txn_max_retries);

        let (seq_field, seq_init, seq_ws_self) = Self::sequence_claim_plumbing(
            schema,
            &quote! { self },
            &quote! { keys },
            true,
        );

        let rollback_arms: Vec<_> = tx_models
            .iter()
            .map(|m| {
                let tag = m.name.as_str();
                let field = format_ident!("{}", Self::to_snake_case(&m.name));
                quote! {
                    #tag => {
                        let __mark = *__mark;
                        let _ = self.db.#field.__truncate_all_to(__mark);
                        self.db.#field.row_count = __mark;
                        if let Some(__wm) = self.wal_marks.get(#tag) {
                            let _ = self.db.#field.wal.truncate_to(*__wm);
                        }
                        self.db.#field.in_transaction = false;
                        self.db.#field.run_deferred_maintenance();
                    }
                }
            })
            .collect();

        let commit_reindex_arms: Vec<_> = tx_models
            .iter()
            .map(|m| {
                let tag = m.name.as_str();
                let field = format_ident!("{}", Self::to_snake_case(&m.name));
                quote! {
                    #tag => {
                        self.db.#field.__reindex_committed();
                        self.db.#field.in_transaction = false;
                        self.db.#field.run_deferred_maintenance();
                    }
                }
            })
            .collect();

        let commit_fsync_arms: Vec<_> = tx_models
            .iter()
            .map(|m| {
                let tag = m.name.as_str();
                let field = format_ident!("{}", Self::to_snake_case(&m.name));
                quote! {
                    #tag => {
                        let _ = self.db.#field.commit();
                        let _ = self.db.#field.wal.flush();
                        __journal.push((#tag.to_string(), self.db.#field.row_count as u64));
                    }
                }
            })
            .collect();

        let mut tx_methods: Vec<TokenStream> = Vec::new();
        for model in &tx_models {
            tx_methods.push(Self::generate_txn_model_methods(model, schema));
        }

        let mark_methods: Vec<_> = tx_models
            .iter()
            .map(|m| {
                let tag = m.name.as_str();
                let field = format_ident!("{}", Self::to_snake_case(&m.name));
                let mark_fn = format_ident!("__mark_{}", Self::to_snake_case(&m.name));
                quote! {
                    fn #mark_fn(&mut self) {
                        if !self.marks.contains_key(#tag) {
                            let __rc = self.db.#field.row_count;
                            let __wb = self.db.#field.wal.size().unwrap_or(0);
                            self.marks.insert(#tag, __rc);
                            self.wal_marks.insert(#tag, __wb);
                            self.db.#field.in_transaction = true;
                        }
                    }
                }
            })
            .collect();

        quote! {
            pub struct TxHandle<'db> {
                db: &'db mut Database,
                marks: std::collections::BTreeMap<&'static str, usize>,
                wal_marks: std::collections::BTreeMap<&'static str, u64>,
                pending_events: Vec<(&'static str, forgedb_changefeed::ChangeKind, Vec<u8>, usize, Vec<u8>)>,
                staged_unique_keys:
                    std::collections::BTreeSet<(&'static str, &'static str, String)>,
                #seq_field
                committed: bool,
            }

            impl<'db> TxHandle<'db> {
                fn begin(db: &'db mut Database) -> Self {
                    TxHandle {
                        db,
                        marks: std::collections::BTreeMap::new(),
                        wal_marks: std::collections::BTreeMap::new(),
                        pending_events: Vec::new(),
                        staged_unique_keys: std::collections::BTreeSet::new(),
                        #seq_init
                        committed: false,
                    }
                }

                #(#mark_methods)*

                #(#tx_methods)*

                pub fn commit(mut self) -> Result<(), TxError> {
                    let mut __journal: Vec<(String, u64)> = Vec::new();
                    let __touched: Vec<&'static str> = self.marks.keys().copied().collect();
                    for __tag in &__touched {
                        match *__tag {
                            #(#commit_fsync_arms)*
                            _ => {}
                        }
                    }
                    if let Some(__jrnl) = self.db._txn_journal.as_mut() {
                        let __payload = serde_json::to_vec(&__journal)
                            .map_err(|e| TxError::Io(e.to_string()))?;
                        __jrnl
                            .write(&forgedb_wal::WalEntry::raw("_txn", __payload))
                            .map_err(|e| TxError::Io(e.to_string()))?;
                        __jrnl.flush().map_err(|e| TxError::Io(e.to_string()))?;
                    }
                    for __tag in &__touched {
                        match *__tag {
                            #(#commit_reindex_arms)*
                            _ => {}
                        }
                    }
                    for (__model, __kind, _id_bytes, __row, __bytes) in std::mem::take(&mut self.pending_events) {
                        self.db.changefeed.emit(__model, __row, __kind);
                        if let Some(__broker) = &self.db.broker {
                            if let Ok(mut __b) = __broker.lock() {
                                let _ = __b.record(__model, __row as u64, __kind, __bytes);
                            }
                        }
                    }
                    self.committed = true;
                    Ok(())
                }

                pub fn rollback(mut self) {
                    self.rollback_internal();
                    self.committed = true;
                }

                fn rollback_internal(&mut self) {
                    let __touched: Vec<&'static str> = self.marks.keys().copied().collect();
                    for __tag in &__touched {
                        if let Some(__mark) = self.marks.get(*__tag) {
                            match *__tag {
                                #(#rollback_arms)*
                                _ => {}
                            }
                        }
                    }
                    self.pending_events.clear();
                }

                pub fn __write_set(&self, snapshot_lsn: forgedb_txn::Lsn) -> forgedb_txn::WriteSet {
                    let mut keys: Vec<forgedb_txn::OpaqueKey> = Vec::new();
                    for (__model, _kind, __id_bytes, _row, _bytes) in &self.pending_events {
                        keys.push(
                            __forgedb_ws_key(&[b"r", __model.as_bytes(), __id_bytes])
                                .into_boxed_slice(),
                        );
                    }
                    for (__mtag, __fname, __ekey) in &self.staged_unique_keys {
                        keys.push(
                            __forgedb_ws_key(&[
                                b"u",
                                __mtag.as_bytes(),
                                __fname.as_bytes(),
                                __ekey.as_bytes(),
                            ])
                            .into_boxed_slice(),
                        );
                    }
                    #seq_ws_self
                    forgedb_txn::WriteSet { keys, snapshot_lsn }
                }
            }

            impl<'db> Drop for TxHandle<'db> {
                fn drop(&mut self) {
                    if !self.committed {
                        self.rollback_internal();
                    }
                }
            }

            const DEFAULT_TXN_RETRIES: u32 = #__txn_max_retries;

            impl Database {
                pub fn transaction<T>(
                    &mut self,
                    f: impl FnOnce(&mut TxHandle) -> Result<T, TxError>,
                ) -> Result<T, TxError> {
                    let mut tx = TxHandle::begin(self);
                    match f(&mut tx) {
                        Ok(v) => {
                            tx.commit()?;
                            Ok(v)
                        }
                        Err(e) => {
                            tx.rollback();
                            Err(e)
                        }
                    }
                }

                pub fn transaction_retrying<T>(
                    &mut self,
                    retries: u32,
                    f: impl Fn(&mut TxHandle) -> Result<T, TxError>,
                ) -> Result<T, TxError> {
                    let __seq_arc = std::sync::Arc::clone(&self.seq);
                    for __attempt in 0..=retries {
                        let __snap_lsn = {
                            let mut __seq = self.seq.lock().unwrap();
                            __seq.register_snapshot()
                        };
                        let mut tx = TxHandle::begin(self);
                        let __out = f(&mut tx);
                        let __val = match __out {
                            Ok(v) => v,
                            Err(e) => {
                                tx.rollback();
                                let mut __seq = __seq_arc.lock().unwrap();
                                __seq.release_snapshot(__snap_lsn);
                                return Err(e);
                            }
                        };
                        let __ws = tx.__write_set(__snap_lsn);
                        let __outcome = {
                            let mut __seq = __seq_arc.lock().unwrap();
                            __seq.try_commit(&__ws)
                        };
                        match __outcome {
                            forgedb_txn::CommitOutcome::Committed(_l) => {
                                tx.commit()?;
                                let mut __seq = __seq_arc.lock().unwrap();
                                __seq.release_snapshot(__snap_lsn);
                                let _ = __attempt;
                                return Ok(__val);
                            }
                            forgedb_txn::CommitOutcome::Conflict { .. } => {
                                tx.rollback();
                                let mut __seq = __seq_arc.lock().unwrap();
                                __seq.release_snapshot(__snap_lsn);
                            }
                        }
                    }
                    Err(TxError::Conflict)
                }

                pub fn transaction_optimistic<T>(
                    &mut self,
                    f: impl Fn(&mut TxHandle) -> Result<T, TxError>,
                ) -> Result<T, TxError> {
                    self.transaction_retrying(DEFAULT_TXN_RETRIES, f)
                }
            }
        }
    }

    fn generate_shared_database_impl(schema: &Schema) -> TokenStream {
        let (seq_field, seq_init, seq_ws_tx) = Self::sequence_claim_plumbing(
            schema,
            &quote! { __tx },
            &quote! { __ws_keys },
            true,
        );
        let tx_models: Vec<&forgedb_parser::Model> = schema
            .models
            .iter()
            .filter(|m| m.has_identity())
            .collect();
        if tx_models.is_empty() {
            return quote! {};
        }

        let concurrent_apply_arms: Vec<_> = tx_models
            .iter()
            .map(|m| {
                let tag = m.name.as_str();
                let model_name = format_ident!("{}", m.name);
                let field = format_ident!("{}", Self::to_snake_case(&m.name));
                quote! {
                    #tag => {
                        let __record: #model_name = serde_json::from_slice(__bytes)
                            .map_err(|e| TxError::Io(e.to_string()))?;
                        let __row = self.#field.__stage_append(__record, *__deleted);
                        __staged_events.push((*__model_tag, *__kind, __row, __bytes.clone()));
                    }
                }
            })
            .collect();

        let concurrent_mark_arms: Vec<_> = tx_models
            .iter()
            .map(|m| {
                let tag = m.name.as_str();
                let field = format_ident!("{}", Self::to_snake_case(&m.name));
                quote! {
                    #tag => {
                        if !__marks.contains_key(#tag) {
                            let __rc = self.#field.row_count;
                            let __wb = self.#field.wal.size().unwrap_or(0);
                            __marks.insert(#tag, __rc);
                            __wal_marks.insert(#tag, __wb);
                            self.#field.in_transaction = true;
                        }
                    }
                }
            })
            .collect();

        let concurrent_fsync_arms: Vec<_> = tx_models
            .iter()
            .map(|m| {
                let tag = m.name.as_str();
                let field = format_ident!("{}", Self::to_snake_case(&m.name));
                quote! {
                    #tag => {
                        let _ = self.#field.commit();
                        let _ = self.#field.wal.flush();
                        __journal.push((#tag.to_string(), self.#field.row_count as u64));
                    }
                }
            })
            .collect();

        let concurrent_reindex_arms: Vec<_> = tx_models
            .iter()
            .map(|m| {
                let tag = m.name.as_str();
                let field = format_ident!("{}", Self::to_snake_case(&m.name));
                quote! {
                    #tag => {
                        self.#field.__reindex_committed();
                        self.#field.in_transaction = false;
                        self.#field.run_deferred_maintenance();
                    }
                }
            })
            .collect();

        let concurrent_rollback_arms: Vec<_> = tx_models
            .iter()
            .map(|m| {
                let tag = m.name.as_str();
                let field = format_ident!("{}", Self::to_snake_case(&m.name));
                quote! {
                    #tag => {
                        if let Some(&__mark) = __marks.get(#tag) {
                            let _ = self.#field.__truncate_all_to(__mark);
                            self.#field.row_count = __mark;
                            if let Some(&__wm) = __wal_marks.get(#tag) {
                                let _ = self.#field.wal.truncate_to(__wm);
                            }
                            self.#field.in_transaction = false;
                            self.#field.run_deferred_maintenance();
                        }
                    }
                }
            })
            .collect();

        let mut concurrent_model_methods: Vec<TokenStream> = Vec::new();
        for model in &tx_models {
            let model_name = format_ident!("{}", model.name);
            let model_tag = model.name.as_str();
            let snake = Self::to_snake_case(&model.name);
            let field = format_ident!("{}", snake);
            let id_type = Self::id_type_tokens(schema, model);
            let id_field = Self::id_field_ident(model);
            let validate_fn = format_ident!("validate_{}", snake);
            let create_fn = format_ident!("create_{}", snake);
            let update_fn = format_ident!("update_{}", snake);
            let delete_fn = format_ident!("delete_{}", snake);
            let get_fn = format_ident!("get_{}", snake);
            let all_fn = format_ident!("all_{}", snake);
            let snap_field = format_ident!("{}", snake);
            let auto_synth = Self::generate_auto_synthesis(
                model,
                &quote! { self.inner.read().unwrap().#snap_field },
            );
            let ts_gate = Self::generate_timestamp_write_gate(schema, model);

            let seq_claim = Self::generate_sequence_claim_staging(model);
            let unique_checks_insert: Vec<_> = Self::indexed_fields(model)
                .iter()
                .filter(|f| f.unique)
                .map(|f| {
                    let ident = Self::index_field_ident(f);
                    let fident = format_ident!("{}", f.name);
                    let fname = f.name.as_str();
                    let mtag = model.name.as_str();
                    let key = Self::index_key_expr(schema, &f.field_type, Self::index_value_expr(&f.field_type,
                        quote! { record.#fident },
                    ));
                    quote! {
                        {
                            let __uk: String = { #key };
                            if self.staged_unique_keys.contains(&(#mtag, #fname, __uk.clone())) {
                                return Err(TxError::Validation(ValidationError::Unique { model: #mtag, field: #fname }));
                            }
                            {
                                let __db = self.inner.read().unwrap();
                                if __db.#field.#ident.get(&__uk).is_some_and(|__ids| !__ids.is_empty()) {
                                    return Err(TxError::Validation(ValidationError::Unique { model: #mtag, field: #fname }));
                                }
                            }
                            self.staged_unique_keys.insert((#mtag, #fname, __uk));
                        }
                    }
                })
                .collect();

            let unique_checks_update: Vec<_> = Self::indexed_fields(model)
                .iter()
                .filter(|f| f.unique)
                .map(|f| {
                    let ident = Self::index_field_ident(f);
                    let fident = format_ident!("{}", f.name);
                    let fname = f.name.as_str();
                    let mtag = model.name.as_str();
                    let key = Self::index_key_expr(schema, &f.field_type, Self::index_value_expr(&f.field_type,
                        quote! { record.#fident },
                    ));
                    quote! {
                        {
                            let __uk: String = { #key };
                            {
                                let __db = self.inner.read().unwrap();
                                if let Some(__ids) = __db.#field.#ident.get(&__uk) {
                                    if __ids.iter().any(|__i| *__i != id) {
                                        return Err(TxError::Validation(ValidationError::Unique { model: #mtag, field: #fname }));
                                    }
                                }
                            }
                            let __committed_owns = {
                                let __db = self.inner.read().unwrap();
                                __db.#field.#ident.get(&__uk).is_some_and(|__ids| __ids.contains(&id))
                            };
                            if !__committed_owns && self.staged_unique_keys.contains(&(#mtag, #fname, __uk.clone())) {
                                return Err(TxError::Validation(ValidationError::Unique { model: #mtag, field: #fname }));
                            }
                            self.staged_unique_keys.insert((#mtag, #fname, __uk));
                        }
                    }
                })
                .collect();

            let fk_checks: Vec<_> = model
                .fields
                .iter()
                .filter_map(|f| {
                    let (target_name, optional) = match &f.field_type {
                        forgedb_parser::FieldType::Relation(
                            forgedb_parser::RelationType::RequiredReference(t),
                        ) => (t, false),
                        forgedb_parser::FieldType::Relation(
                            forgedb_parser::RelationType::OptionalReference(t),
                        ) => (t, true),
                        _ => return None,
                    };
                    let target = schema.find_model(target_name)?;
                    let target_get_fn = format_ident!("get_{}", Self::to_snake_case(&target.name));
                    let fk_field = format_ident!("{}", f.name);
                    let fname = f.name.as_str();
                    let tname = target.name.as_str();
                    let mtag = model.name.as_str();
                    Some(if optional {
                        quote! {
                            if let Some(__fk) = record.#fk_field {
                                if self.#target_get_fn(__fk).is_none() {
                                    return Err(TxError::Validation(ValidationError::DanglingReference {
                                        model: #mtag, field: #fname, target: #tname,
                                    }));
                                }
                            }
                        }
                    } else {
                        quote! {
                            if self.#target_get_fn(record.#fk_field).is_none() {
                                return Err(TxError::Validation(ValidationError::DanglingReference {
                                    model: #mtag, field: #fname, target: #tname,
                                }));
                            }
                        }
                    })
                })
                .collect();

            concurrent_model_methods.push(quote! {
                pub fn #create_fn(&mut self, mut record: #model_name) -> Result<#id_type, TxError> {
                    #auto_synth
                    #ts_gate
                    #validate_fn(&record)?;
                    #(#unique_checks_insert)*
                    #seq_claim
                    #(#fk_checks)*
                    let id = record.#id_field;
                    let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
                    let __bytes = serde_json::to_vec(&record).unwrap_or_default();
                    self.buffer.push((#model_tag, forgedb_changefeed::ChangeKind::Inserted, __id_bytes.clone(), __bytes.clone(), false));
                    self.pending_events.push((#model_tag, forgedb_changefeed::ChangeKind::Inserted, __id_bytes, __bytes));
                    Ok(id)
                }

                pub fn #update_fn(&mut self, id: #id_type, record: #model_name) -> Result<bool, TxError> {
                    if self.#get_fn(id).is_none() {
                        return Ok(false);
                    }
                    #ts_gate
                    #validate_fn(&record)?;
                    #(#unique_checks_update)*
                    #(#fk_checks)*
                    let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
                    let __bytes = serde_json::to_vec(&record).unwrap_or_default();
                    self.buffer.push((#model_tag, forgedb_changefeed::ChangeKind::Updated, __id_bytes.clone(), __bytes.clone(), false));
                    self.pending_events.push((#model_tag, forgedb_changefeed::ChangeKind::Updated, __id_bytes, __bytes));
                    Ok(true)
                }

                pub fn #delete_fn(&mut self, id: #id_type) -> bool {
                    let record = match self.#get_fn(id) {
                        Some(r) => r,
                        None => return false,
                    };
                    let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
                    let __bytes = serde_json::to_vec(&record).unwrap_or_default();
                    self.buffer.push((#model_tag, forgedb_changefeed::ChangeKind::Deleted, __id_bytes.clone(), __bytes.clone(), true));
                    self.pending_events.push((#model_tag, forgedb_changefeed::ChangeKind::Deleted, __id_bytes, __bytes));
                    true
                }

                pub fn #get_fn(&self, id: #id_type) -> Option<#model_name> {
                    let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
                    for (_, _kind, __ib, __bytes, __del) in self.buffer.iter().rev() {
                        if __ib == &__id_bytes {
                            if *__del { return None; }
                            return serde_json::from_slice(__bytes).ok();
                        }
                    }
                    let __db = self.inner.read().unwrap();
                    __db.#field.get_at(&self.snap.#snap_field, id)
                }

                pub fn #all_fn(&self) -> Vec<#model_name> {
                    let __db = self.inner.read().unwrap();
                    let mut __rows = __db.#field.all_at(&self.snap.#snap_field);
                    drop(__db);
                    for (_, _kind, __ib, __bytes, __del) in &self.buffer {
                        let __id_bytes_ref: &Vec<u8> = __ib;
                        if *__del {
                            __rows.retain(|r| {
                                serde_json::to_vec(&r.#id_field).unwrap_or_default().as_slice() != __id_bytes_ref.as_slice()
                            });
                        } else if let Ok(__rec) = serde_json::from_slice::<#model_name>(__bytes) {
                            let __staged_id = __rec.#id_field;
                            if let Some(__pos) = __rows.iter().position(|r| r.#id_field == __staged_id) {
                                __rows[__pos] = __rec;
                            } else {
                                __rows.push(__rec);
                            }
                        }
                    }
                    __rows
                }
            });
        }

        quote! {
            #[derive(Clone)]
            pub struct SharedDatabase {
                inner: std::sync::Arc<std::sync::RwLock<Database>>,
                seq: std::sync::Arc<std::sync::Mutex<forgedb_txn::CommitSequencer>>,
            }

            impl SharedDatabase {
                pub fn transaction_concurrent<T>(
                    &self,
                    retries: u32,
                    f: impl Fn(&mut ConcurrentTxHandle) -> Result<T, TxError>,
                ) -> Result<T, TxError> {
                    let __seq_arc = std::sync::Arc::clone(&self.seq);
                    for __attempt in 0..=retries {
                        let __snap_lsn = {
                            let mut __seq = __seq_arc.lock().unwrap();
                            __seq.register_snapshot()
                        };
                        let __snap = {
                            let __db = self.inner.read().unwrap();
                            __db.snapshot()
                        };
                        let mut __tx = ConcurrentTxHandle {
                            inner: std::sync::Arc::clone(&self.inner),
                            snap: __snap,
                            buffer: Vec::new(),
                            staged_unique_keys: std::collections::BTreeSet::new(),
                            #seq_init
                            pending_events: Vec::new(),
                        };
                        let __out = f(&mut __tx);
                        let __val = match __out {
                            Ok(v) => v,
                            Err(e) => {
                                let mut __seq = __seq_arc.lock().unwrap();
                                __seq.release_snapshot(__snap_lsn);
                                return Err(e);
                            }
                        };
                        let mut __ws_keys: Vec<forgedb_txn::OpaqueKey> = Vec::new();
                        for (__model, _kind, __id_bytes, _bytes, _del) in &__tx.buffer {
                            __ws_keys.push(
                                __forgedb_ws_key(&[b"r", __model.as_bytes(), __id_bytes])
                                    .into_boxed_slice(),
                            );
                        }
                        for (__mtag, __fname, __ekey) in &__tx.staged_unique_keys {
                            __ws_keys.push(
                                __forgedb_ws_key(&[
                                    b"u",
                                    __mtag.as_bytes(),
                                    __fname.as_bytes(),
                                    __ekey.as_bytes(),
                                ])
                                .into_boxed_slice(),
                            );
                        }
                        #seq_ws_tx
                        let __ws = forgedb_txn::WriteSet { keys: __ws_keys, snapshot_lsn: __snap_lsn };
                        let __outcome = {
                            let mut __seq = __seq_arc.lock().unwrap();
                            __seq.try_commit(&__ws)
                        };
                        match __outcome {
                            forgedb_txn::CommitOutcome::Committed(_l) => {
                                {
                                    let mut __db = self.inner.write().unwrap();
                                    let __buf = __tx.buffer;
                                    let __evts = __tx.pending_events;
                                    __db.__apply_and_commit_concurrent_buffer(__buf, __evts)?;
                                }
                                let mut __seq = __seq_arc.lock().unwrap();
                                __seq.release_snapshot(__snap_lsn);
                                let _ = __attempt;
                                return Ok(__val);
                            }
                            forgedb_txn::CommitOutcome::Conflict { .. } => {
                                let mut __seq = __seq_arc.lock().unwrap();
                                __seq.release_snapshot(__snap_lsn);
                            }
                        }
                    }
                    Err(TxError::Conflict)
                }
            }

            pub struct ConcurrentTxHandle {
                inner: std::sync::Arc<std::sync::RwLock<Database>>,
                snap: DatabaseSnapshot,
                buffer: Vec<(&'static str, forgedb_changefeed::ChangeKind, Vec<u8>, Vec<u8>, bool)>,
                staged_unique_keys:
                    std::collections::BTreeSet<(&'static str, &'static str, String)>,
                #seq_field
                pending_events: Vec<(&'static str, forgedb_changefeed::ChangeKind, Vec<u8>, Vec<u8>)>,
            }

            impl ConcurrentTxHandle {
                #(#concurrent_model_methods)*
            }

            impl Database {
                pub fn shared(self) -> SharedDatabase {
                    let seq = std::sync::Arc::clone(&self.seq);
                    SharedDatabase {
                        inner: std::sync::Arc::new(std::sync::RwLock::new(self)),
                        seq,
                    }
                }

                pub(crate) fn __apply_and_commit_concurrent_buffer(
                    &mut self,
                    buffer: Vec<(&'static str, forgedb_changefeed::ChangeKind, Vec<u8>, Vec<u8>, bool)>,
                    events: Vec<(&'static str, forgedb_changefeed::ChangeKind, Vec<u8>, Vec<u8>)>,
                ) -> Result<Vec<(Vec<u8>, u64)>, TxError> {
                    let mut __marks: std::collections::BTreeMap<&'static str, usize> =
                        std::collections::BTreeMap::new();
                    let mut __wal_marks: std::collections::BTreeMap<&'static str, u64> =
                        std::collections::BTreeMap::new();

                    for (__model_tag, _kind, _id_bytes, _bytes, _deleted) in &buffer {
                        match *__model_tag {
                            #(#concurrent_mark_arms)*
                            _ => {}
                        }
                    }

                    let mut __staged_events: Vec<(&'static str, forgedb_changefeed::ChangeKind, usize, Vec<u8>)> = Vec::new();
                    for (__model_tag, __kind, _id_bytes, __bytes, __deleted) in &buffer {
                        let __result: Result<(), TxError> = (|| {
                            match *__model_tag {
                                #(#concurrent_apply_arms)*
                                _ => {}
                            }
                            Ok(())
                        })();
                        if let Err(e) = __result {
                            let __touched: Vec<&'static str> = __marks.keys().copied().collect();
                            for __tag in &__touched {
                                match *__tag {
                                    #(#concurrent_rollback_arms)*
                                    _ => {}
                                }
                            }
                            return Err(e);
                        }
                    }

                    let mut __journal: Vec<(String, u64)> = Vec::new();
                    let __touched: Vec<&'static str> = __marks.keys().copied().collect();
                    for __tag in &__touched {
                        match *__tag {
                            #(#concurrent_fsync_arms)*
                            _ => {}
                        }
                    }
                    if let Some(__jrnl) = self._txn_journal.as_mut() {
                        let __payload = serde_json::to_vec(&__journal)
                            .map_err(|e| TxError::Io(e.to_string()))?;
                        __jrnl
                            .write(&forgedb_wal::WalEntry::raw("_txn", __payload))
                            .map_err(|e| TxError::Io(e.to_string()))?;
                        __jrnl.flush().map_err(|e| TxError::Io(e.to_string()))?;
                    }
                    for __tag in &__touched {
                        match *__tag {
                            #(#concurrent_reindex_arms)*
                            _ => {}
                        }
                    }
                    let mut __row_index_pairs: Vec<(Vec<u8>, u64)> = Vec::with_capacity(__staged_events.len());
                    for (__model, __kind, __row, __bytes) in __staged_events {
                        __row_index_pairs.push((__model.as_bytes().to_vec(), __row as u64));
                        self.changefeed.emit(__model, __row, __kind);
                        if let Some(__broker) = &self.broker {
                            if let Ok(mut __b) = __broker.lock() {
                                let _ = __b.record(__model, __row as u64, __kind, __bytes);
                            }
                        }
                    }
                    let _ = events;
                    Ok(__row_index_pairs)
                }
            }
        }
    }

    fn generate_txn_model_methods(
        model: &forgedb_parser::Model,
        schema: &Schema,
    ) -> TokenStream {
        let model_name = format_ident!("{}", model.name);
        let model_tag = model.name.as_str();
        let snake = Self::to_snake_case(&model.name);
        let field = format_ident!("{}", snake);
        let id_type = Self::id_type_tokens(schema, model);
        let id_field = Self::id_field_ident(model);
        let mark_fn = format_ident!("__mark_{}", snake);
        let create_fn = format_ident!("create_{}", snake);
        let update_fn = format_ident!("update_{}", snake);
        let delete_fn = format_ident!("delete_{}", snake);
        let get_fn = format_ident!("get_{}", snake);
        let all_fn = format_ident!("all_{}", snake);
        let validate_fn = format_ident!("validate_{}", snake);
        let auto_synth = Self::generate_auto_synthesis(model, &quote! { self.db.#field });
        let ts_gate = Self::generate_timestamp_write_gate(schema, model);

        let seq_claim = Self::generate_sequence_claim_staging(model);
        let unique_checks_insert: Vec<_> = Self::indexed_fields(model)
            .iter()
            .filter(|f| f.unique)
            .map(|f| {
                let ident = Self::index_field_ident(f);
                let fident = format_ident!("{}", f.name);
                let fname = f.name.as_str();
                let mtag = model.name.as_str();
                let key = Self::index_key_expr(schema, &f.field_type, Self::index_value_expr(&f.field_type,
                    quote! { record.#fident },
                ));
                quote! {
                    {
                        let __uk: String = { #key };
                        if self.db.#field.#ident.get(&__uk).is_some_and(|__ids| !__ids.is_empty()) {
                            return Err(TxError::Validation(ValidationError::Unique { model: #mtag, field: #fname }));
                        }
                        if self.staged_unique_keys.contains(&(#mtag, #fname, __uk.clone())) {
                            return Err(TxError::Validation(ValidationError::Unique { model: #mtag, field: #fname }));
                        }
                        self.staged_unique_keys.insert((#mtag, #fname, __uk));
                    }
                }
            })
            .collect();
        let unique_checks_update: Vec<_> = Self::indexed_fields(model)
            .iter()
            .filter(|f| f.unique)
            .map(|f| {
                let ident = Self::index_field_ident(f);
                let fident = format_ident!("{}", f.name);
                let fname = f.name.as_str();
                let mtag = model.name.as_str();
                let key = Self::index_key_expr(schema, &f.field_type, Self::index_value_expr(&f.field_type,
                    quote! { record.#fident },
                ));
                quote! {
                    {
                        let __uk: String = { #key };
                        if let Some(__ids) = self.db.#field.#ident.get(&__uk) {
                            if __ids.iter().any(|__i| *__i != id) {
                                return Err(TxError::Validation(ValidationError::Unique { model: #mtag, field: #fname }));
                            }
                        }
                        let __committed_owns = self.db.#field.#ident
                            .get(&__uk)
                            .is_some_and(|__ids| __ids.contains(&id));
                        if !__committed_owns
                            && self.staged_unique_keys.contains(&(#mtag, #fname, __uk.clone()))
                        {
                            return Err(TxError::Validation(ValidationError::Unique { model: #mtag, field: #fname }));
                        }
                        self.staged_unique_keys.insert((#mtag, #fname, __uk));
                    }
                }
            })
            .collect();

        let fk_checks: Vec<_> = model
            .fields
            .iter()
            .filter_map(|f| {
                let (target_name, optional) = match &f.field_type {
                    forgedb_parser::FieldType::Relation(
                        forgedb_parser::RelationType::RequiredReference(t),
                    ) => (t, false),
                    forgedb_parser::FieldType::Relation(
                        forgedb_parser::RelationType::OptionalReference(t),
                    ) => (t, true),
                    _ => return None,
                };
                let target = schema.find_model(target_name)?;
                let target_get = format_ident!("get_{}", Self::to_snake_case(&target.name));
                let fk_field = format_ident!("{}", f.name);
                let fname = f.name.as_str();
                let tname = target.name.as_str();
                let mtag = model.name.as_str();
                Some(if optional {
                    quote! {
                        if let Some(__fk) = record.#fk_field {
                            if self.#target_get(__fk).is_none() {
                                return Err(TxError::Validation(ValidationError::DanglingReference {
                                    model: #mtag, field: #fname, target: #tname,
                                }));
                            }
                        }
                    }
                } else {
                    quote! {
                        if self.#target_get(record.#fk_field).is_none() {
                            return Err(TxError::Validation(ValidationError::DanglingReference {
                                model: #mtag, field: #fname, target: #tname,
                            }));
                        }
                    }
                })
            })
            .collect();


        quote! {
            pub fn #create_fn(&mut self, mut record: #model_name) -> Result<#id_type, TxError> {
                self.#mark_fn();
                #auto_synth
                #ts_gate
                #validate_fn(&record)?;
                #(#unique_checks_insert)*
                #seq_claim
                #(#fk_checks)*
                let id = record.#id_field;
                let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
                let __bytes = serde_json::to_vec(&record).unwrap_or_default();
                let __row = self.db.#field.__stage_append(record, false);
                self.pending_events.push((#model_tag, forgedb_changefeed::ChangeKind::Inserted, __id_bytes, __row, __bytes));
                Ok(id)
            }

            pub fn #update_fn(&mut self, id: #id_type, record: #model_name) -> Result<bool, TxError> {
                if self.#get_fn(id).is_none() {
                    return Ok(false);
                }
                self.#mark_fn();
                #ts_gate
                #validate_fn(&record)?;
                #(#unique_checks_update)*
                #(#fk_checks)*
                let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
                let __bytes = serde_json::to_vec(&record).unwrap_or_default();
                let __row = self.db.#field.__stage_append(record, false);
                self.pending_events.push((#model_tag, forgedb_changefeed::ChangeKind::Updated, __id_bytes, __row, __bytes));
                Ok(true)
            }

            pub fn #delete_fn(&mut self, id: #id_type) -> bool {
                let record = match self.#get_fn(id) {
                    Some(r) => r,
                    None => return false,
                };
                self.#mark_fn();
                let __id_bytes = serde_json::to_vec(&id).unwrap_or_default();
                let __bytes = serde_json::to_vec(&record).unwrap_or_default();
                let __row = self.db.#field.__stage_append(record, true);
                self.pending_events.push((#model_tag, forgedb_changefeed::ChangeKind::Deleted, __id_bytes, __row, __bytes));
                true
            }

            pub fn #get_fn(&self, id: #id_type) -> Option<#model_name> {
                let __snap = forgedb_storage::Snapshot::new(self.db.#field.row_count);
                self.db.#field.get_at(&__snap, id)
            }

            pub fn #all_fn(&self) -> Vec<#model_name> {
                let __snap = forgedb_storage::Snapshot::new(self.db.#field.row_count);
                self.db.#field.all_at(&__snap)
            }
        }
    }

    fn generate_junction_structs(schema: &Schema) -> TokenStream {
        let mut structs = Vec::new();

        for m in Self::valid_m2m(schema) {
            let struct_ident = Self::junction_struct_ident(&m);
            let reader_ident = format_ident!("{}Reader", struct_ident);
            let (lt, rt) = Self::junction_key_pair(schema, &m);
            let lk = Self::key_type_ident(schema, &lt);
            let rk = Self::key_type_ident(schema, &rt);
            let lwv = Self::junction_key_width(&lt);
            let rwv = Self::junction_key_width(&rt);
            let lw = quote! { #lwv };
            let rw = quote! { #rwv };
            let l_col_ty = Self::storage_column_type_tokens(schema, &lt, false);
            let r_col_ty = Self::storage_column_type_tokens(schema, &rt, false);
            let l_read_local = Self::junction_read_expr(schema, &lt, &quote! { left_col }, &quote! { i });
            let r_read_local = Self::junction_read_expr(schema, &rt, &quote! { right_col }, &quote! { i });
            let l_read_self = Self::junction_read_expr(schema, &lt, &quote! { self.left_col }, &quote! { i });
            let r_read_self = Self::junction_read_expr(schema, &rt, &quote! { self.right_col }, &quote! { i });
            let l_append = Self::junction_append_expr(schema, &lt, &quote! { self.left_col }, &quote! { left });
            let r_append = Self::junction_append_expr(schema, &rt, &quote! { self.right_col }, &quote! { right });
            let sumv = proc_macro2::Literal::usize_unsuffixed(lwv + rwv);
            let sw = quote! { #sumv };
            let l_frame = Self::junction_frame_stmt(&lt, &quote! { __row_bytes }, &quote! { left });
            let r_frame = Self::junction_frame_stmt(&rt, &quote! { __row_bytes }, &quote! { right });
            let field_snake = Self::to_snake_case(&m.model1);
            let field_snake2 = Self::to_snake_case(&m.model2);
            let base = format!("{}_{}_link", field_snake, field_snake2);
            let left_path = format!("{}/fixed/left.bin", base);
            let right_path = format!("{}/fixed/right.bin", base);
            let tombstones_path = format!("{}/tombstones.bin", base);
            let manifest_path = format!("{}/manifest.json", base);

            structs.push(quote! {
                pub struct #struct_ident {
                    left_col: FixedColumn,
                    right_col: FixedColumn,
                    tombstones: Tombstones,
                    row_count: usize,
                    left_index: std::collections::HashMap<#lk, Vec<#rk>>,
                    right_index: std::collections::HashMap<#rk, Vec<#lk>>,
                    changefeed: Option<forgedb_changefeed::ChangeFeed>,
                    broker: Option<std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>>,
                }

                impl #struct_ident {
                    pub fn new() -> Self {
                        Self::new_at(std::path::Path::new("."))
                    }

                    pub fn new_at(root: &std::path::Path) -> Self {
                        let left_col = FixedColumn::new(
                            root.join(#left_path),
                            #lw,
                        ).expect("Failed to create junction column");
                        let right_col = FixedColumn::new(
                            root.join(#right_path),
                            #rw,
                        ).expect("Failed to create junction column");
                        let mut tombstones = Tombstones::new(
                            root.join(#tombstones_path),
                        ).expect("Failed to create junction tombstones");
                        let row_count = right_col.len();
                        while tombstones.len() < row_count {
                            tombstones
                                .append(false)
                                .expect("Failed to back-fill junction tombstone");
                        }
                        let mut left_index: std::collections::HashMap<#lk, Vec<#rk>> =
                            std::collections::HashMap::new();
                        let mut right_index: std::collections::HashMap<#rk, Vec<#lk>> =
                            std::collections::HashMap::new();
                        {
                            let mut __state: std::collections::HashMap<(#lk, #rk), bool> =
                                std::collections::HashMap::new();
                            let mut __order: Vec<(#lk, #rk)> = Vec::new();
                            for i in 0..row_count {
                                let l = #l_read_local;
                                let r = #r_read_local;
                                if !__state.contains_key(&(l, r)) {
                                    __order.push((l, r));
                                }
                                __state.insert((l, r), tombstones.is_deleted(i).unwrap_or(false));
                            }
                            for (l, r) in __order {
                                if !__state.get(&(l, r)).copied().unwrap_or(true) {
                                    left_index.entry(l).or_default().push(r);
                                    right_index.entry(r).or_default().push(l);
                                }
                            }
                        }
                        let db = Self {
                            left_col,
                            right_col,
                            tombstones,
                            row_count,
                            left_index,
                            right_index,
                            changefeed: None,
                            broker: None,
                        };
                        db.write_manifest(root);
                        db
                    }

                    fn __index_add(&mut self, left: #lk, right: #rk) {
                        let rs = self.left_index.entry(left).or_default();
                        if !rs.contains(&right) {
                            rs.push(right);
                        }
                        let ls = self.right_index.entry(right).or_default();
                        if !ls.contains(&left) {
                            ls.push(left);
                        }
                    }

                    fn __index_remove(&mut self, left: #lk, right: #rk) {
                        if let Some(rs) = self.left_index.get_mut(&left) {
                            rs.retain(|r| *r != right);
                            if rs.is_empty() {
                                self.left_index.remove(&left);
                            }
                        }
                        if let Some(ls) = self.right_index.get_mut(&right) {
                            ls.retain(|l| *l != left);
                            if ls.is_empty() {
                                self.right_index.remove(&right);
                            }
                        }
                    }

                    pub fn rights_of(&self, left: #lk) -> Vec<#rk> {
                        self.left_index.get(&left).cloned().unwrap_or_default()
                    }

                    pub fn lefts_of(&self, right: #rk) -> Vec<#lk> {
                        self.right_index.get(&right).cloned().unwrap_or_default()
                    }

                    pub fn attach_changefeed(&mut self, feed: forgedb_changefeed::ChangeFeed) {
                        self.changefeed = Some(feed);
                    }

                    pub fn attach_broker(
                        &mut self,
                        broker: Option<std::sync::Arc<std::sync::Mutex<forgedb_changefeed::durable::DurableBroker>>>,
                    ) {
                        self.broker = broker;
                    }

                    fn write_manifest(&self, root: &std::path::Path) {
                        let columns = vec![
                            forgedb_storage::ColumnMetadata {
                                name: "left".to_string(),
                                column_type: #l_col_ty,
                                column_index: 0usize,
                                value_size: #lw,
                                kind: forgedb_storage::ColumnKind::Fixed,
                                relative_path: "fixed/left.bin".to_string(),
                            },
                            forgedb_storage::ColumnMetadata {
                                name: "right".to_string(),
                                column_type: #r_col_ty,
                                column_index: 1usize,
                                value_size: #rw,
                                kind: forgedb_storage::ColumnKind::Fixed,
                                relative_path: "fixed/right.bin".to_string(),
                            },
                        ];
                        let __manifest_abs = root.join(#manifest_path);
                        let __compaction_epoch = forgedb_storage::Manifest::load_from(&__manifest_abs)
                            .map(|m| m.compaction_epoch)
                            .unwrap_or(0);
                        let __schema_version = forgedb_storage::Manifest::load_from(&__manifest_abs)
                            .map(|m| m.schema_version)
                            .unwrap_or(EXPECTED_SCHEMA_VERSION);
                        let manifest = forgedb_storage::Manifest {
                            row_count: self.row_count,
                            columns,
                            wal_enabled: false,
                    last_checkpoint: self.row_count as u64,
                            compaction_epoch: __compaction_epoch,
                            schema_version: __schema_version,
                            engine_version: EXPECTED_ENGINE_VERSION,
                            row_anchor: Some(forgedb_storage::RowAnchor {
                                relative_path: "fixed/right.bin".to_string(),
                                bytes_per_row: #rw,
                            }),
                            auto_sequences: Default::default(),
                        };
                        let _ = manifest.save_to(&__manifest_abs);
                    }

                    pub fn link(&mut self, left: #lk, right: #rk) {
                        let row_index = self.row_count;
                        #l_append.expect("Failed to append link");
                        #r_append.expect("Failed to append link");
                        self.tombstones.append(false)
                            .expect("Failed to append junction tombstone");
                        self.row_count += 1;
                        self.__index_add(left, right);
                        if let Some(feed) = &self.changefeed {
                            feed.emit(#base, row_index, forgedb_changefeed::ChangeKind::Linked);
                        }
                        if let Some(__broker) = &self.broker {
                            let mut __row_bytes = Vec::with_capacity(#sw);
                            #l_frame
                            #r_frame
                            if let Ok(mut __b) = __broker.lock() {
                                let _ = __b.record(
                                    #base,
                                    row_index as u64,
                                    forgedb_changefeed::ChangeKind::Linked,
                                    __row_bytes,
                                );
                            }
                        }
                    }

                    pub fn unlink(&mut self, left: #lk, right: #rk) -> bool {
                        let __live = self
                            .left_index
                            .get(&left)
                            .map(|rs| rs.contains(&right))
                            .unwrap_or(false);
                        if !__live {
                            return false;
                        }
                        let row_index = self.row_count;
                        #l_append.expect("Failed to append unlink");
                        #r_append.expect("Failed to append unlink");
                        self.tombstones.append(true)
                            .expect("Failed to append junction tombstone");
                        self.row_count += 1;
                        self.__index_remove(left, right);
                        if let Some(feed) = &self.changefeed {
                            feed.emit(#base, row_index, forgedb_changefeed::ChangeKind::Deleted);
                        }
                        if let Some(__broker) = &self.broker {
                            let mut __row_bytes = Vec::with_capacity(#sw);
                            #l_frame
                            #r_frame
                            if let Ok(mut __b) = __broker.lock() {
                                let _ = __b.record(
                                    #base,
                                    row_index as u64,
                                    forgedb_changefeed::ChangeKind::Deleted,
                                    __row_bytes,
                                );
                            }
                        }
                        true
                    }

                    pub fn unlink_all_left(&mut self, id: #lk) {
                        let __targets = self.rights_of(id);
                        for r in __targets {
                            self.unlink(id, r);
                        }
                    }

                    pub fn unlink_all_right(&mut self, id: #rk) {
                        let __targets = self.lefts_of(id);
                        for l in __targets {
                            self.unlink(l, id);
                        }
                    }

                    pub fn checkpoint(&mut self) {
                        self.left_col
                            .sync_to_drive()
                            .expect("Failed to sync junction left column on checkpoint");
                        self.right_col
                            .sync_to_drive()
                            .expect("Failed to sync junction right column on checkpoint");
                        self.tombstones
                            .sync_to_drive()
                            .expect("Failed to sync junction tombstones on checkpoint");
                        self.tombstones
                            .barrier()
                            .expect("Failed to issue junction checkpoint device barrier");
                    }

                    pub fn pairs(&self) -> Vec<(#lk, #rk)> {
                        self.pairs_prefix(self.row_count)
                    }

                    fn pairs_prefix(&self, end: usize) -> Vec<(#lk, #rk)> {
                        let mut order: Vec<(#lk, #rk)> = Vec::new();
                        let mut state: std::collections::HashMap<(#lk, #rk), bool> =
                            std::collections::HashMap::new();
                        for i in 0..end {
                            let left = #l_read_self;
                            let right = #r_read_self;
                            let deleted = self.tombstones.is_deleted(i).unwrap_or(false);
                            if !state.contains_key(&(left, right)) {
                                order.push((left, right));
                            }
                            state.insert((left, right), deleted);
                        }
                        order
                            .into_iter()
                            .filter(|pair| !state.get(pair).copied().unwrap_or(true))
                            .collect()
                    }

                    pub fn row_count(&self) -> usize {
                        self.row_count
                    }

                    pub fn snapshot(&self) -> forgedb_storage::Snapshot {
                        forgedb_storage::Snapshot::new(self.row_count)
                    }

                    pub fn pairs_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<(#lk, #rk)> {
                        self.pairs_prefix(snap.watermark())
                    }

                    pub fn reader(&self) -> #reader_ident {
                        #reader_ident {
                            left_col: self.left_col.reader()
                                .expect("Failed to open junction left reader"),
                            right_col: self.right_col.reader()
                                .expect("Failed to open junction right reader"),
                            tombstones: self.tombstones.reader()
                                .expect("Failed to open junction tombstones reader"),
                        }
                    }
                }

                pub struct #reader_ident {
                    left_col: forgedb_storage::FixedColumnReader,
                    right_col: forgedb_storage::FixedColumnReader,
                    tombstones: forgedb_storage::TombstonesReader,
                }

                impl #reader_ident {
                    pub fn pairs_at(&self, snap: &forgedb_storage::Snapshot) -> Vec<(#lk, #rk)> {
                        let end = snap.watermark();
                        let mut order: Vec<(#lk, #rk)> = Vec::new();
                        let mut state: std::collections::HashMap<(#lk, #rk), bool> =
                            std::collections::HashMap::new();
                        for i in 0..end {
                            let left = #l_read_self;
                            let right = #r_read_self;
                            let deleted = self.tombstones.is_deleted(i).unwrap_or(false);
                            if !state.contains_key(&(left, right)) {
                                order.push((left, right));
                            }
                            state.insert((left, right), deleted);
                        }
                        order
                            .into_iter()
                            .filter(|pair| !state.get(pair).copied().unwrap_or(true))
                            .collect()
                    }
                }
            });
        }

        quote! { #(#structs)* }
    }

    fn generate_traversal_impl(schema: &Schema) -> TokenStream {
        use std::collections::{HashMap, HashSet};

        let mut methods = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for model in &schema.models {
            let model_snake = Self::to_snake_case(&model.name);
            for field in &model.fields {
                let (target_name, optional) = match &field.field_type {
                    forgedb_parser::FieldType::Relation(
                        forgedb_parser::RelationType::RequiredReference(t),
                    ) => (t, false),
                    forgedb_parser::FieldType::Relation(
                        forgedb_parser::RelationType::OptionalReference(t),
                    ) => (t, true),
                    _ => continue,
                };
                let Some(target) = schema.find_model(target_name) else {
                    continue;
                };

                let method_name = format!("{}_{}", model_snake, field.name);
                if !seen.insert(method_name.clone()) {
                    continue;
                }
                let method_ident = format_ident!("{}", method_name);
                let model_ident = format_ident!("{}", model.name);
                let target_ident = format_ident!("{}", target.name);
                let target_field = format_ident!("{}", Self::to_snake_case(&target.name));
                let fk_field = format_ident!("{}", field.name);

                if optional {
                    methods.push(quote! {
                        pub fn #method_ident(&self, record: &#model_ident) -> Option<#target_ident> {
                            record.#fk_field.and_then(|fk| self.#target_field.get(fk))
                        }
                    });
                } else {
                    methods.push(quote! {
                        pub fn #method_ident(&self, record: &#model_ident) -> Option<#target_ident> {
                            self.#target_field.get(record.#fk_field)
                        }
                    });
                }
            }
        }

        let pairs = schema.detect_relations();
        let mut group_counts: HashMap<(String, String), usize> = HashMap::new();
        for p in &pairs {
            *group_counts
                .entry((p.parent_model.clone(), p.parent_field.clone()))
                .or_default() += 1;
        }
        for p in &pairs {
            let Some(parent) = schema.find_model(&p.parent_model) else {
                continue;
            };
            let parent_id_type = Self::id_type_tokens(schema, parent);

            let ambiguous = group_counts
                .get(&(p.parent_model.clone(), p.parent_field.clone()))
                .is_some_and(|&c| c > 1);
            let method_name = if ambiguous {
                format!(
                    "{}_{}_by_{}",
                    Self::to_snake_case(&p.parent_model),
                    p.parent_field,
                    p.child_field
                )
            } else {
                format!("{}_{}", Self::to_snake_case(&p.parent_model), p.parent_field)
            };
            if !seen.insert(method_name.clone()) {
                continue;
            }
            let method_ident = format_ident!("{}", method_name);
            let child_ident = format_ident!("{}", p.child_model);
            let child_field = format_ident!("{}", Self::to_snake_case(&p.child_model));
            let fk_probe = format_ident!("find_by_{}", p.child_field);

            let child_indexed = schema
                .find_model(&p.child_model)
                .is_some_and(|c| c.has_identity());


            let body = if child_indexed {
                let arg = Self::fk_probe_arg(schema, &p.parent_model, p.is_required);
                quote! { self.#child_field.#fk_probe(#arg) }
            } else {
                let fk_field = format_ident!("{}", p.child_field);
                let predicate = if p.is_required {
                    quote! { record.#fk_field == id }
                } else {
                    quote! { record.#fk_field == Some(id) }
                };
                quote! {
                    self.#child_field
                        .all()
                        .into_iter()
                        .filter(|record| #predicate)
                        .collect()
                }
            };

            methods.push(quote! {
                pub fn #method_ident(&self, id: #parent_id_type) -> Vec<#child_ident> {
                    #body
                }
            });
        }

        for m in Self::valid_m2m(schema) {
            let junction_field = Self::junction_field_ident(&m);
            let model1_ident = format_ident!("{}", m.model1);
            let model2_ident = format_ident!("{}", m.model2);
            let model1_storage = format_ident!("{}", Self::to_snake_case(&m.model1));
            let model2_storage = format_ident!("{}", Self::to_snake_case(&m.model2));
            let (lt, rt) = Self::junction_key_pair(schema, &m);
            let lk = Self::key_type_ident(schema, &lt);
            let rk = Self::key_type_ident(schema, &rt);

            let link_name = format!(
                "link_{}_{}",
                Self::to_snake_case(&m.model1),
                Self::to_snake_case(&m.model2)
            );
            if seen.insert(link_name.clone()) {
                let link_ident = format_ident!("{}", link_name);
                methods.push(quote! {
                    pub fn #link_ident(&mut self, left: #lk, right: #rk) {
                        self.#junction_field.link(left, right);
                    }
                });
            }

            let unlink_name = format!(
                "unlink_{}_{}",
                Self::to_snake_case(&m.model1),
                Self::to_snake_case(&m.model2)
            );
            if seen.insert(unlink_name.clone()) {
                let unlink_ident = format_ident!("{}", unlink_name);
                let junction_field_u = Self::junction_field_ident(&m);
                methods.push(quote! {
                    pub fn #unlink_ident(&mut self, left: #lk, right: #rk) -> bool {
                        self.#junction_field_u.unlink(left, right)
                    }
                });
            }

            let fwd_name = format!("{}_{}", Self::to_snake_case(&m.model1), m.field1);
            if seen.insert(fwd_name.clone()) {
                let fwd_ident = format_ident!("{}", fwd_name);
                methods.push(quote! {
                    pub fn #fwd_ident(&self, id: #lk) -> Vec<#model2_ident> {
                        self.#junction_field
                            .rights_of(id)
                            .into_iter()
                            .filter_map(|right| self.#model2_storage.get(right))
                            .collect()
                    }
                });

                let fwd_at_name = format!(
                    "{}_{}_at",
                    Self::to_snake_case(&m.model1),
                    m.field1
                );
                if seen.insert(fwd_at_name.clone()) {
                    let fwd_at_ident = format_ident!("{}", fwd_at_name);
                    let junction_field2 = junction_field.clone();
                    methods.push(quote! {
                        pub fn #fwd_at_ident(
                            &self,
                            snap: &DatabaseSnapshot,
                            id: #lk,
                        ) -> Vec<#model2_ident> {
                            self.#junction_field2
                                .pairs_at(&snap.#junction_field2)
                                .into_iter()
                                .filter(|(left, _)| *left == id)
                                .filter_map(|(_, right)| {
                                    self.#model2_storage.get_at(&snap.#model2_storage, right)
                                })
                                .collect()
                        }
                    });
                }
            }

            let rev_name = format!("{}_{}", Self::to_snake_case(&m.model2), m.field2);
            if seen.insert(rev_name.clone()) {
                let rev_ident = format_ident!("{}", rev_name);
                methods.push(quote! {
                    pub fn #rev_ident(&self, id: #rk) -> Vec<#model1_ident> {
                        self.#junction_field
                            .lefts_of(id)
                            .into_iter()
                            .filter_map(|left| self.#model1_storage.get(left))
                            .collect()
                    }
                });
            }
        }

        if methods.is_empty() {
            return quote! {};
        }

        quote! {
            impl Database {
                #(#methods)*
            }
        }
    }

    fn generate_reader_traversal_impl(schema: &Schema) -> TokenStream {
        let mut methods = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        for m in Self::valid_m2m(schema) {
            let junction_field = Self::junction_field_ident(&m);
            let model2_ident = format_ident!("{}", m.model2);
            let model2_storage = format_ident!("{}", Self::to_snake_case(&m.model2));
            let (lt, _) = Self::junction_key_pair(schema, &m);
            let lk = Self::key_type_ident(schema, &lt);

            let fwd_at_name = format!("{}_{}_at", Self::to_snake_case(&m.model1), m.field1);
            if seen.insert(fwd_at_name.clone()) {
                let fwd_at_ident = format_ident!("{}", fwd_at_name);
                methods.push(quote! {
                    pub fn #fwd_at_ident(
                        &self,
                        snap: &DatabaseSnapshot,
                        id: #lk,
                    ) -> Vec<#model2_ident> {
                        self.#junction_field
                            .pairs_at(&snap.#junction_field)
                            .into_iter()
                            .filter(|(left, _)| *left == id)
                            .filter_map(|(_, right)| {
                                self.#model2_storage.get_at(&snap.#model2_storage, right)
                            })
                            .collect()
                    }
                });
            }
        }

        if methods.is_empty() {
            return quote! {};
        }

        quote! {
            impl DatabaseReader {
                #(#methods)*
            }
        }
    }

    fn generate_eager_load(schema: &Schema) -> TokenStream {
        let mut items = Vec::new();

        for model in &schema.models {
            let fks: Vec<(&forgedb_parser::Field, &str, bool)> = model
                .fields
                .iter()
                .filter_map(|field| match &field.field_type {
                    forgedb_parser::FieldType::Relation(
                        forgedb_parser::RelationType::RequiredReference(t),
                    ) => Some((field, t.as_str(), false)),
                    forgedb_parser::FieldType::Relation(
                        forgedb_parser::RelationType::OptionalReference(t),
                    ) => Some((field, t.as_str(), true)),
                    _ => None,
                })
                .filter(|(_, target, _)| schema.find_model(target).is_some())
                .collect();

            if fks.is_empty() {
                continue;
            }

            let model_ident = format_ident!("{}", model.name);
            let base_field = Self::to_snake_case(&model.name);
            let base_ident = format_ident!("{}", base_field);
            let struct_ident = format_ident!("{}WithRelations", model.name);
            let getter_ident = format_ident!("{}_with_relations", base_field);
            let id_type = Self::id_type_tokens(schema, model);

            let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
            used.insert(base_field.clone());

            let mut struct_fields = Vec::new();
            let mut resolvers = Vec::new();
            let mut resolved_idents = Vec::new();

            for (field, target, optional) in fks {
                let stripped = field
                    .name
                    .strip_suffix("_id")
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&field.name)
                    .to_string();
                let resolved_name = if used.contains(&stripped) {
                    field.name.clone()
                } else {
                    stripped
                };
                if !used.insert(resolved_name.clone()) {
                    continue;
                }

                let resolved_ident = format_ident!("{}", resolved_name);
                let target_ident = format_ident!("{}", target);
                let target_storage = format_ident!("{}", Self::to_snake_case(target));
                let fk_ident = format_ident!("{}", field.name);

                struct_fields.push(quote! {
                    pub #resolved_ident: Option<#target_ident>
                });

                if optional {
                    resolvers.push(quote! {
                        let #resolved_ident = #base_ident.#fk_ident
                            .and_then(|fk| self.#target_storage.get(fk));
                    });
                } else {
                    resolvers.push(quote! {
                        let #resolved_ident = self.#target_storage.get(#base_ident.#fk_ident);
                    });
                }
                resolved_idents.push(resolved_ident);
            }

            let to_schema_derive = Self::to_schema_derive();
            items.push(quote! {
                #[derive(Debug, Clone, Serialize, Deserialize #to_schema_derive)]
                pub struct #struct_ident {
                    pub #base_ident: #model_ident,
                    #(#struct_fields,)*
                }

                impl Database {
                    pub fn #getter_ident(&self, id: #id_type) -> Option<#struct_ident> {
                        let #base_ident = self.#base_ident.get(id)?;
                        #(#resolvers)*
                        Some(#struct_ident { #base_ident, #(#resolved_idents),* })
                    }
                }
            });
        }

        quote! { #(#items)* }
    }

    pub(crate) fn key_type_ident(
        schema: &Schema,
        field_type: &forgedb_parser::FieldType,
    ) -> TokenStream {
        match field_type {
            forgedb_parser::FieldType::StringN { chars, .. } => {
                let bytes = *chars as usize;
                quote! { InlineStr<#bytes> }
            }
            forgedb_parser::FieldType::Nullable(inner) => Self::key_type_ident(schema, inner),
            other => Self::map_field_type_ident(schema, other),
        }
    }

    fn map_field_type_ident(schema: &Schema, field_type: &forgedb_parser::FieldType) -> TokenStream {
        if Self::fk_backing_type(schema, field_type).is_some() {
            return Self::key_type_ident(schema, &Self::resolved_type(schema, field_type));
        }
        let field_type = &Self::resolved_type(schema, field_type);
        match field_type {
            forgedb_parser::FieldType::U32 => quote! { u32 },
            forgedb_parser::FieldType::U64 => quote! { u64 },
            forgedb_parser::FieldType::I32 => quote! { i32 },
            forgedb_parser::FieldType::I64 => quote! { i64 },
            forgedb_parser::FieldType::F64 => quote! { f64 },
            forgedb_parser::FieldType::Bool => quote! { bool },
            forgedb_parser::FieldType::String
            | forgedb_parser::FieldType::StringN { .. } => quote! { String },
            forgedb_parser::FieldType::Json => quote! { serde_json::Value },
            forgedb_parser::FieldType::Decimal => quote! { rust_decimal::Decimal },
            forgedb_parser::FieldType::Enum(name) => {
                let ident = format_ident!("{}", name);
                quote! { #ident }
            }
            forgedb_parser::FieldType::Uuid => quote! { Uuid },
            forgedb_parser::FieldType::Timestamp(_) => quote! { Timestamp },
            forgedb_parser::FieldType::StructType(name) => {
                let ident = format_ident!("{}", name);
                quote! { #ident }
            }
            forgedb_parser::FieldType::OptionalStructType(name) => {
                let ident = format_ident!("{}", name);
                quote! { #ident }
            }
            forgedb_parser::FieldType::Nullable(inner) => {
                Self::map_field_type_ident(schema, inner)
            }
            forgedb_parser::FieldType::Bytes(size) => {
                quote! { [u8; #size] }
            }
            forgedb_parser::FieldType::FixedArray(inner, count) => {
                let inner_type = Self::map_field_type_ident(schema, inner);
                quote! { [#inner_type; #count] }
            }
            forgedb_parser::FieldType::Relation(_) => quote! { () },
            _ => quote! { String },
        }
    }

    pub(crate) fn to_snake_case(s: &str) -> String {
        let mut result = String::new();
        for (i, c) in s.chars().enumerate() {
            if c.is_uppercase() && i > 0 {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        }
        result
    }

    fn generate_coordinated_client(schema: &Schema) -> TokenStream {
        let (_, seq_init_coord, seq_ws_coord) = Self::sequence_claim_plumbing(
            schema,
            &quote! { __tx },
            &quote! { __ws_keys },
            false,
        );
        let seq_fastforward = Self::generate_sequence_fastforward(schema);
        let seq_outcome_binding = if Self::schema_has_bare_integer_auto(schema) {
            quote! { __outcome }
        } else {
            quote! { _ }
        };
        let tx_models: Vec<&forgedb_parser::Model> = schema
            .models
            .iter()
            .filter(|m| m.has_identity())
            .collect();
        if tx_models.is_empty() {
            return quote! {};
        }

        let peer_refresh_arms: Vec<_> = tx_models
            .iter()
            .map(|m| {
                let field = format_ident!("{}", Self::to_snake_case(&m.name));
                quote! {
                    let __from = self.#field.row_count;
                    self.#field.__sync_columns_from_disk();
                    self.#field.__reindex_delta(__from);
                }
            })
            .collect();

        quote! {
            #[cfg(not(target_arch = "wasm32"))]
            pub struct CoordinatedDatabase {
                shared: SharedDatabase,
                coordinator: std::sync::Arc<forgedb_coordinator::client::CoordinatorClient>,
                last_refreshed_lsn: std::sync::atomic::AtomicU64,
            }

            #[cfg(not(target_arch = "wasm32"))]
            impl CoordinatedDatabase {
                pub fn transaction_coordinated<T>(
                    &self,
                    retries: u32,
                    f: impl Fn(&mut ConcurrentTxHandle) -> Result<T, TxError>,
                ) -> Result<T, TxError> {
                    use forgedb_changefeed::ChangeKind as __CK;
                    use std::sync::atomic::Ordering;
                    let __coord = std::sync::Arc::clone(&self.coordinator);
                    let __seq_arc = std::sync::Arc::clone(&self.shared.seq);
                    let __inner = std::sync::Arc::clone(&self.shared.inner);

                    let mut __last_lsn = __coord.last_known_lsn();

                    for __attempt in 0..=retries {
                        {
                            let __coord_lsn = __coord.last_known_lsn();
                            let __refreshed = self.last_refreshed_lsn.load(Ordering::Acquire);
                            if __coord_lsn > __refreshed {
                                let mut __db = __inner.write().unwrap();
                                __db.__peer_refresh();
                                self.last_refreshed_lsn.store(__coord_lsn, Ordering::Release);
                            }
                        }

                        let __snap_lsn = {
                            let mut __seq = __seq_arc.lock().unwrap();
                            __seq.register_snapshot()
                        };

                        let __snap = {
                            let __db = __inner.read().unwrap();
                            __db.snapshot()
                        };

                        let mut __tx = ConcurrentTxHandle {
                            inner: std::sync::Arc::clone(&__inner),
                            snap: __snap,
                            buffer: Vec::new(),
                            staged_unique_keys: std::collections::BTreeSet::new(),
                            #seq_init_coord
                            pending_events: Vec::new(),
                        };
                        let __out = f(&mut __tx);
                        let __val = match __out {
                            Ok(v) => v,
                            Err(e) => {
                                let mut __seq = __seq_arc.lock().unwrap();
                                __seq.release_snapshot(__snap_lsn);
                                return Err(e);
                            }
                        };

                        let mut __ws_keys: Vec<Vec<u8>> = Vec::new();
                        for (__model, _kind, __id_bytes, _bytes, _del) in &__tx.buffer {
                            __ws_keys.push(__forgedb_ws_key(&[
                                b"r",
                                __model.as_bytes(),
                                __id_bytes,
                            ]));
                        }
                        for (__mtag, __fname, __ekey) in &__tx.staged_unique_keys {
                            __ws_keys.push(__forgedb_ws_key(&[
                                b"u",
                                __mtag.as_bytes(),
                                __fname.as_bytes(),
                                __ekey.as_bytes(),
                            ]));
                        }
                        #seq_ws_coord

                        let __turn = {
                            let mut __busy_retries = 0u32;
                            loop {
                                match __coord.request_turn(__ws_keys.clone(), __last_lsn) {
                                    Ok(grant) => break Ok(grant),
                                    Err(forgedb_coordinator::client::ClientError::Conflict { conflict_key }) => {
                                        break Err(forgedb_txn::CommitOutcome::Conflict {
                                            key: conflict_key.into_boxed_slice(),
                                        });
                                    }
                                    Err(forgedb_coordinator::client::ClientError::Busy) => {
                                        if __busy_retries >= 5 {
                                            break Err(forgedb_txn::CommitOutcome::Conflict {
                                                key: b"__busy__".to_vec().into_boxed_slice(),
                                            });
                                        }
                                        __busy_retries += 1;
                                        std::thread::sleep(std::time::Duration::from_millis(20 * __busy_retries as u64));
                                    }
                                    Err(e) => {
                                        let _ = __coord.reconnect();
                                        let mut __seq = __seq_arc.lock().unwrap();
                                        __seq.release_snapshot(__snap_lsn);
                                        return Err(TxError::Io(e.to_string()));
                                    }
                                }
                            }
                        };

                        match __turn {
                            Err(#seq_outcome_binding) => {
                                #seq_fastforward
                                let mut __seq = __seq_arc.lock().unwrap();
                                __seq.release_snapshot(__snap_lsn);
                                __last_lsn = __coord.last_known_lsn();
                                continue;
                            }
                            Ok((__turn_id, _reserved_lsn)) => {
                                let __kinds_copy: Vec<u8> = __tx.buffer
                                    .iter()
                                    .map(|(_, k, _, _, _)| k.to_byte())
                                    .collect();
                                let __bytes_copy: Vec<Vec<u8>> = __tx.buffer
                                    .iter()
                                    .map(|(_, _, _, b, _)| b.clone())
                                    .collect();

                                let __apply_result = {
                                    let mut __db = __inner.write().unwrap();
                                    __db.__apply_and_commit_concurrent_buffer(
                                        __tx.buffer,
                                        __tx.pending_events,
                                    )
                                };

                                let (__model_tags, __row_indices) = match &__apply_result {
                                    Ok(__pairs) => {
                                        let tags: Vec<Vec<u8>> = __pairs.iter().map(|(t, _)| t.clone()).collect();
                                        let rows: Vec<u64> = __pairs.iter().map(|(_, r)| *r).collect();
                                        (tags, rows)
                                    }
                                    Err(_) => {
                                        (Vec::new(), Vec::new())
                                    }
                                };
                                let __ack = __coord.committed(
                                    __turn_id,
                                    __model_tags,
                                    __row_indices,
                                    __kinds_copy,
                                    __bytes_copy,
                                );
                                match __ack {
                                    Ok(__lsn) => { __last_lsn = __lsn; }
                                    Err(e) => {
                                        eprintln!("coordinator: Committed ack error: {e}");
                                        let _ = __coord.reconnect();
                                    }
                                }

                                let mut __seq = __seq_arc.lock().unwrap();
                                __seq.release_snapshot(__snap_lsn);

                                return __apply_result.map(|_| __val);
                            }
                        }
                    }
                    Err(TxError::Conflict)
                }
            }

            #[cfg(not(target_arch = "wasm32"))]
            impl Database {
                pub(crate) fn __peer_refresh(&mut self) {
                    #(#peer_refresh_arms)*
                }

                pub fn connect(
                    root: std::path::PathBuf,
                    socket_path: std::path::PathBuf,
                ) -> Result<CoordinatedDatabase, TxError> {
                    let coordinator = forgedb_coordinator::client::CoordinatorClient::connect(&socket_path)
                        .map_err(|e| TxError::CoordinatorUnavailable(e.to_string()))?;
                    let shared = Self::__open_with_lock(root, None).shared();
                    Ok(CoordinatedDatabase {
                        shared,
                        coordinator: std::sync::Arc::new(coordinator),
                        last_refreshed_lsn: std::sync::atomic::AtomicU64::new(0),
                    })
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgedb_parser::ast::IndexType;
    use forgedb_parser::{Field, FieldType, Model, Schema};

    fn probe_field(field_type: FieldType) -> Field {
        Field {
            position: None,
            name: "probe".to_string(),
            field_type,
            auto_generate: false,
            unique: false,
            indexed: true,
            constraints: vec![],
            index_type: IndexType::Hash,
            is_computed: false,
            fulltext_indexed: false,
            is_materialized: false,
        }
    }

    #[test]
    fn ordered_key_type_agrees_with_parser_range_support() {
        let types = [
            FieldType::U32,
            FieldType::U64,
            FieldType::I32,
            FieldType::I64,
            FieldType::F64,
            FieldType::Timestamp(forgedb_parser::TimestampPrecision::Millis),
            FieldType::Decimal,
            FieldType::String,
            FieldType::Bool,
            FieldType::Uuid,
            FieldType::Json,
            FieldType::Bytes(8),
            FieldType::Enum("Status".to_string()),
        ];
        for ty in types {
            let generated = RustGenerator::ordered_key_type(&probe_field(ty.clone())).is_some();
            let claimed = ty.supports_range_queries();
            assert_eq!(
                generated, claimed,
                "{ty:?}: codegen emits an ordered index = {generated}, but \
                 supports_range_queries() claims {claimed}"
            );
        }
    }

    #[test]
    fn numeric_bound_operands_cover_every_numeric_type() {
        let types = [
            FieldType::U32,
            FieldType::U64,
            FieldType::I32,
            FieldType::I64,
            FieldType::F64,
            FieldType::Decimal,
            FieldType::Timestamp(forgedb_parser::TimestampPrecision::Millis),
            FieldType::String,
            FieldType::Bool,
            FieldType::Uuid,
            FieldType::Json,
            FieldType::Bytes(8),
            FieldType::Enum("Status".to_string()),
        ];
        for ty in types {
            let gated = RustGenerator::is_numeric_type(&ty);
            let comparable =
                RustGenerator::numeric_bound_operands(&ty, &BoundLiteral::Int(1)).is_some();
            assert_eq!(
                gated, comparable,
                "{ty:?}: is_numeric_type says {gated}, but numeric_bound_operands \
                 says {comparable} — a `true`/`None` pair emits no check at all"
            );
        }
    }

    #[test]
    fn nullable_numeric_fields_still_get_bound_operands() {
        for inner in [FieldType::U64, FieldType::F64, FieldType::Decimal] {
            let ty = FieldType::Nullable(Box::new(inner.clone()));
            assert!(
                RustGenerator::is_numeric_type(&ty),
                "nullable {inner:?} is numeric"
            );
            assert!(
                RustGenerator::numeric_bound_operands(&ty, &BoundLiteral::Int(1)).is_some(),
                "nullable {inner:?} must compare in the inner domain"
            );
        }
    }

    #[test]
    fn each_numeric_domain_compares_without_a_lossy_cast() {
        let rhs = |ty: FieldType| {
            RustGenerator::numeric_bound_operands(&ty, &BoundLiteral::Int(7))
                .unwrap()
                .1
                .to_string()
                .replace(' ', "")
        };
        assert_eq!(rhs(FieldType::U64), "(7i64asi128)");
        assert_eq!(rhs(FieldType::I64), "(7i64asi128)");
        assert_eq!(rhs(FieldType::F64), "(7i64asf64)");
        assert_eq!(rhs(FieldType::Decimal), "rust_decimal::Decimal::from(7i64)");
    }

    #[test]
    fn nullable_fields_are_never_ordered_eligible() {
        for inner in [FieldType::U32, FieldType::F64, FieldType::Decimal] {
            let field = probe_field(FieldType::Nullable(Box::new(inner.clone())));
            assert!(
                RustGenerator::ordered_key_type(&field).is_none(),
                "nullable {inner:?} must not get an ordered index"
            );
        }
    }

    #[test]
    fn f64_ordered_param_type_stays_f64_while_its_key_is_u64() {
        let f = probe_field(FieldType::F64);
        assert_eq!(
            RustGenerator::ordered_key_type(&f).unwrap().to_string(),
            "u64"
        );
        assert_eq!(
            RustGenerator::ordered_param_type(&f).unwrap().to_string(),
            "f64"
        );
        for ty in [FieldType::U32, FieldType::I64, FieldType::Decimal] {
            let f = probe_field(ty.clone());
            assert_eq!(
                RustGenerator::ordered_key_type(&f).unwrap().to_string(),
                RustGenerator::ordered_param_type(&f).unwrap().to_string(),
                "{ty:?}: key and param types must coincide"
            );
        }
    }

    #[test]
    fn test_rust_generation_with_quote() {
        let schema = Schema {
            models: vec![Model { position: None,
                name: "User".to_string(),
                fields: vec![
                    Field { position: None,
                        name: "id".to_string(),
                        field_type: FieldType::Uuid,
                        auto_generate: true,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                    Field { position: None,
                        name: "email".to_string(),
                        field_type: FieldType::String,
                        auto_generate: false,
                        unique: true,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                    Field { position: None,
                        name: "age".to_string(),
                        field_type: FieldType::OptionalStructType("Age".to_string()),
                        auto_generate: false,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    },
                ],
                composite_indexes: vec![],
                projections: Vec::new(),
                soft_delete: false,
            }],
            structs: vec![],
            enums: vec![],
        };

        let result = RustGenerator::generate(&schema).unwrap();
        let code = result.code;

        assert!(code.contains("pub struct User"));
        assert!(code.contains("pub id: Uuid"));
        assert!(code.contains("pub email: String"));
        assert!(code.contains("pub age: Option<Age>"));
        assert!(code.contains("pub struct UserStorage"));
        assert!(code.contains("pub struct Database"));
        assert!(code.contains("pub user: UserStorage"));
        assert!(code.contains("impl UserStorage"));
        assert!(code.contains("pub fn new"));
        assert!(code.contains("pub fn insert"));
        assert!(code.contains("pub fn get"));

        assert!(code.contains("use std::collections::HashMap"));
        assert!(code.contains("use forgedb_types::{Uuid, Timestamp, Value}"));
    }

    #[test]
    fn test_multiple_models() {
        let schema = Schema {
            models: vec![
                Model { position: None,
                    name: "User".to_string(),
                    fields: vec![Field { position: None,
                        name: "id".to_string(),
                        field_type: FieldType::Uuid,
                        auto_generate: true,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    }],
                    composite_indexes: vec![],
                    projections: Vec::new(),
                    soft_delete: false,
                },
                Model { position: None,
                    name: "Post".to_string(),
                    fields: vec![Field { position: None,
                        name: "id".to_string(),
                        field_type: FieldType::Uuid,
                        auto_generate: true,
                        unique: false,
                        indexed: false,
                        constraints: vec![],
                        index_type: IndexType::Hash,
                        is_computed: false,
                        fulltext_indexed: false,
                        is_materialized: false,
                    }],
                    composite_indexes: vec![],
                    projections: Vec::new(),
                    soft_delete: false,
                },
            ],
            structs: vec![],
            enums: vec![],
        };

        let result = RustGenerator::generate(&schema).unwrap();
        let code = result.code;

        assert!(code.contains("pub struct User"));
        assert!(code.contains("pub struct Post"));
        assert!(code.contains("pub struct UserStorage"));
        assert!(code.contains("pub struct PostStorage"));
        assert!(code.contains("pub user: UserStorage"));
        assert!(code.contains("pub post: PostStorage"));
    }

    fn keyed(name: &str, ft: FieldType) -> Model {
        Model {
            position: None,
            name: name.to_string(),
            fields: vec![Field {
                position: None,
                name: "id".to_string(),
                field_type: ft,
                auto_generate: false,
                unique: false,
                indexed: false,
                constraints: vec![],
                index_type: IndexType::Hash,
                is_computed: false,
                fulltext_indexed: false,
                is_materialized: false,
            }],
            composite_indexes: vec![],
            projections: Vec::new(),
            soft_delete: false,
        }
    }

    fn schema_of(models: Vec<Model>) -> Schema {
        Schema {
            models,
            structs: vec![],
            enums: vec![],
        }
    }

    #[test]
    fn fk_backing_type_is_none_for_an_identity_less_target() {
        let ghost = Model {
            position: None,
            name: "Ghost".to_string(),
            fields: vec![],
            composite_indexes: vec![],
            projections: Vec::new(),
            soft_delete: false,
        };
        let schema = schema_of(vec![ghost]);
        let fk = FieldType::Relation(forgedb_parser::RelationType::RequiredReference(
            "Ghost".to_string(),
        ));
        assert_eq!(
            RustGenerator::fk_backing_type(&schema, &fk),
            None,
            "an identity-less target resolves to nothing, never to Uuid by default"
        );

        let missing = FieldType::Relation(forgedb_parser::RelationType::RequiredReference(
            "Nope".to_string(),
        ));
        assert_eq!(RustGenerator::fk_backing_type(&schema, &missing), None);
    }

    #[test]
    fn fk_backing_type_resolves_through_a_chain() {
        let schema = schema_of(vec![
            keyed("Customer", FieldType::Uuid),
            keyed(
                "Order",
                FieldType::Relation(forgedb_parser::RelationType::RequiredReference(
                    "Customer".into(),
                )),
            ),
        ]);
        let fk = FieldType::Relation(forgedb_parser::RelationType::RequiredReference(
            "Order".to_string(),
        ));
        assert_eq!(
            RustGenerator::fk_backing_type(&schema, &fk),
            Some(FieldType::Uuid)
        );

        let opt = FieldType::Relation(forgedb_parser::RelationType::OptionalReference(
            "Order".to_string(),
        ));
        assert_eq!(
            RustGenerator::fk_backing_type(&schema, &opt),
            Some(FieldType::Nullable(Box::new(FieldType::Uuid)))
        );
    }

    #[test]
    fn fk_backing_type_is_depth_bounded_against_an_identity_cycle() {
        let schema = schema_of(vec![
            keyed(
                "Left",
                FieldType::Relation(forgedb_parser::RelationType::RequiredReference(
                    "Right".into(),
                )),
            ),
            keyed(
                "Right",
                FieldType::Relation(forgedb_parser::RelationType::RequiredReference(
                    "Left".into(),
                )),
            ),
        ]);
        let fk = FieldType::Relation(forgedb_parser::RelationType::RequiredReference(
            "Left".to_string(),
        ));
        assert_eq!(
            RustGenerator::fk_backing_type(&schema, &fk),
            None,
            "a cycle resolves to nothing — it must not recurse forever"
        );

        assert!(RustGenerator::generate(&schema).is_ok());
    }

    #[test]
    fn a_self_referential_fk_is_not_an_identity_cycle() {
        let mut category = keyed("Category", FieldType::Uuid);
        category.fields.push(Field {
            position: None,
            name: "parent".to_string(),
            field_type: FieldType::Relation(forgedb_parser::RelationType::OptionalReference(
                "Category".into(),
            )),
            auto_generate: false,
            unique: false,
            indexed: false,
            constraints: vec![],
            index_type: IndexType::Hash,
            is_computed: false,
            fulltext_indexed: false,
            is_materialized: false,
        });
        let schema = schema_of(vec![category]);
        let fk = FieldType::Relation(forgedb_parser::RelationType::OptionalReference(
            "Category".to_string(),
        ));
        assert_eq!(
            RustGenerator::fk_backing_type(&schema, &fk),
            Some(FieldType::Nullable(Box::new(FieldType::Uuid)))
        );
    }
}
