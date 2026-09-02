use crate::ast::{
    ComponentProtocol, ComponentReference, CompositeIndex, Constraint, ConstraintParam, EnumDef,
    Field, FieldType, Model, Projection, RelationInclusion, RelationType, Schema, Struct,
    TimestampPrecision,
};
use crate::lexer::{Lexer, Token, TokenWithPos};
use forgedb_validation::{Position, ValidationError};

#[derive(Debug, Clone)]
pub struct ParsedSchema {
    pub schema: Schema,
    pub diagnostics: Vec<ValidationError>,
}

pub struct Parser {
    tokens: Vec<Token>,
    tokens_with_pos: Vec<TokenWithPos>,
    position: usize,
    use_validation: bool,
    recovering: bool,
    recovery_diagnostics: Vec<ValidationError>,
    warnings: Vec<ValidationError>,
    #[cfg(test)]
    seed_warning: Option<String>,
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
            recovering: false,
            recovery_diagnostics: Vec::new(),
            warnings: Vec::new(),
            #[cfg(test)]
            seed_warning: None,
        })
    }

    #[inline]
    fn emit_seeded_warning(&mut self) {
        #[cfg(test)]
        if let Some(message) = self.seed_warning.clone() {
            self.warn(message, None, None);
        }
    }

    pub fn warnings(&self) -> &[ValidationError] {
        &self.warnings
    }

    pub fn take_warnings(&mut self) -> Vec<ValidationError> {
        std::mem::take(&mut self.warnings)
    }

    #[allow(dead_code)]
    pub(crate) fn warn(
        &mut self,
        message: impl Into<String>,
        position: Option<Position>,
        suggestion: Option<String>,
    ) {
        let mut diag = ValidationError::warning(message);
        diag.position = position;
        diag.suggestion = suggestion;
        self.warnings.push(diag);
    }

    fn get_current_position(&self) -> Option<Position> {
        if self.position < self.tokens_with_pos.len() {
            Some(self.tokens_with_pos[self.position].position)
        } else {
            None
        }
    }

    fn position_at(&self, idx: usize) -> Option<Position> {
        self.tokens_with_pos.get(idx).map(|t| t.position)
    }

    fn diag(message: String, position: Option<Position>) -> ValidationError {
        let err = ValidationError::new(message);
        match position {
            Some(p) => err.with_position(p),
            None => err,
        }
    }

    fn recover_to_member_boundary(&mut self) {
        while !matches!(
            self.current_token(),
            Token::Newline | Token::RBrace | Token::Eof
        ) {
            self.advance();
        }
        self.skip_newlines();
    }

    fn next_significant_is_lbrace(&self, idx: usize) -> bool {
        let mut i = idx;
        while matches!(self.tokens.get(i), Some(Token::Newline)) {
            i += 1;
        }
        matches!(self.tokens.get(i), Some(Token::LBrace))
    }

    fn synchronize_from(&mut self, start: usize) {
        let n = self.tokens.len();
        let mut i = start;

        while i < n {
            match &self.tokens[i] {
                Token::LBrace => {
                    let mut depth = 0i32;
                    let mut j = i;
                    while j < n {
                        match self.tokens[j] {
                            Token::LBrace => depth += 1,
                            Token::RBrace => {
                                depth -= 1;
                                if depth == 0 {
                                    self.position = j + 1;
                                    return;
                                }
                            }
                            Token::Eof => {
                                self.position = j;
                                return;
                            }
                            _ => {}
                        }
                        j += 1;
                    }
                    self.position = n;
                    return;
                }
                Token::Eof => {
                    self.position = i;
                    return;
                }
                Token::KwStruct | Token::KwEnum if i > start => {
                    self.position = i;
                    return;
                }
                Token::Ident(_) if i > start && self.next_significant_is_lbrace(i + 1) => {
                    self.position = i;
                    return;
                }
                _ => i += 1,
            }
        }
        self.position = n;
    }

    fn recover_diag(&mut self, message: String, at: usize) {
        let pos = self.position_at(at);
        self.recovery_diagnostics.push(Self::diag(message, pos));
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

    fn peek_token(&self) -> &Token {
        self.tokens.get(self.position + 1).unwrap_or(&Token::Eof)
    }

    fn at_bytes_type(&self) -> bool {
        matches!(self.current_token(), Token::Ident(name) if name == "bytes")
            && matches!(self.peek_token(), Token::LParen)
    }

    fn parse_bytes_type(&mut self) -> Result<FieldType, String> {
        let deprecated = matches!(self.current_token(), Token::TypeCharDeprecated);
        let keyword_position = self.get_current_position();
        let keyword = if deprecated { "char" } else { "bytes" };
        self.advance();
        self.expect(Token::LParen)?;
        let size = match self.current_token() {
            Token::Number(n) => *n as usize,
            _ => {
                return Err(format!(
                    "Expected size after '{}(', found {:?}",
                    keyword,
                    self.current_token()
                ))
            }
        };
        self.advance();
        self.expect(Token::RParen)?;
        if deprecated {
            self.warn(
                format!(
                    "`char({size})` is deprecated and will be removed in the next major version. \
                     The type stores raw bytes — there is no UTF-8 guarantee and no text \
                     semantics, unlike SQL's CHAR(N), which is fixed-length text. If this field \
                     holds text, use `string` instead."
                ),
                keyword_position,
                Some(format!("bytes({size})")),
            );
        }
        Ok(FieldType::Bytes(size))
    }

    fn at_parameterized_string(&self) -> bool {
        matches!(self.current_token(), Token::TypeString)
            && matches!(self.peek_token(), Token::LParen)
    }

    fn parse_string_type(&mut self) -> Result<FieldType, String> {
        let keyword_position = self.get_current_position();
        self.advance();
        self.expect(Token::LParen)?;
        let raw = match self.current_token() {
            Token::Number(n) => *n,
            _ => {
                return Err(format!(
                    "Expected a character count after 'string(', found {:?}",
                    self.current_token()
                ))
            }
        };
        let chars = u8::try_from(raw).ok().filter(|n| *n >= 1).ok_or_else(|| {
            let at = keyword_position
                .map(|p| format!(" at line {}, column {}", p.line, p.column))
                .unwrap_or_default();
            format!(
                "Inline string width must be between 1 and 255 characters, found {raw}{at}. \
                 For an unbounded value use bare `string`."
            )
        })?;
        self.advance();
        let exact = if matches!(self.current_token(), Token::Bang) {
            self.advance();
            true
        } else {
            false
        };
        self.expect(Token::RParen)?;
        Ok(FieldType::StringN { chars, exact })
    }

    fn at_parameterized_timestamp(&self) -> bool {
        matches!(self.current_token(), Token::TypeTimestamp)
            && matches!(self.peek_token(), Token::LParen)
    }

    fn parse_timestamp_type(&mut self) -> Result<FieldType, String> {
        let keyword_position = self.get_current_position();
        self.advance();
        self.expect(Token::LParen)?;
        let key = match self.current_token() {
            Token::Ident(name) => name.clone(),
            other => format!("{other:?}"),
        };
        let precision = TimestampPrecision::from_key(&key).ok_or_else(|| {
            let at = keyword_position
                .map(|p| format!(" at line {}, column {}", p.line, p.column))
                .unwrap_or_default();
            format!(
                "Unknown timestamp precision `{key}`{at}. The precisions are `s`, `ms` and \
                 `us`; a bare `timestamp` means `timestamp(ms)`. Nanoseconds are not \
                 offerable — the on-disk unit is microseconds."
            )
        })?;
        self.advance();
        self.expect(Token::RParen)?;
        Ok(FieldType::Timestamp(precision))
    }

    fn skip_newlines(&mut self) {
        while matches!(self.current_token(), Token::Newline) {
            self.advance();
        }
    }

    fn read_component_path(&mut self) -> Result<String, String> {
        let mut path = String::new();

        loop {
            match self.current_token() {
                Token::Ident(name) => {
                    path.push_str(name);
                    self.advance();

                    if matches!(self.current_token(), Token::Slash) {
                        path.push('/');
                        self.advance();
                    } else {
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
        self.expect(Token::At)?;

        let constraint_position = self.get_current_position();
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

        if matches!(self.current_token(), Token::LParen) {
            self.advance();

            loop {
                match self.current_token() {
                    Token::Number(n) => {
                        constraint = constraint.with_param(ConstraintParam::Number(*n));
                        self.advance();
                    }
                    Token::Fractional(s) => {
                        constraint =
                            constraint.with_param(ConstraintParam::Fractional(s.clone()));
                        self.advance();
                    }
                    Token::Gt | Token::Lt => {
                        let greater = matches!(self.current_token(), Token::Gt);
                        let op = if greater { '>' } else { '<' };
                        self.advance();
                        let value = match self.current_token() {
                            Token::Number(n) => ConstraintParam::Number(*n),
                            Token::Fractional(s) => ConstraintParam::Fractional(s.clone()),
                            other => {
                                return Err(format!(
                                    "Expected a number after '{op}' in constraint \
                                     parameters, found {other:?}"
                                ))
                            }
                        };
                        self.advance();
                        constraint = constraint.with_param(ConstraintParam::Exclusive {
                            greater,
                            value: Box::new(value),
                        });
                    }
                    Token::Ident(s) => {
                        let ident = s.clone();
                        self.advance();
                        if matches!(self.current_token(), Token::Colon) {
                            self.advance();
                            let value = match self.current_token() {
                                Token::Number(n) => ConstraintParam::Number(*n),
                                Token::Fractional(s) => ConstraintParam::Fractional(s.clone()),
                                Token::Str(s) => ConstraintParam::String(s.clone()),
                                Token::Ident(s) => ConstraintParam::String(s.clone()),
                                other => {
                                    return Err(format!(
                                        "Expected a value after '{ident}:' in constraint \
                                         parameters, found {other:?}"
                                    ))
                                }
                            };
                            self.advance();
                            constraint = constraint.with_param(ConstraintParam::Named {
                                name: ident,
                                value: Box::new(value),
                            });
                        } else {
                            constraint = constraint.with_param(ConstraintParam::String(ident));
                        }
                    }
                    Token::Str(s) => {
                        constraint = constraint.with_param(ConstraintParam::String(s.clone()));
                        self.advance();
                    }
                    Token::Asterisk => {
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

        if constraint.name == "length" {
            self.check_length_constraint(&constraint, constraint_position)?;
        }

        Ok(constraint)
    }

    fn check_length_constraint(
        &mut self,
        constraint: &Constraint,
        position: Option<Position>,
    ) -> Result<(), String> {
        let named: Vec<(&str, &ConstraintParam)> = constraint
            .params
            .iter()
            .filter_map(|p| match p {
                ConstraintParam::Named { name, value } => Some((name.as_str(), value.as_ref())),
                _ => None,
            })
            .collect();

        if named.is_empty() {
            if constraint.params.len() == 1
                && matches!(constraint.params[0], ConstraintParam::Number(_))
            {
                let ConstraintParam::Number(n) = constraint.params[0] else {
                    unreachable!("guarded by the matches! above")
                };
                self.warn(
                    format!(
                        "`@length({n})` now means an EXACT length of {n}; it previously meant a \
                         maximum of {n}. Write `@length(max: {n})` to keep the old meaning, or \
                         `@length({n})` deliberately to require exactly {n}."
                    ),
                    position,
                    Some(format!("@length(max: {n})")),
                );
            }
            return Ok(());
        }

        if named.len() != constraint.params.len() {
            return Err(
                "@length takes either positional numbers or named arguments, not both — \
                 write `@length(min: a, max: b)`"
                    .to_string(),
            );
        }

        let (mut min, mut max) = (None, None);
        for (name, value) in named {
            let slot = match name {
                "min" => &mut min,
                "max" => &mut max,
                other => {
                    return Err(format!(
                        "Unknown @length argument `{other}` — the accepted names are `min` and \
                         `max`, as in `@length(min: 3, max: 64)`"
                    ))
                }
            };
            let ConstraintParam::Number(n) = value else {
                return Err(format!(
                    "@length argument `{name}` must be a number, as in `@length({name}: 20)`"
                ));
            };
            if slot.is_some() {
                return Err(format!("duplicate @length argument `{name}`"));
            }
            *slot = Some(*n);
        }

        if let (Some(lo), Some(hi)) = (min, max)
            && lo > hi
        {
            return Err(format!(
                "@length min ({lo}) is greater than max ({hi}) — no value can satisfy it"
            ));
        }

        Ok(())
    }

    fn parse_type(&mut self) -> Result<FieldType, String> {
        if self.at_bytes_type() {
            return self.parse_bytes_type();
        }
        if self.at_parameterized_string() {
            return self.parse_string_type();
        }
        if self.at_parameterized_timestamp() {
            return self.parse_timestamp_type();
        }

        match self.current_token() {
            Token::LBracket => {
                self.advance();

                let first_token = self.current_token().clone();

                if self.at_bytes_type() {
                    let inner_type = self.parse_bytes_type()?;
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

                match first_token {
                    Token::Ident(name) => {
                        self.advance();

                        match self.current_token() {
                            Token::Semicolon => {
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
                                return Ok(FieldType::FixedArray(
                                    Box::new(FieldType::StructType(name)),
                                    count,
                                ));
                            }
                            Token::RBracket => {
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
            Token::Question => {
                self.advance();
                if self.at_bytes_type() {
                    let inner = self.parse_bytes_type()?;
                    return Ok(FieldType::Nullable(Box::new(inner)));
                }
                match self.current_token().clone() {
                    Token::Ident(name) => {
                        self.advance();
                        return Ok(FieldType::Relation(RelationType::OptionalReference(name)));
                    }
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
                    | Token::TypeCharDeprecated => {
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
            Token::Ident(name) => {
                let type_name = name.clone();
                self.advance();

                if matches!(type_name.as_str(), "tsx" | "jsx" | "api")
                    && matches!(self.current_token(), Token::Colon)
                {
                    self.advance();
                    self.skip_newlines();

                    self.expect(Token::Slash)?;
                    self.expect(Token::Slash)?;

                    let path = self.read_component_path()?;

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

                return Ok(FieldType::StructType(type_name));
            }
            _ => {}
        }

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
            Token::TypeString if matches!(self.peek_token(), Token::LParen) => {
                return self.parse_string_type()
            }
            Token::TypeString => FieldType::String,
            Token::TypeJson => FieldType::Json,
            Token::TypeDecimal => FieldType::Decimal,
            Token::TypeUuid => FieldType::Uuid,
            Token::TypeTimestamp if matches!(self.peek_token(), Token::LParen) => {
                return self.parse_timestamp_type()
            }
            Token::TypeTimestamp => FieldType::Timestamp(TimestampPrecision::default()),
            Token::TypeCharDeprecated => return self.parse_bytes_type(),
            Token::Ident(name) if name == "bytes" && matches!(self.peek_token(), Token::LParen) => {
                return self.parse_bytes_type()
            }
            _ => return Err(format!("Expected type, found {:?}", self.current_token())),
        };
        self.advance();
        Ok(field_type)
    }

    fn parse_directive(&mut self) -> Result<CompositeIndex, String> {
        self.expect(Token::At)?;

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
        self.expect(Token::LParen)?;

        let mut fields = Vec::new();
        loop {
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

    fn parse_projection_directive(&mut self) -> Result<Projection, String> {
        self.expect(Token::LParen)?;

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

        self.expect(Token::Colon)?;

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

        self.expect(Token::Colon)?;

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

        let mut field_type = self.parse_type()?;

        if matches!(self.current_token(), Token::Question) {
            match field_type {
                FieldType::StructType(ref name) => {
                    let name = name.clone();
                    self.advance();
                    field_type = FieldType::OptionalStructType(name);
                }
                FieldType::U32
                | FieldType::U64
                | FieldType::I32
                | FieldType::I64
                | FieldType::F64
                | FieldType::Bool
                | FieldType::String
                | FieldType::StringN { .. }
                | FieldType::Json
                | FieldType::Decimal
                | FieldType::Uuid
                | FieldType::Timestamp(_)
                | FieldType::Bytes(_)
                | FieldType::FixedArray(_, _) => {
                    let inner = field_type.clone();
                    self.advance();
                    field_type = FieldType::Nullable(Box::new(inner));
                }
                _ => {}
            }
        }

        if auto_generate && !field_type.is_auto_generatable() {
            return Err(format!(
                "Auto-generate symbol '+' cannot be used with type {:?}. Only u32, u64, uuid, and timestamp support auto-generation",
                field_type
            ));
        }

        let mut constraints = Vec::new();
        let mut is_computed = false;
        let mut fulltext_indexed = false;
        let mut is_materialized = false;
        while matches!(self.current_token(), Token::At) {
            let constraint = self.parse_constraint()?;
            if constraint.name == "relations" {
                if let FieldType::Component(ref mut comp_ref) = field_type {
                    if constraint.params.is_empty() {
                        return Err("@relations directive requires parameters".to_string());
                    }

                    if constraint.params.len() == 1 {
                        if let ConstraintParam::String(s) = &constraint.params[0] {
                            if s == "*" {
                                comp_ref.relations = RelationInclusion::All;
                            } else {
                                comp_ref.relations = RelationInclusion::Specific(vec![s.clone()]);
                            }
                        } else {
                            return Err(
                                "@relations parameter must be a field name or *".to_string()
                            );
                        }
                    } else {
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
                    continue;
                } else {
                    return Err(
                        "@relations directive can only be used with component fields".to_string(),
                    );
                }
            }
            if constraint.name == "computed" {
                is_computed = true;
            }
            if constraint.name == "fulltext" {
                fulltext_indexed = true;
            }
            if constraint.name == "materialized" {
                is_materialized = true;
            }
            constraints.push(constraint);
        }

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

        self.expect(Token::KwStruct)?;

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

        self.skip_newlines();
        self.expect(Token::LBrace)?;

        let mut fields = Vec::new();
        self.skip_newlines();

        while !matches!(self.current_token(), Token::RBrace | Token::Eof) {
            let member_start = self.position;
            match self.parse_field() {
                Ok(field) => {
                    fields.push(field);
                    self.skip_newlines();
                }
                Err(e) if self.recovering => {
                    self.recover_diag(e, member_start);
                    self.recover_to_member_boundary();
                    if self.position == member_start {
                        self.advance();
                    }
                }
                Err(e) => return Err(e),
            }
        }

        if let Err(e) = self.expect(Token::RBrace) {
            if self.recovering {
                self.recover_diag(e, self.position);
            } else {
                return Err(e);
            }
        }

        if fields.is_empty() {
            let e = format!("Struct '{}' has no fields", name);
            if self.recovering {
                self.recovery_diagnostics.push(Self::diag(e, struct_pos));
            } else {
                return Err(e);
            }
        }

        Ok(Struct {
            name,
            fields,
            position: struct_pos,
        })
    }

    fn parse_enum(&mut self) -> Result<EnumDef, String> {
        self.skip_newlines();

        self.expect(Token::KwEnum)?;

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

        self.skip_newlines();
        self.expect(Token::LBrace)?;
        self.skip_newlines();

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

            variants.push(variant);

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

        self.skip_newlines();
        self.expect(Token::LBrace)?;

        let mut fields = Vec::new();
        let mut composite_indexes = Vec::new();
        let mut projections = Vec::new();
        let mut soft_delete = false;
        self.skip_newlines();

        while !matches!(self.current_token(), Token::RBrace | Token::Eof) {
            let member_start = self.position;
            match self.parse_model_member(
                &mut fields,
                &mut composite_indexes,
                &mut projections,
                &mut soft_delete,
            ) {
                Ok(()) => {}
                Err(e) if self.recovering => {
                    self.recover_diag(e, member_start);
                    self.recover_to_member_boundary();
                    if self.position == member_start {
                        self.advance();
                    }
                }
                Err(e) => return Err(e),
            }
        }

        if let Err(e) = self.expect(Token::RBrace) {
            if self.recovering {
                self.recover_diag(e, self.position);
            } else {
                return Err(e);
            }
        }

        if fields.is_empty() {
            let e = format!("Model '{}' has no fields", name);
            if self.recovering {
                self.recovery_diagnostics
                    .push(Self::diag(e, model_pos));
            } else {
                return Err(e);
            }
        }

        Ok(Model {
            name,
            fields,
            composite_indexes,
            projections,
            soft_delete,
            position: model_pos,
        })
    }

    fn parse_model_member(
        &mut self,
        fields: &mut Vec<Field>,
        composite_indexes: &mut Vec<CompositeIndex>,
        projections: &mut Vec<Projection>,
        soft_delete: &mut bool,
    ) -> Result<(), String> {
        if matches!(self.current_token(), Token::At) {
            let start_pos = self.position;
            match self.parse_directive() {
                Ok(composite_index) => {
                    composite_indexes.push(composite_index);
                    self.skip_newlines();
                }
                Err(_) => {
                    self.position = start_pos;
                    self.advance();
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
                            *soft_delete = true;
                            self.skip_newlines();
                        }
                        "projection" => {
                            let projection = self.parse_projection_directive()?;
                            projections.push(projection);
                            self.skip_newlines();
                        }
                        _ => {
                            return Err(format!("Unknown model directive: @{}", directive_name));
                        }
                    }
                }
            }
        } else {
            let field = self.parse_field()?;
            fields.push(field);
            self.skip_newlines();
        }
        Ok(())
    }

    pub fn parse(&mut self) -> Result<Schema, String> {
        let schema = self.parse_unvalidated()?;

        let mut errors = Vec::new();
        if self.use_validation {
            crate::validate::collect_naming_errors(&schema, &mut errors);
        }
        crate::validate::collect_structure_errors(&schema, &mut errors);

        if let Some(first) = errors.iter().find(|d| !d.is_warning()) {
            return Err(first.to_string());
        }
        self.warnings.extend(errors.into_iter().filter(|d| d.is_warning()));

        Ok(schema)
    }

    pub fn parse_unvalidated(&mut self) -> Result<Schema, String> {
        self.warnings.clear();
        self.emit_seeded_warning();

        let mut structs = Vec::new();
        let mut enums = Vec::new();
        let mut models = Vec::new();
        let mut enum_names = std::collections::HashSet::new();
        self.skip_newlines();

        while !matches!(self.current_token(), Token::Eof) {
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

    pub fn parse_recover(&mut self) -> ParsedSchema {
        enum Decl {
            Struct(Struct),
            Enum(EnumDef),
            Model(Model),
        }

        let prev_recovering = self.recovering;
        self.recovering = true;
        self.recovery_diagnostics.clear();
        self.warnings.clear();
        self.emit_seeded_warning();

        let mut structs = Vec::new();
        let mut enums = Vec::new();
        let mut models = Vec::new();
        let mut enum_names = std::collections::HashSet::new();

        self.skip_newlines();
        while !matches!(self.current_token(), Token::Eof) {
            let decl_start = self.position;
            let result = if matches!(self.current_token(), Token::KwStruct) {
                self.parse_struct().map(Decl::Struct)
            } else if matches!(self.current_token(), Token::KwEnum) {
                self.parse_enum().map(Decl::Enum)
            } else {
                self.parse_model().map(Decl::Model)
            };

            match result {
                Ok(Decl::Struct(s)) => structs.push(s),
                Ok(Decl::Enum(e)) => {
                    enum_names.insert(e.name.clone());
                    enums.push(e);
                }
                Ok(Decl::Model(m)) => models.push(m),
                Err(e) => {
                    self.recover_diag(e, decl_start);
                    self.synchronize_from(decl_start);
                }
            }

            if self.position == decl_start {
                self.advance();
            }
            self.skip_newlines();
        }

        for model in &mut models {
            for field in &mut model.fields {
                Self::resolve_enum_field_type(&mut field.field_type, &enum_names);
            }
        }

        let schema = Schema {
            structs,
            enums,
            models,
        };

        let mut diagnostics = std::mem::take(&mut self.recovery_diagnostics);
        diagnostics.extend(crate::validate::validate_schema(&schema));
        diagnostics.extend(self.take_warnings());
        diagnostics.sort_by_key(|d| {
            d.position
                .map(|p| (p.line, p.column))
                .unwrap_or((usize::MAX, usize::MAX))
        });

        self.recovering = prev_recovering;
        ParsedSchema { schema, diagnostics }
    }

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

    #[test]
    fn parsed_nodes_carry_source_positions() {
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
    fn length_accepts_named_arguments() {
        for (src_args, expected) in [
            (
                "min: 1",
                vec![ConstraintParam::Named {
                    name: "min".to_string(),
                    value: Box::new(ConstraintParam::Number(1)),
                }],
            ),
            (
                "max: 20",
                vec![ConstraintParam::Named {
                    name: "max".to_string(),
                    value: Box::new(ConstraintParam::Number(20)),
                }],
            ),
            (
                "min: 3, max: 64",
                vec![
                    ConstraintParam::Named {
                        name: "min".to_string(),
                        value: Box::new(ConstraintParam::Number(3)),
                    },
                    ConstraintParam::Named {
                        name: "max".to_string(),
                        value: Box::new(ConstraintParam::Number(64)),
                    },
                ],
            ),
            (
                "max: 64, min: 3",
                vec![
                    ConstraintParam::Named {
                        name: "max".to_string(),
                        value: Box::new(ConstraintParam::Number(64)),
                    },
                    ConstraintParam::Named {
                        name: "min".to_string(),
                        value: Box::new(ConstraintParam::Number(3)),
                    },
                ],
            ),
        ] {
            let src = format!(
                "Thing {{\n  id: +uuid\n  name: string @length({src_args})\n}}\n"
            );
            let mut parser = Parser::new(&src).unwrap();
            let schema = parser
                .parse()
                .unwrap_or_else(|e| panic!("`@length({src_args})` must parse: {e}"));
            let cons = field_constraints(&schema, "Thing", "name");
            assert_eq!(cons[0].params, expected, "for `@length({src_args})`");
            assert!(
                parser.warnings().is_empty(),
                "the named form is the recommended spelling and must be silent: {:?}",
                parser.warnings()
            );
        }
    }

    #[test]
    fn length_positional_pair_is_unchanged_and_silent() {
        let src = r#"
            Thing {
                id: +uuid
                name: string @length(3, 5)
            }
        "#;
        let mut parser = Parser::new(src).unwrap();
        let schema = parser.parse().unwrap();
        let cons = field_constraints(&schema, "Thing", "name");
        assert_eq!(
            cons[0].params,
            vec![ConstraintParam::Number(3), ConstraintParam::Number(5)]
        );
        assert!(
            parser.warnings().is_empty(),
            "the positional pair keeps working with no diagnostic: {:?}",
            parser.warnings()
        );
    }

    #[test]
    fn length_single_arg_warns_that_it_now_means_exact() {
        let src = r#"
            Thing {
                id: +uuid
                name: string @length(20)
            }
        "#;
        let mut parser = Parser::new(src).unwrap();
        let schema = parser.parse().expect("it must still parse");
        let cons = field_constraints(&schema, "Thing", "name");
        assert_eq!(
            cons[0].params,
            vec![ConstraintParam::Number(20)],
            "the AST is unchanged — the meaning moved, not the shape"
        );

        let warnings = parser.take_warnings();
        assert_eq!(warnings.len(), 1, "exactly one warning: {warnings:?}");
        let w = &warnings[0];
        assert!(w.is_warning(), "a meaning change is not a parse error");
        assert!(
            w.message.contains("max: 20"),
            "the warning must name the spelling that preserves the old meaning: {}",
            w.message
        );
        assert!(
            w.message.contains("exact"),
            "the warning must say what it means now: {}",
            w.message
        );
        assert!(w.position.is_some(), "the diagnostic is anchored in the source");
    }

    #[test]
    fn length_rejects_malformed_named_arguments() {
        for (args, expected_fragment) in [
            ("foo: 3", "min"),
            ("min: 1, min: 2", "duplicate"),
            ("min: 5, max: 3", "min"),
            ("1, max: 2", "positional"),
            ("min: \"x\"", "number"),
        ] {
            let src = format!(
                "Thing {{\n  id: +uuid\n  name: string @length({args})\n}}\n"
            );
            let err = Parser::new(&src)
                .unwrap()
                .parse()
                .expect_err(&format!("`@length({args})` must be rejected"));
            assert!(
                err.to_lowercase().contains(expected_fragment),
                "`@length({args})` error should mention {expected_fragment:?}, got: {err}"
            );
        }
    }

    #[test]
    fn length_named_arguments_survive_parse_recover() {
        let src = r#"
            Thing {
                id: +uuid
                name: string @length(min: 3, max: 64)
            }
        "#;
        let mut parser = Parser::new(src).unwrap();
        let parsed = parser.parse_recover();
        assert!(
            parsed.diagnostics.iter().all(|d| d.is_warning()),
            "valid source must produce no errors on the recovering path: {:?}",
            parsed.diagnostics
        );
        let cons = field_constraints(&parsed.schema, "Thing", "name");
        assert_eq!(cons[0].params.len(), 2, "both named args survive recovery");
    }

    #[test]
    fn parses_enum_and_enum_typed_fields() {
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
        assert!(status.field_type.is_fixed_size());
    }

    #[test]
    fn enum_trailing_comma_optional() {
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

    fn recover(input: &str) -> ParsedSchema {
        Parser::new(input).unwrap().parse_recover()
    }

    #[test]
    fn recover_valid_schema_matches_parse() {
        let input = "User {\n  id: +uuid\n  email: string\n}\nPost {\n  id: +uuid\n}\n";
        let recovered = recover(input);
        assert!(recovered.diagnostics.is_empty(), "no diagnostics: {:?}", recovered.diagnostics);
        assert_eq!(recovered.schema, Parser::new(input).unwrap().parse().unwrap());
    }

    #[test]
    fn recover_empty_input_is_not_an_error() {
        let recovered = recover("\n  \n");
        assert!(recovered.diagnostics.is_empty());
        assert!(recovered.schema.models.is_empty());
    }

    #[test]
    fn recover_skips_a_broken_declaration_and_keeps_the_rest() {
        let input = "\
User {
  id: +uuid
}

Broken
  id: +uuid

Post {
  id: +uuid
}
";
        let recovered = recover(input);
        let names: Vec<&str> = recovered.schema.models.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"User"), "User survived: {names:?}");
        assert!(names.contains(&"Post"), "Post survived after recovery: {names:?}");
        assert!(
            recovered.diagnostics.iter().any(|d| d.position.is_some()),
            "the broken declaration produced a positioned diagnostic"
        );
    }

    #[test]
    fn recover_skips_a_broken_field_and_keeps_the_model() {
        let input = "User {\n  id: +uuid\n  @@@ : broken\n  email: string\n}\n";
        let recovered = recover(input);
        let user = recovered
            .schema
            .find_model("User")
            .expect("User model still produced despite a bad field");
        let fields: Vec<&str> = user.fields.iter().map(|f| f.name.as_str()).collect();
        assert!(fields.contains(&"id") && fields.contains(&"email"), "good fields survived: {fields:?}");
        assert!(!recovered.diagnostics.is_empty(), "the bad field was reported");
    }

    #[test]
    fn recover_merges_semantic_diagnostics() {
        let input = "User {\n  id: +uuid\n  pet: *Ghost\n}\n";
        let recovered = recover(input);
        assert!(
            recovered
                .diagnostics
                .iter()
                .any(|d| d.message.contains("references undefined model 'Ghost'")),
            "semantic diagnostics included: {:?}",
            recovered.diagnostics
        );
    }

    #[test]
    fn recover_diagnostics_are_sorted_by_position() {
        let input = "User {\n  id: +uuid\n  BadName: string\n  a: *Nope\n}\n";
        let recovered = recover(input);
        let lines: Vec<usize> = recovered
            .diagnostics
            .iter()
            .filter_map(|d| d.position.map(|p| p.line))
            .collect();
        let mut sorted = lines.clone();
        sorted.sort();
        assert_eq!(lines, sorted, "diagnostics sorted by line: {lines:?}");
    }

    #[test]
    fn recover_handles_unterminated_block() {
        let recovered = recover("User {\n  id: +uuid\n  email:");
        assert!(!recovered.diagnostics.is_empty(), "unterminated buffer is diagnosed");
    }

    #[test]
    fn recover_reports_multiple_broken_declarations() {
        let input = "\
A
  x: u32

Good {
  id: +uuid
}

B
  y: u32
";
        let recovered = recover(input);
        assert!(recovered.schema.find_model("Good").is_some(), "middle valid model survived");
        assert!(
            recovered.diagnostics.len() >= 2,
            "both broken declarations reported: {:?}",
            recovered.diagnostics
        );
    }

    const VALID: &str = "User {\n  id: +uuid\n  email: string\n}\n";

    #[test]
    fn warnings_are_empty_without_a_producer() {
        let mut p = Parser::new(VALID).unwrap();
        assert!(p.parse().is_ok(), "valid schema still parses");
        assert!(p.warnings().is_empty(), "no producer ⇒ no warnings");

        let recovered = Parser::new(VALID).unwrap().parse_recover();
        assert!(
            recovered.diagnostics.is_empty(),
            "valid schema has no diagnostics of any severity: {:?}",
            recovered.diagnostics
        );
    }

    #[test]
    fn fail_fast_parse_carries_warnings() {
        let mut p = Parser::new(VALID).unwrap();
        p.parse().expect("valid schema parses");
        p.warn("char(n) is deprecated", Some(Position::new(2, 7)), Some("use bytes(n)".into()));

        assert_eq!(p.warnings().len(), 1);
        let w = &p.warnings()[0];
        assert!(w.is_warning(), "severity is Warning, not Error");
        assert_eq!(w.position, Some(Position::new(2, 7)));
        assert_eq!(w.suggestion.as_deref(), Some("use bytes(n)"));

        let taken = p.take_warnings();
        assert_eq!(taken.len(), 1);
        assert!(p.warnings().is_empty(), "take_warnings drains the buffer");
    }

    #[test]
    fn warnings_do_not_accumulate_across_parses() {
        let mut p = Parser::new(VALID).unwrap();
        p.parse().unwrap();
        p.warn("first run", None, None);
        assert_eq!(p.warnings().len(), 1);

        p.position = 0;
        p.parse().unwrap();
        assert!(p.warnings().is_empty(), "a fresh parse clears prior warnings");
    }

    #[test]
    fn recover_folds_warnings_into_diagnostics() {
        let mut p = Parser::new(VALID).unwrap();
        p.seed_warning = Some("char(n) is deprecated".into());
        let parsed = p.parse_recover();

        assert_eq!(parsed.diagnostics.len(), 1, "{:?}", parsed.diagnostics);
        assert!(parsed.diagnostics[0].is_warning());
        assert!(
            p.warnings().is_empty(),
            "the fold drains the buffer — one channel per entry point, not two"
        );
    }

    #[test]
    fn recover_keeps_warnings_and_errors_distinguishable() {
        let mut p = Parser::new("User {\n  id: +uuid\n  posts: [Post]\n}\n").unwrap();
        p.seed_warning = Some("char(n) is deprecated".into());
        let parsed = p.parse_recover();

        let errors: Vec<_> = parsed.diagnostics.iter().filter(|d| !d.is_warning()).collect();
        let warnings: Vec<_> = parsed.diagnostics.iter().filter(|d| d.is_warning()).collect();

        assert!(!errors.is_empty(), "the dangling relation is still fatal");
        assert_eq!(warnings.len(), 1, "the deprecation is present and non-fatal");
        assert!(
            errors
                .iter()
                .all(|e| e.severity == forgedb_validation::Severity::Error),
            "errors keep the default severity"
        );
    }

    #[test]
    fn fail_fast_parse_surfaces_a_producer_warning() {
        let mut p = Parser::new(VALID).unwrap();
        p.seed_warning = Some("@length(N) now means exactly N".into());
        assert!(p.parse().is_ok(), "a warning never fails a parse");
        assert_eq!(p.warnings().len(), 1);
        assert!(p.warnings()[0].is_warning());
    }

    #[test]
    fn fail_fast_parse_does_not_fail_on_a_validate_schema_warning() {
        let mut p = Parser::new("Thing {\n  id: &+uuid\n  name: string\n}\n").unwrap();
        let schema = p.parse().expect("a redundant modifier is valid, not an error");
        assert_eq!(schema.models.len(), 1);

        let warnings = p.take_warnings();
        assert_eq!(warnings.len(), 1, "surfaced, not dropped: {warnings:?}");
        assert!(warnings[0].is_warning());
        assert!(warnings[0].message.contains("has no effect"));
    }

    #[test]
    fn fail_fast_parse_still_fails_on_an_error_beside_a_warning() {
        let mut p =
            Parser::new("Thing {\n  id: &+uuid\n}\n\nBad {\n  name: string\n}\n").unwrap();
        let err = p.parse().expect_err("the missing identity is still fatal");
        assert!(err.contains("no identity field"), "reported the error, not the warning: {err}");
    }

    fn parse_field(src: &str, field: &str) -> (FieldType, Vec<ValidationError>) {
        let mut p = Parser::new(src).unwrap();
        let schema = p.parse().expect("schema parses");
        let ty = schema
            .models
            .iter()
            .flat_map(|m| &m.fields)
            .find(|f| f.name == field)
            .unwrap_or_else(|| panic!("field `{field}` not found"))
            .field_type
            .clone();
        (ty, p.take_warnings())
    }

    #[test]
    fn bytes_parses_without_a_diagnostic() {
        let (ty, warnings) = parse_field("T {\n  id: +uuid\n  code: bytes(3)\n}\n", "code");
        assert_eq!(ty, FieldType::Bytes(3));
        assert!(warnings.is_empty(), "canonical spelling is silent: {warnings:?}");
    }

    #[test]
    fn char_parses_to_bytes_and_warns_once() {
        let (ty, warnings) = parse_field("T {\n  id: +uuid\n  code: char(3)\n}\n", "code");
        assert_eq!(ty, FieldType::Bytes(3), "same AST as `bytes(3)`");
        assert_eq!(warnings.len(), 1, "exactly one diagnostic: {warnings:?}");

        let w = &warnings[0];
        assert!(w.is_warning(), "a deprecation is never an error");
        assert_eq!(
            w.position.map(|p| (p.line, p.column)),
            Some((3, 9)),
            "anchored at the `char` keyword, not the size or the field name"
        );
        assert_eq!(
            w.suggestion.as_deref(),
            Some("bytes(3)"),
            "the suggestion carries the size, so a quick-fix can apply it verbatim"
        );
    }

    #[test]
    fn char_warns_in_every_type_position() {
        for (src_type, expected) in [
            ("char(2)?", FieldType::Nullable(Box::new(FieldType::Bytes(2)))),
            ("?char(4)", FieldType::Nullable(Box::new(FieldType::Bytes(4)))),
            (
                "[char(8); 2]",
                FieldType::FixedArray(Box::new(FieldType::Bytes(8)), 2),
            ),
            ("^&char(5)", FieldType::Bytes(5)),
        ] {
            let src = format!("T {{\n  id: +uuid\n  f: {src_type}\n}}\n");
            let (ty, warnings) = parse_field(&src, "f");
            assert_eq!(ty, expected, "`{src_type}` reaches the right AST");
            assert_eq!(warnings.len(), 1, "`{src_type}` warns exactly once");
        }
    }

    #[test]
    fn char_deprecation_preserves_modifiers() {
        let mut p = Parser::new("T {\n  id: +uuid\n  key: ^&char(5)\n}\n").unwrap();
        let schema = p.parse().expect("parses");
        let f = schema.models[0]
            .fields
            .iter()
            .find(|f| f.name == "key")
            .unwrap();
        assert!(f.indexed && f.unique, "`^` and `&` survive the warning");
        assert_eq!(p.warnings().len(), 1);
    }

    #[test]
    fn bytes_is_still_a_valid_field_name() {
        let (ty, warnings) = parse_field(
            "T {\n  id: +uuid\n  bytes: i32?\n  blob: bytes(4)\n}\n",
            "bytes",
        );
        assert_eq!(ty, FieldType::Nullable(Box::new(FieldType::I32)));
        assert!(warnings.is_empty());

        let (blob, _) = parse_field(
            "T {\n  id: +uuid\n  bytes: i32?\n  blob: bytes(4)\n}\n",
            "blob",
        );
        assert_eq!(
            blob,
            FieldType::Bytes(4),
            "the type still resolves in the same schema as the field name"
        );
    }

    #[test]
    fn bare_bytes_without_a_size_is_not_the_type() {
        let mut p = Parser::new("T {\n  id: +uuid\n  f: bytes\n}\n").unwrap();
        assert!(p.parse().is_err(), "a sizeless `bytes` is not a type");
    }

    #[test]
    fn string_n_parses_to_the_inexact_variant() {
        let (ty, warnings) = parse_field("T {\n  id: +uuid\n  slug: string(64)\n}\n", "slug");
        assert_eq!(ty, FieldType::StringN { chars: 64, exact: false });
        assert!(warnings.is_empty(), "no diagnostic for a well-formed width: {warnings:?}");
    }

    #[test]
    fn string_n_bang_parses_to_the_exact_variant() {
        let (ty, warnings) = parse_field("T {\n  id: +uuid\n  code: string(26!)\n}\n", "code");
        assert_eq!(ty, FieldType::StringN { chars: 26, exact: true });
        assert!(warnings.is_empty(), "no diagnostic: {warnings:?}");
    }

    #[test]
    fn string_n_width_is_bounded_at_the_parse() {
        for src in ["string(0)", "string(256)", "string(1000)", "string(0!)", "string(256!)"] {
            let schema = format!("T {{\n  id: +uuid\n  f: {src}\n}}\n");
            let mut p = Parser::new(&schema).unwrap();
            let err = p.parse().expect_err("`{src}` must not parse");
            assert!(
                err.contains("between 1 and 255"),
                "`{src}` names the admissible range, got: {err}"
            );
        }
    }

    #[test]
    fn string_n_rejects_a_negative_width() {
        let mut p = Parser::new("T {\n  id: +uuid\n  f: string(-1)\n}\n").unwrap();
        let err = p.parse().expect_err("a negative width must not parse");
        assert!(err.contains("between 1 and 255"), "got: {err}");
    }

    #[test]
    fn bare_string_is_unchanged() {
        for (src_type, expected) in [
            ("string", FieldType::String),
            ("string?", FieldType::Nullable(Box::new(FieldType::String))),
            ("?string", FieldType::Nullable(Box::new(FieldType::String))),
        ] {
            let src = format!("T {{\n  id: +uuid\n  f: {src_type}\n}}\n");
            let (ty, warnings) = parse_field(&src, "f");
            assert_eq!(ty, expected, "`{src_type}` is still bare `string`");
            assert!(warnings.is_empty());
        }
    }

    #[test]
    fn string_n_parses_in_every_type_position() {
        for (src_type, expected) in [
            ("string(8)?", FieldType::Nullable(Box::new(FieldType::StringN { chars: 8, exact: false }))),
            ("?string(8)", FieldType::Nullable(Box::new(FieldType::StringN { chars: 8, exact: false }))),
            ("string(4!)?", FieldType::Nullable(Box::new(FieldType::StringN { chars: 4, exact: true }))),
            ("^&string(5!)", FieldType::StringN { chars: 5, exact: true }),
        ] {
            let src = format!("T {{\n  id: +uuid\n  f: {src_type}\n}}\n");
            let (ty, warnings) = parse_field(&src, "f");
            assert_eq!(ty, expected, "`{src_type}` reaches the right AST");
            assert!(warnings.is_empty(), "`{src_type}`: {warnings:?}");
        }
    }

    #[test]
    fn a_stray_bang_is_a_parse_error() {
        let mut p = Parser::new("T {\n  id: +uuid\n  f: !string\n}\n").unwrap();
        assert!(p.parse().is_err(), "`!` outside `string(N!)` is not a modifier");

        let mut p = Parser::new("T {\n  id: +uuid\n  f: bytes(3!)\n}\n").unwrap();
        assert!(p.parse().is_err(), "`!` is not admitted by `bytes(N)`");
    }

    #[test]
    fn a_bare_timestamp_is_millis() {
        let (ty, warnings) = parse_field("T {\n  id: +uuid\n  at: timestamp\n}\n", "at");
        assert_eq!(ty, FieldType::Timestamp(TimestampPrecision::Millis));
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn each_precision_key_parses() {
        for (key, expected) in [
            ("s", TimestampPrecision::Seconds),
            ("ms", TimestampPrecision::Millis),
            ("us", TimestampPrecision::Micros),
        ] {
            let src = format!("T {{\n  id: +uuid\n  at: timestamp({key})\n}}\n");
            let (ty, warnings) = parse_field(&src, "at");
            assert_eq!(ty, FieldType::Timestamp(expected), "timestamp({key})");
            assert!(warnings.is_empty(), "timestamp({key}): {warnings:?}");
        }
    }

    #[test]
    fn an_unknown_precision_key_names_the_three_that_exist() {
        for bad in ["ns", "seconds", "millis", "S", "MS", "us2", "micro"] {
            let src = format!("T {{\n  id: +uuid\n  at: timestamp({bad})\n}}\n");
            let mut p = Parser::new(&src).unwrap();
            let err = p.parse().expect_err("timestamp({bad}) must not parse");
            assert!(
                err.contains("`s`") && err.contains("`ms`") && err.contains("`us`"),
                "timestamp({bad}) must name the three admissible keys, got: {err}"
            );
        }
        for src_type in ["timestamp()", "timestamp(1)", "timestamp(us!)"] {
            let src = format!("T {{\n  id: +uuid\n  at: {src_type}\n}}\n");
            let mut p = Parser::new(&src).unwrap();
            assert!(p.parse().is_err(), "`{src_type}` must not parse");
        }
    }

    #[test]
    fn precision_survives_every_type_position() {
        for (src_type, expected) in [
            (
                "timestamp(us)?",
                FieldType::Nullable(Box::new(FieldType::Timestamp(TimestampPrecision::Micros))),
            ),
            (
                "?timestamp(s)",
                FieldType::Nullable(Box::new(FieldType::Timestamp(TimestampPrecision::Seconds))),
            ),
            (
                "^timestamp(us)",
                FieldType::Timestamp(TimestampPrecision::Micros),
            ),
            (
                "[timestamp(us); 3]",
                FieldType::FixedArray(
                    Box::new(FieldType::Timestamp(TimestampPrecision::Micros)),
                    3,
                ),
            ),
        ] {
            let src = format!("T {{\n  id: +uuid\n  f: {src_type}\n}}\n");
            let (ty, warnings) = parse_field(&src, "f");
            assert_eq!(ty, expected, "`{src_type}` reaches the right AST");
            assert!(warnings.is_empty(), "`{src_type}`: {warnings:?}");
        }
    }
}
