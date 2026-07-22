use crate::ast::{
    ComponentProtocol, ComponentReference, CompositeIndex, Constraint, ConstraintParam, EnumDef,
    Field, FieldType, Model, Projection, RelationInclusion, RelationType, Schema, Struct,
};
use crate::lexer::{Lexer, Token, TokenWithPos};
use forgedb_validation::Position;

pub struct Parser {
    tokens: Vec<Token>,
    tokens_with_pos: Vec<TokenWithPos>,
    position: usize,
    use_validation: bool,
}

impl Parser {
    pub fn new(input: &str) -> Result<Self, String> {
        Self::new_with_validation(input, true)
    }

    pub fn new_with_validation(input: &str, use_validation: bool) -> Result<Self, String> {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.tokenize()?;

        let mut lexer = Lexer::new(input);
        let tokens_with_pos = lexer.tokenize_with_pos()?;

        Ok(Parser {
            tokens,
            tokens_with_pos,
            position: 0,
            use_validation,
        })
    }

    fn get_current_position(&self) -> Option<Position> {
        if self.position < self.tokens_with_pos.len() {
            Some(self.tokens_with_pos[self.position].position)
        } else {
            None
        }
    }

    fn current_token(&self) -> &Token {
        if self.position < self.tokens.len() {
            &self.tokens[self.position]
        } else {
            &Token::Eof
        }
    }

    fn advance(&mut self) {
        if self.position < self.tokens.len() {
            self.position += 1;
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.current_token(), Token::Newline) {
            self.advance();
        }
    }

    fn read_component_path(&mut self) -> Result<String, String> {
        // Read a path like: components/user/card
        // This is a sequence of identifiers separated by slashes
        let mut path = String::new();

        loop {
            match self.current_token() {
                Token::Ident(name) => {
                    path.push_str(name);
                    self.advance();

                    // Check if there's a slash for more path segments
                    if matches!(self.current_token(), Token::Slash) {
                        path.push('/');
                        self.advance();
                        // Continue to read next segment
                    } else {
                        // End of path
                        break;
                    }
                }
                _ => {
                    return Err(format!(
                        "Expected path component (identifier), found {:?}",
                        self.current_token()
                    ))
                }
            }
        }

        if path.is_empty() {
            return Err("Component path cannot be empty".to_string());
        }

        Ok(path)
    }

    fn expect(&mut self, expected: Token) -> Result<(), String> {
        if self.current_token() == &expected {
            self.advance();
            Ok(())
        } else {
            Err(format!(
                "Expected {:?}, found {:?}",
                expected,
                self.current_token()
            ))
        }
    }

    fn parse_constraint(&mut self) -> Result<Constraint, String> {
        // Expect @
        self.expect(Token::At)?;

        // Parse constraint name
        let name = match self.current_token() {
            Token::Ident(s) => s.clone(),
            _ => {
                return Err(format!(
                    "Expected constraint name after '@', found {:?}",
                    self.current_token()
                ))
            }
        };
        self.advance();

        let mut constraint = Constraint::new(name);

        // Check for parameters
        if matches!(self.current_token(), Token::LParen) {
            self.advance();

            // Parse parameters
            loop {
                match self.current_token() {
                    Token::Number(n) => {
                        constraint = constraint.with_param(ConstraintParam::Number(*n));
                        self.advance();
                    }
                    Token::Ident(s) => {
                        constraint = constraint.with_param(ConstraintParam::String(s.clone()));
                        self.advance();
                    }
                    Token::Str(s) => {
                        constraint = constraint.with_param(ConstraintParam::String(s.clone()));
                        self.advance();
                    }
                    Token::Asterisk => {
                        // Special case for @relations(*) syntax
                        constraint =
                            constraint.with_param(ConstraintParam::String("*".to_string()));
                        self.advance();
                    }
                    _ => {
                        return Err(format!(
                            "Expected constraint parameter, found {:?}",
                            self.current_token()
                        ))
                    }
                }

                // Check for comma (more params) or closing paren
                match self.current_token() {
                    Token::Comma => {
                        self.advance();
                        continue;
                    }
                    Token::RParen => {
                        self.advance();
                        break;
                    }
                    _ => {
                        return Err(format!(
                            "Expected ',' or ')' in constraint parameters, found {:?}",
                            self.current_token()
                        ))
                    }
                }
            }
        }

        Ok(constraint)
    }

