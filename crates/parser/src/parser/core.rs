use crate::ast::{
    ComponentProtocol, ComponentReference, CompositeIndex, Constraint, ConstraintParam, Field,
    FieldType, Model, RelationInclusion, RelationType, Schema, Struct,
};
use crate::lexer::{Lexer, Token, TokenWithPos};
use forgedb_validation::{validate_field_name, validate_model_name, Position};

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

        // Validate field name
        if self.use_validation {
            if let Err(e) = validate_field_name(&name, field_pos) {
                return Err(e.to_string());
            }
        }

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

        // Validate struct name
        if self.use_validation {
            if let Err(e) = validate_model_name(&name, struct_pos) {
                return Err(e.to_string());
            }
        }

        // Expect opening brace
        self.skip_newlines();
        self.expect(Token::LBrace)?;

        // Parse fields
        let mut fields = Vec::new();
        let mut field_names = std::collections::HashSet::new();
        self.skip_newlines();

        while !matches!(self.current_token(), Token::RBrace | Token::Eof) {
            let field = self.parse_field()?;

            // Check for duplicate field name
            if field_names.contains(&field.name) {
                return Err(format!(
                    "Duplicate field name '{}' in struct '{}'",
                    field.name, name
                ));
            }
            field_names.insert(field.name.clone());

            fields.push(field);
            self.skip_newlines();
        }

        // Expect closing brace
        self.expect(Token::RBrace)?;

        if fields.is_empty() {
            return Err(format!("Struct '{}' has no fields", name));
        }

        Ok(Struct { name, fields })
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

        // Validate model name
        if self.use_validation {
            if let Err(e) = validate_model_name(&name, model_pos) {
                return Err(e.to_string());
            }
        }

        // Expect opening brace
        self.skip_newlines();
        self.expect(Token::LBrace)?;

        // Parse fields and directives
        let mut fields = Vec::new();
        let mut field_names = std::collections::HashSet::new();
        let mut composite_indexes = Vec::new();
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

                // Check for duplicate field name
                if field_names.contains(&field.name) {
                    return Err(format!(
                        "Duplicate field name '{}' in model '{}'",
                        field.name, name
                    ));
                }
                field_names.insert(field.name.clone());

                fields.push(field);
                self.skip_newlines();
            }
        }

        // Expect closing brace
        self.expect(Token::RBrace)?;

        if fields.is_empty() {
            return Err(format!("Model '{}' has no fields", name));
        }

        // Validate composite indexes reference existing fields
        for comp_idx in &composite_indexes {
            for field_name in &comp_idx.fields {
                if !field_names.contains(field_name) {
                    return Err(format!(
                        "Composite index in model '{}' references undefined field '{}'",
                        name, field_name
                    ));
                }
            }
        }

        Ok(Model {
            name,
            fields,
            composite_indexes,
            soft_delete,
        })
    }

    pub fn parse(&mut self) -> Result<Schema, String> {
        let mut structs = Vec::new();
        let mut models = Vec::new();
        let mut struct_names = std::collections::HashSet::new();
        let mut model_names = std::collections::HashSet::new();
        self.skip_newlines();

        while !matches!(self.current_token(), Token::Eof) {
            // Check if this is a struct or model declaration
            if matches!(self.current_token(), Token::KwStruct) {
                let struct_def = self.parse_struct()?;

                // Check for duplicate struct name
                if struct_names.contains(&struct_def.name) {
                    return Err(format!("Duplicate struct name '{}'", struct_def.name));
                }
                struct_names.insert(struct_def.name.clone());

                structs.push(struct_def);
            } else {
                let model = self.parse_model()?;

                // Check for duplicate model name
                if model_names.contains(&model.name) {
                    return Err(format!("Duplicate model name '{}'", model.name));
                }
                model_names.insert(model.name.clone());

                models.push(model);
            }

            self.skip_newlines();
        }

        if models.is_empty() && structs.is_empty() {
            return Err("Schema is empty".to_string());
        }

        let schema = Schema { structs, models };

        // Validate relations and struct references
        schema.validate_relations()?;
        schema.validate_struct_references()?;

        Ok(schema)
    }
}