    fn parse_type(&mut self) -> Result<FieldType, String> {
        // Check for relation types first
        match self.current_token() {
            // Fixed array or One-to-many: [type; count] or [Post]
            Token::LBracket => {
                self.advance();

                // Check if this is a fixed array [type; count] or one-to-many [Model]
                let first_token = self.current_token().clone();

                match first_token {
                    Token::Ident(name) => {
                        self.advance();

                        // Check next token to distinguish [Model] vs [type; count]
                        match self.current_token() {
                            Token::Semicolon => {
                                // This is [Ident; count] - but Ident should be a type or struct
                                self.advance();
                                let count = match self.current_token() {
                                    Token::Number(n) => *n as usize,
                                    _ => {
                                        return Err(format!(
                                            "Expected array count after ';', found {:?}",
                                            self.current_token()
                                        ))
                                    }
                                };
                                self.advance();
                                self.expect(Token::RBracket)?;
                                // The Ident could be a struct type
                                return Ok(FieldType::FixedArray(
                                    Box::new(FieldType::StructType(name)),
                                    count,
                                ));
                            }
                            Token::RBracket => {
                                // This is [Model] - one-to-many relation
                                self.advance();
                                return Ok(FieldType::Relation(RelationType::OneToMany(name)));
                            }
                            _ => {
                                return Err(format!(
                                    "Expected ';' or ']' after type name, found {:?}",
                                    self.current_token()
                                ))
                            }
                        }
                    }
                    _ => {
                        // Parse base type for fixed array
                        let inner_type = self.parse_primitive_type()?;
                        self.expect(Token::Semicolon)?;
                        let count = match self.current_token() {
                            Token::Number(n) => *n as usize,
                            _ => {
                                return Err(format!(
                                    "Expected array count, found {:?}",
                                    self.current_token()
                                ))
                            }
                        };
                        self.advance();
                        self.expect(Token::RBracket)?;
                        return Ok(FieldType::FixedArray(Box::new(inner_type), count));
                    }
                }
            }
            // Required reference: *User
            Token::Asterisk => {
                self.advance();
                let model_name = match self.current_token() {
                    Token::Ident(name) => name.clone(),
                    _ => {
                        return Err(format!(
                            "Expected model name after '*', found {:?}",
                            self.current_token()
                        ))
                    }
                };
                self.advance();
                return Ok(FieldType::Relation(RelationType::RequiredReference(
                    model_name,
                )));
            }
            // Optional reference or nullable primitive: ?User  or  ?i32
            Token::Question => {
                self.advance();
                match self.current_token().clone() {
                    Token::Ident(name) => {
                        self.advance();
                        return Ok(FieldType::Relation(RelationType::OptionalReference(name)));
                    }
                    // Nullable primitive types: ?i32, ?string, ?bool, etc.
                    Token::TypeU32
                    | Token::TypeU64
                    | Token::TypeI32
                    | Token::TypeI64
                    | Token::TypeF64
                    | Token::TypeBool
                    | Token::TypeString
                    | Token::TypeUuid
                    | Token::TypeTimestamp
                    | Token::TypeJson
                    | Token::TypeDecimal
                    | Token::TypeChar => {
                        let inner = self.parse_primitive_type()?;
                        return Ok(FieldType::Nullable(Box::new(inner)));
                    }
                    _ => {
                        return Err(format!(
                            "Expected model name or primitive type after '?', found {:?}",
                            self.current_token()
                        ))
                    }
                }
            }
            // Struct types, component references, or primitive identifiers
            Token::Ident(name) => {
                let type_name = name.clone();
                self.advance();

                // Check if this is a component protocol (tsx://, jsx://, api://)
                if matches!(type_name.as_str(), "tsx" | "jsx" | "api")
                    && matches!(self.current_token(), Token::Colon)
                {
                    // Parse component reference: tsx://path
                    self.advance(); // skip :
                    self.skip_newlines(); // Skip any newlines after colon

                    // Expect two slashes
                    self.expect(Token::Slash)?;
                    self.expect(Token::Slash)?;

                    // Read the component path
                    let path = self.read_component_path()?;

                    // Determine protocol
                    let protocol = match type_name.as_str() {
                        "tsx" => ComponentProtocol::Tsx,
                        "jsx" => ComponentProtocol::Jsx,
                        "api" => ComponentProtocol::Api,
                        _ => unreachable!(),
                    };

                    return Ok(FieldType::Component(ComponentReference {
                        protocol,
                        path,
                        relations: RelationInclusion::None,
                    }));
                }

                // This could be a struct type or struct? for optional
                return Ok(FieldType::StructType(type_name));
            }
            _ => {}
        }

        // Primitive types
        self.parse_primitive_type()
    }

    fn parse_primitive_type(&mut self) -> Result<FieldType, String> {
        let field_type = match self.current_token() {
            Token::TypeU32 => FieldType::U32,
            Token::TypeU64 => FieldType::U64,
            Token::TypeI32 => FieldType::I32,
            Token::TypeI64 => FieldType::I64,
            Token::TypeF64 => FieldType::F64,
            Token::TypeBool => FieldType::Bool,
            Token::TypeString => FieldType::String,
            Token::TypeJson => FieldType::Json,
            Token::TypeDecimal => FieldType::Decimal,
            Token::TypeUuid => FieldType::Uuid,
            Token::TypeTimestamp => FieldType::Timestamp,
            Token::TypeChar => {
                // char(N) - expect (N)
                self.advance();
                self.expect(Token::LParen)?;
                let size = match self.current_token() {
                    Token::Number(n) => *n as usize,
                    _ => {
                        return Err(format!(
                            "Expected size after 'char(', found {:?}",
                            self.current_token()
                        ))
                    }
                };
                self.advance();
                self.expect(Token::RParen)?;
                return Ok(FieldType::Char(size));
            }
            _ => return Err(format!("Expected type, found {:?}", self.current_token())),
        };
        self.advance();
        Ok(field_type)
    }

    fn parse_directive(&mut self) -> Result<CompositeIndex, String> {
        // Expect @
        self.expect(Token::At)?;

        // Get directive name
        let directive_name = match self.current_token() {
            Token::Ident(s) => s.clone(),
            _ => {
                return Err(format!(
                    "Expected directive name after '@', found {:?}",
                    self.current_token()
                ))
            }
        };
        self.advance();

        match directive_name.as_str() {
            "index" => self.parse_index_directive(),
            _ => Err(format!("Unknown directive: @{}", directive_name)),
        }
    }

    fn parse_index_directive(&mut self) -> Result<CompositeIndex, String> {
        // Expect (
        self.expect(Token::LParen)?;

        // Parse field list
        let mut fields = Vec::new();
        loop {
            // Parse field name
            let field_name = match self.current_token() {
                Token::Ident(s) => s.clone(),
                _ => {
                    return Err(format!(
                        "Expected field name in @index directive, found {:?}",
                        self.current_token()
                    ))
                }
            };
            self.advance();
            fields.push(field_name);

            // Check for comma or closing paren
            match self.current_token() {
                Token::Comma => {
                    self.advance();
                    // Continue loop to parse next field
                }
                Token::RParen => {
                    self.advance();
                    break;
                }
                _ => {
                    return Err(format!(
                        "Expected ',' or ')' in @index directive, found {:?}",
                        self.current_token()
                    ))
                }
            }
        }

        if fields.len() < 2 {
            return Err("Composite index must include at least 2 fields".to_string());
        }

        Ok(CompositeIndex { fields })
    }

    /// Parse a `@projection(<name>: <field>, ...)` directive body (#113).  The
    /// caller has already consumed `@projection`; the current token is `(`.
    fn parse_projection_directive(&mut self) -> Result<Projection, String> {
        self.expect(Token::LParen)?;

        // Projection name.
        let name = match self.current_token() {
            Token::Ident(s) => s.clone(),
            _ => {
                return Err(format!(
                    "Expected projection name in @projection directive, found {:?}",
                    self.current_token()
                ))
            }
        };
        self.advance();

        // Name/field-list separator.
        self.expect(Token::Colon)?;

        // Field list (same shape as @index).
        let mut fields = Vec::new();
        loop {
            let field_name = match self.current_token() {
                Token::Ident(s) => s.clone(),
                _ => {
                    return Err(format!(
                        "Expected field name in @projection directive, found {:?}",
                        self.current_token()
                    ))
                }
            };
            self.advance();
            fields.push(field_name);

            match self.current_token() {
                Token::Comma => {
                    self.advance();
                }
                Token::RParen => {
                    self.advance();
                    break;
                }
                _ => {
                    return Err(format!(
                        "Expected ',' or ')' in @projection directive, found {:?}",
                        self.current_token()
                    ))
                }
            }
        }

        if fields.is_empty() {
            return Err(format!(
                "@projection '{}' must name at least one field",
                name
            ));
        }

        Ok(Projection { name, fields })
    }

    fn parse_field(&mut self) -> Result<Field, String> {
        self.skip_newlines();

        // Parse field name
        let field_pos = self.get_current_position();
        let name = match self.current_token() {
            Token::Ident(s) => s.clone(),
            _ => {
                return Err(format!(
                    "Expected field name, found {:?}",
                    self.current_token()
                ))
            }
        };
        self.advance();

        // Naming, duplicate, and reference checks are deferred to
        // `crate::validate` (the single positioned authority), run after the
        // whole schema is assembled. Only structural/syntactic errors are fatal
        // during parsing.

        // Expect colon
        self.expect(Token::Colon)?;

        // Check for symbols (+, &, and ^)
        let mut auto_generate = false;
        let mut unique = false;
        let mut indexed = false;

        loop {
            match self.current_token() {
                Token::Plus => {
                    auto_generate = true;
                    self.advance();
                }
                Token::Ampersand => {
                    unique = true;
                    self.advance();
                }
                Token::Caret => {
                    indexed = true;
                    self.advance();
                }
                _ => break,
            }
        }

        // Parse type
        let mut field_type = self.parse_type()?;

        // Check for postfix nullable marker (Type?)
        if matches!(self.current_token(), Token::Question) {
            match field_type {
                FieldType::StructType(ref name) => {
                    let name = name.clone();
                    self.advance();
                    field_type = FieldType::OptionalStructType(name);
                }
                // Nullable primitives with postfix `?`: e.g. `age: i32?`
                FieldType::U32
                | FieldType::U64
                | FieldType::I32
                | FieldType::I64
                | FieldType::F64
                | FieldType::Bool
                | FieldType::String
                | FieldType::Json
                | FieldType::Decimal
                | FieldType::Uuid
                | FieldType::Timestamp
                | FieldType::Char(_)
                | FieldType::FixedArray(_, _) => {
                    let inner = field_type.clone();
                    self.advance();
                    field_type = FieldType::Nullable(Box::new(inner));
                }
                _ => {} // Don't consume `?` for relations or other types
            }
        }

        // Validate auto-generate is compatible with type
        if auto_generate && !field_type.is_auto_generatable() {
            return Err(format!(
                "Auto-generate symbol '+' cannot be used with type {:?}. Only u32, u64, uuid, and timestamp support auto-generation",
                field_type
            ));
        }

        // Parse constraints (@directives)
        let mut constraints = Vec::new();
        let mut is_computed = false;
        let mut fulltext_indexed = false;
        let mut is_materialized = false;
        while matches!(self.current_token(), Token::At) {
            let constraint = self.parse_constraint()?;
            // Check if this is the @relations directive (Sprint 17 - for components)
            if constraint.name == "relations" {
                if let FieldType::Component(ref mut comp_ref) = field_type {
                    // Parse the @relations parameters
                    if constraint.params.is_empty() {
                        return Err("@relations directive requires parameters".to_string());
                    }

                    // Check if it's @relations(*)
                    if constraint.params.len() == 1 {
                        if let ConstraintParam::String(s) = &constraint.params[0] {
                            if s == "*" {
                                comp_ref.relations = RelationInclusion::All;
                            } else {
                                // Single relation field
                                comp_ref.relations = RelationInclusion::Specific(vec![s.clone()]);
                            }
                        } else {
                            return Err(
                                "@relations parameter must be a field name or *".to_string()
                            );
                        }
                    } else {
                        // Multiple specific relations
                        let mut fields = Vec::new();
                        for param in &constraint.params {
                            if let ConstraintParam::String(field_name) = param {
                                fields.push(field_name.clone());
                            } else {
                                return Err("@relations parameters must be field names".to_string());
                            }
                        }
                        comp_ref.relations = RelationInclusion::Specific(fields);
                    }
                    // Don't add @relations to constraints list since we've handled it
                    continue;
                } else {
                    return Err(
                        "@relations directive can only be used with component fields".to_string(),
                    );
                }
            }
            // Check if this is the @computed directive
            if constraint.name == "computed" {
                is_computed = true;
            }
            // Check if this is the @fulltext directive (Sprint 18)
            if constraint.name == "fulltext" {
                fulltext_indexed = true;
            }
            // Check if this is the @materialized directive (Sprint 19)
            if constraint.name == "materialized" {
                is_materialized = true;
            }
            constraints.push(constraint);
        }

        // Determine index type based on field type
        let index_type = field_type.default_index_type();

        Ok(Field {
            name,
            field_type,
            auto_generate,
            unique,
            indexed,
            constraints,
            index_type,
            is_computed,
            fulltext_indexed,
            is_materialized,
            position: field_pos,
        })
    }

    fn parse_struct(&mut self) -> Result<Struct, String> {
        self.skip_newlines();

        // Expect 'struct' keyword
        self.expect(Token::KwStruct)?;

        // Parse struct name
        let struct_pos = self.get_current_position();
        let name = match self.current_token() {
            Token::Ident(s) => s.clone(),
            _ => {
                return Err(format!(
                    "Expected struct name, found {:?}",
                    self.current_token()
                ))
            }
        };
        self.advance();

        // Name/duplicate/reference validation is deferred to `crate::validate`.

        // Expect opening brace
        self.skip_newlines();
        self.expect(Token::LBrace)?;

        // Parse fields
        let mut fields = Vec::new();
        self.skip_newlines();

        while !matches!(self.current_token(), Token::RBrace | Token::Eof) {
            let field = self.parse_field()?;
            fields.push(field);
            self.skip_newlines();
        }

        // Expect closing brace
        self.expect(Token::RBrace)?;

        if fields.is_empty() {
            return Err(format!("Struct '{}' has no fields", name));
        }

        Ok(Struct {
            name,
            fields,
            position: struct_pos,
        })
    }

    /// Parse a top-level `enum Name { V1, V2, ... }` (#enum).  A sibling of
    /// `struct`/model; variants are a comma/newline-separated PascalCase list
    /// (trailing comma optional, matching the struct/model brace style).
    fn parse_enum(&mut self) -> Result<EnumDef, String> {
        self.skip_newlines();

        // Expect 'enum' keyword
        self.expect(Token::KwEnum)?;

        // Parse enum name (PascalCase — validated like a model/struct name).
        let enum_pos = self.get_current_position();
        let name = match self.current_token() {
            Token::Ident(s) => s.clone(),
            _ => {
                return Err(format!(
                    "Expected enum name, found {:?}",
                    self.current_token()
                ))
            }
        };
        self.advance();

        // Name/variant/duplicate validation is deferred to `crate::validate`.

        // Expect opening brace
        self.skip_newlines();
        self.expect(Token::LBrace)?;
        self.skip_newlines();

        // Parse variant list.
        let mut variants = Vec::new();
        while !matches!(self.current_token(), Token::RBrace | Token::Eof) {
            let variant = match self.current_token() {
                Token::Ident(s) => s.clone(),
                _ => {
                    return Err(format!(
                        "Expected enum variant name in enum '{}', found {:?}",
                        name,
                        self.current_token()
                    ))
                }
            };
            self.advance();

            // Variant PascalCase + uniqueness are checked by `crate::validate`.
            variants.push(variant);

            // Separator: comma or newline; trailing comma optional.
            match self.current_token() {
                Token::Comma => {
                    self.advance();
                    self.skip_newlines();
                }
                _ => {
                    self.skip_newlines();
                }
            }
        }

        self.expect(Token::RBrace)?;

        if variants.is_empty() {
            return Err(format!("Enum '{}' has no variants", name));
        }

        Ok(EnumDef {
            name,
            variants,
            position: enum_pos,
        })
    }

    fn parse_model(&mut self) -> Result<Model, String> {
        self.skip_newlines();

        // Parse model name
        let model_pos = self.get_current_position();
        let name = match self.current_token() {
            Token::Ident(s) => s.clone(),
            _ => {
                return Err(format!(
                    "Expected model name, found {:?}",
                    self.current_token()
                ))
            }
        };
        self.advance();

        // Name/duplicate/reference validation is deferred to `crate::validate`.

        // Expect opening brace
        self.skip_newlines();
        self.expect(Token::LBrace)?;

        // Parse fields and directives
        let mut fields = Vec::new();
        let mut composite_indexes = Vec::new();
        let mut projections = Vec::new();
        let mut soft_delete = false;
        self.skip_newlines();

        while !matches!(self.current_token(), Token::RBrace | Token::Eof) {
            // Check for directive
            if matches!(self.current_token(), Token::At) {
                // Try to parse as composite index first
                let start_pos = self.position;
                match self.parse_directive() {
                    Ok(composite_index) => {
                        composite_indexes.push(composite_index);
                        self.skip_newlines();
                    }
                    Err(_) => {
                        // Reset position and try to parse as model-level directive
                        self.position = start_pos;
                        self.advance(); // skip @
                        let directive_name = match self.current_token() {
                            Token::Ident(s) => s.clone(),
                            _ => {
                                return Err(format!(
                                    "Expected directive name after '@', found {:?}",
                                    self.current_token()
                                ))
                            }
                        };
                        self.advance();

                        match directive_name.as_str() {
                            "soft_delete" => {
                                soft_delete = true;
                                self.skip_newlines();
                            }
                            "projection" => {
                                let projection = self.parse_projection_directive()?;
                                projections.push(projection);
                                self.skip_newlines();
                            }
                            _ => {
                                return Err(format!(
                                    "Unknown model directive: @{}",
                                    directive_name
                                ));
                            }
                        }
                    }
                }
            } else {
                let field = self.parse_field()?;
                fields.push(field);
                self.skip_newlines();
            }
        }

        // Expect closing brace
        self.expect(Token::RBrace)?;

        if fields.is_empty() {
            return Err(format!("Model '{}' has no fields", name));
        }

        // Duplicate field names, composite-index field references, and
        // projection name/field checks are deferred to `crate::validate` (the
        // single positioned authority), run once the whole schema is assembled.

        Ok(Model {
            name,
            fields,
            composite_indexes,
            projections,
            soft_delete,
            position: model_pos,
        })
    }

    /// Parse the input into a [`Schema`], running the full positioned semantic
    /// validation ([`crate::validate::validate_schema`]) and failing fast on the
    /// first diagnostic to preserve the historical `Result<Schema, String>`
    /// contract. Naming diagnostics are gated by the parser's `use_validation`
    /// flag (see [`Self::new_with_validation`]); structural/reference diagnostics
    /// always run.
    pub fn parse(&mut self) -> Result<Schema, String> {
        let schema = self.parse_unvalidated()?;

        let mut errors = Vec::new();
        if self.use_validation {
            crate::validate::collect_naming_errors(&schema, &mut errors);
        }
        crate::validate::collect_structure_errors(&schema, &mut errors);

        if let Some(first) = errors.first() {
            return Err(first.to_string());
        }

        Ok(schema)
    }

    /// Parse the input into a [`Schema`] performing only *structural* parsing:
    /// tokens are assembled into the AST and enum field-type references are
    /// resolved, but no schema-level semantic validation is run. Syntactic
    /// errors (unexpected tokens, malformed directives, empty models/structs,
    /// composite-index arity) are still fatal.
    ///
    /// The returned schema may therefore contain semantic defects (duplicate
    /// names, dangling relations, bad casing). Callers that want those reported —
    /// the CLI `forgedb validate` command and the LSP — run
    /// [`crate::validate::validate_schema`] on the result to collect **all**
    /// positioned diagnostics instead of only the first. (Error recovery for
    /// mid-keystroke buffers is #173 WS2c.)
    pub fn parse_unvalidated(&mut self) -> Result<Schema, String> {
        let mut structs = Vec::new();
        let mut enums = Vec::new();
        let mut models = Vec::new();
        // Names of declared enums, used only to resolve bare-identifier field
        // types below (duplicate detection is deferred to `crate::validate`).
        let mut enum_names = std::collections::HashSet::new();
        self.skip_newlines();

        while !matches!(self.current_token(), Token::Eof) {
            // Dispatch on the leading keyword: struct / enum / (bare) model.
            if matches!(self.current_token(), Token::KwStruct) {
                structs.push(self.parse_struct()?);
            } else if matches!(self.current_token(), Token::KwEnum) {
                let enum_def = self.parse_enum()?;
                enum_names.insert(enum_def.name.clone());
                enums.push(enum_def);
            } else {
                models.push(self.parse_model()?);
            }

            self.skip_newlines();
        }

        if models.is_empty() && structs.is_empty() && enums.is_empty() {
            return Err("Schema is empty".to_string());
        }

        // Resolve bare-identifier field types (parsed as `StructType`/
        // `OptionalStructType`) that name a declared enum into `FieldType::Enum`
        // (#enum).  A bare PascalCase identifier is either an enum (resolved here)
        // or a struct (left as `StructType`, validated by struct-reference checks).
        // This runs AFTER all declarations are collected so an enum may be declared
        // after the model that references it.
        for model in &mut models {
            for field in &mut model.fields {
                Self::resolve_enum_field_type(&mut field.field_type, &enum_names);
            }
        }

        Ok(Schema {
            structs,
            enums,
            models,
        })
    }

    /// Rewrite `StructType(name)`/`OptionalStructType(name)` → `Enum(name)`/
    /// `Nullable(Enum(name))` when `name` is a declared enum (#enum).  Everything
    /// else is left untouched (a real struct reference, or a non-named type).
    fn resolve_enum_field_type(
        field_type: &mut FieldType,
        enum_names: &std::collections::HashSet<String>,
    ) {
        match field_type {
            FieldType::StructType(name) if enum_names.contains(name) => {
                *field_type = FieldType::Enum(name.clone());
            }
            FieldType::OptionalStructType(name) if enum_names.contains(name) => {
                *field_type = FieldType::Nullable(Box::new(FieldType::Enum(name.clone())));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field_constraints<'a>(schema: &'a Schema, model: &str, field: &str) -> &'a [Constraint] {
        let m = schema
            .models
            .iter()
            .find(|m| m.name == model)
            .expect("model not found");
        let f = m
            .fields
            .iter()
            .find(|f| f.name == field)
            .expect("field not found");
        &f.constraints
    }

    /// 2a (epic #173): parsed AST nodes carry the source position of their name,
    /// so the LSP and unified validation can map diagnostics to editor ranges.
    /// Positions are 1-based line/column.
    #[test]
    fn parsed_nodes_carry_source_positions() {
        // Line-numbered so the expected positions are unambiguous:
        // 1: User {          2:   id: +uuid       3:   email: &string   4: }
        // 6: struct Point {  7:   x: i32          8: }
        // 10: enum Status {  11:   Active         12:   Inactive         13: }
        let src = "User {\n  id: +uuid\n  email: &string\n}\n\nstruct Point {\n  x: i32\n}\n\nenum Status {\n  Active\n  Inactive\n}\n";
        let schema = Parser::new(src).unwrap().parse().unwrap();

        let user = schema.find_model("User").unwrap();
        let upos = user.position.expect("model carries a position");
        assert_eq!((upos.line, upos.column), (1, 1), "User model name");

        let id = user.fields.iter().find(|f| f.name == "id").unwrap();
        let idpos = id.position.expect("field carries a position");
        assert_eq!((idpos.line, idpos.column), (2, 3), "id field name");

        let email = user.fields.iter().find(|f| f.name == "email").unwrap();
        assert_eq!(email.position.unwrap().line, 3, "email field line");

        let point = schema.find_struct("Point").unwrap();
        assert_eq!(point.position.unwrap().line, 6, "Point struct line");

        let status = schema.find_enum("Status").unwrap();
        assert_eq!(status.position.unwrap().line, 10, "Status enum line");
    }

    #[test]
    fn parses_projection_directive() {
        let src = r#"
            Post {
                id: +uuid
                title: string
                slug: ^string
                content: string

                @projection(card: title, slug)
                @projection(list_row: title)
            }
        "#;
        let schema = Parser::new(src).unwrap().parse().unwrap();
        let post = schema.models.iter().find(|m| m.name == "Post").unwrap();
        assert_eq!(post.projections.len(), 2);
        assert_eq!(post.projections[0].name, "card");
        assert_eq!(post.projections[0].fields, vec!["title", "slug"]);
        assert_eq!(post.projections[1].name, "list_row");
        assert_eq!(post.projections[1].fields, vec!["title"]);
    }

    #[test]
    fn projection_rejects_unknown_field() {
        let src = r#"
            Post {
                id: +uuid
                title: string
                @projection(card: title, nope)
            }
        "#;
        let err = Parser::new(src).unwrap().parse().unwrap_err();
        assert!(err.contains("undefined field 'nope'"), "got: {err}");
    }

    #[test]
    fn projection_rejects_duplicate_name() {
        let src = r#"
            Post {
                id: +uuid
                title: string
                slug: string
                @projection(card: title)
                @projection(card: slug)
            }
        "#;
        let err = Parser::new(src).unwrap().parse().unwrap_err();
        assert!(err.contains("Duplicate @projection name 'card'"), "got: {err}");
    }

    #[test]
    fn parses_json_and_nullable_json_types() {
        let src = r#"
            Event {
                id: +uuid
                payload: json
                meta: json?
            }
        "#;
        let schema = Parser::new(src).unwrap().parse().unwrap();
        let m = schema.models.iter().find(|m| m.name == "Event").unwrap();
        let payload = m.fields.iter().find(|f| f.name == "payload").unwrap();
        let meta = m.fields.iter().find(|f| f.name == "meta").unwrap();
        assert_eq!(payload.field_type, FieldType::Json);
        assert_eq!(
            meta.field_type,
            FieldType::Nullable(Box::new(FieldType::Json))
        );
        // json is variable-length (rides the string column path), not fixed-size.
        assert!(!payload.field_type.is_fixed_size());
        assert!(!meta.field_type.is_fixed_size());
    }

    #[test]
    fn parses_decimal_and_nullable_decimal_types() {
        let src = r#"
            Product {
                id: +uuid
                price: decimal
                discount: decimal?
            }
        "#;
        let schema = Parser::new(src).unwrap().parse().unwrap();
        let m = schema.models.iter().find(|m| m.name == "Product").unwrap();
        let price = m.fields.iter().find(|f| f.name == "price").unwrap();
        let discount = m.fields.iter().find(|f| f.name == "discount").unwrap();
        assert_eq!(price.field_type, FieldType::Decimal);
        assert_eq!(
            discount.field_type,
            FieldType::Nullable(Box::new(FieldType::Decimal))
        );
        // decimal is a fixed 16-byte column (like Uuid), NOT variable-length.
        assert!(price.field_type.is_fixed_size());
        assert!(discount.field_type.is_fixed_size());
        assert_eq!(price.field_type.to_rust_type(), "rust_decimal::Decimal");
    }

    #[test]
    fn parses_string_literal_directive_arg() {
        let src = r#"
            User {
                id: +uuid
                phone: string @pattern("^[0-9]+$")
            }
        "#;
        let schema = Parser::new(src).unwrap().parse().unwrap();
        let cons = field_constraints(&schema, "User", "phone");
        assert_eq!(cons.len(), 1);
        assert_eq!(cons[0].name, "pattern");
        assert_eq!(
            cons[0].params,
            vec![ConstraintParam::String("^[0-9]+$".to_string())]
        );
    }

    #[test]
    fn parses_string_default_on_nullable_string() {
        let src = r#"
            Ticket {
                id: +uuid
                status: string? @default("pending")
            }
        "#;
        let schema = Parser::new(src).unwrap().parse().unwrap();
        let cons = field_constraints(&schema, "Ticket", "status");
        assert_eq!(cons[0].name, "default");
        assert_eq!(
            cons[0].params,
            vec![ConstraintParam::String("pending".to_string())]
        );
    }

    #[test]
    fn bare_identifier_default_still_parses() {
        // The prior workaround must keep working — no regression.
        let src = r#"
            Ticket {
                id: +uuid
                status: string @default(pending)
            }
        "#;
        let schema = Parser::new(src).unwrap().parse().unwrap();
        let cons = field_constraints(&schema, "Ticket", "status");
        assert_eq!(
            cons[0].params,
            vec![ConstraintParam::String("pending".to_string())]
        );
    }

    #[test]
    fn string_and_number_args_mix() {
        let src = r#"
            Thing {
                id: +uuid
                code: string @pattern("[A-Z]{3}") @length(3, 3)
            }
        "#;
        let schema = Parser::new(src).unwrap().parse().unwrap();
        let cons = field_constraints(&schema, "Thing", "code");
        assert_eq!(cons.len(), 2);
        assert_eq!(cons[0].name, "pattern");
        assert_eq!(
            cons[0].params,
            vec![ConstraintParam::String("[A-Z]{3}".to_string())]
        );
        assert_eq!(cons[1].name, "length");
        assert_eq!(
            cons[1].params,
            vec![ConstraintParam::Number(3), ConstraintParam::Number(3)]
        );
    }

    #[test]
    fn parses_enum_and_enum_typed_fields() {
        // A top-level enum declaration, a field referencing it by bare name (the
        // field is resolved to `FieldType::Enum`), a nullable enum field, and an
        // enum declared AFTER the model that uses it (forward reference).
        let src = r#"
            Order {
                id: +uuid
                status: Status
                prev_status: Status?
            }
            enum Status { Active, Inactive, Pending }
        "#;
        let schema = Parser::new(src).unwrap().parse().unwrap();
        assert_eq!(schema.enums.len(), 1);
        let status_enum = schema.find_enum("Status").unwrap();
        assert_eq!(status_enum.variants, vec!["Active", "Inactive", "Pending"]);

        let order = schema.models.iter().find(|m| m.name == "Order").unwrap();
        let status = order.fields.iter().find(|f| f.name == "status").unwrap();
        assert_eq!(status.field_type, FieldType::Enum("Status".to_string()));
        let prev = order.fields.iter().find(|f| f.name == "prev_status").unwrap();
        assert_eq!(
            prev.field_type,
            FieldType::Nullable(Box::new(FieldType::Enum("Status".to_string())))
        );
        // enum is a fixed 1-byte column (not variable-length).
        assert!(status.field_type.is_fixed_size());
    }

    #[test]
    fn enum_trailing_comma_optional() {
        // Both trailing-comma and no-trailing-comma variant lists parse.
        let with = Parser::new("enum A { X, Y, }\nM { id: +uuid }")
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(with.find_enum("A").unwrap().variants, vec!["X", "Y"]);
        let without = Parser::new("enum A { X, Y }\nM { id: +uuid }")
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(without.find_enum("A").unwrap().variants, vec!["X", "Y"]);
    }

    #[test]
    fn enum_rejects_duplicate_variant() {
        let err = Parser::new("enum Status { Active, Active }\nM { id: +uuid }")
            .unwrap()
            .parse()
            .unwrap_err();
        assert!(err.contains("Duplicate variant 'Active'"), "got: {err}");
    }

    #[test]
    fn enum_rejects_lowercase_variant() {
        // Variants must be PascalCase.
        let err = Parser::new("enum Status { active }\nM { id: +uuid }")
            .unwrap()
            .parse()
            .unwrap_err();
        assert!(err.contains("PascalCase"), "got: {err}");
    }

    #[test]
    fn enum_rejects_empty() {
        let err = Parser::new("enum Status { }\nM { id: +uuid }")
            .unwrap()
            .parse()
            .unwrap_err();
        assert!(err.contains("no variants"), "got: {err}");
    }

    #[test]
    fn field_referencing_undefined_named_type_errors() {
        // A bare PascalCase identifier that is neither a declared enum nor struct
        // is an unknown named type.
        let err = Parser::new("Order { id: +uuid\n status: Nope }")
            .unwrap()
            .parse()
            .unwrap_err();
        assert!(err.contains("unknown type 'Nope'"), "got: {err}");
    }

    #[test]
    fn enum_rejects_duplicate_declaration() {
        let err = Parser::new("enum S { A }\nenum S { B }\nM { id: +uuid }")
            .unwrap()
            .parse()
            .unwrap_err();
        assert!(err.contains("Duplicate enum name 'S'"), "got: {err}");
    }
}
