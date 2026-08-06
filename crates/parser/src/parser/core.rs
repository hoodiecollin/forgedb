use crate::ast::{
    ComponentProtocol, ComponentReference, CompositeIndex, Constraint, ConstraintParam, EnumDef,
    Field, FieldType, Model, Projection, RelationInclusion, RelationType, Schema, Struct,
};
use crate::lexer::{Lexer, Token, TokenWithPos};
use forgedb_validation::{Position, ValidationError};

/// The result of a resilient parse ([`Parser::parse_recover`]): a best-effort
/// (possibly partial) [`Schema`] plus every diagnostic collected along the way —
/// syntax errors recovered from during parsing *and* the semantic diagnostics
/// from [`crate::validate::validate_schema`] — each positioned and sorted by
/// source location. This is the shape the LSP consumes to report diagnostics and
/// offer symbols on a buffer that does not fully parse.
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
    /// When set, the field/member loops record a diagnostic and skip to the next
    /// boundary instead of aborting the parse (see [`Parser::parse_recover`]).
    /// The fail-fast [`Parser::parse`]/[`Parser::parse_unvalidated`] paths leave
    /// this `false`, so their behavior is unchanged.
    recovering: bool,
    /// Syntax diagnostics accumulated during a recovering parse.
    recovery_diagnostics: Vec<ValidationError>,
    /// Non-fatal diagnostics accumulated during **any** parse (#237).
    ///
    /// Deliberately separate from `recovery_diagnostics`, which only fills when
    /// `recovering` is set. Warnings must reach the fail-fast
    /// [`Parser::parse`]/[`Parser::parse_unvalidated`] paths too — `forgedb
    /// generate` is the most-run command and therefore where a deprecation most
    /// needs to be seen, and it must not be pushed onto `parse_recover` to get
    /// one (that would change its error semantics from abort-on-first to
    /// recover-and-continue).
    ///
    /// Read with [`Parser::warnings`] / [`Parser::take_warnings`].
    warnings: Vec<ValidationError>,
    /// Test-only producer seam (#237). The channel ships with **no** emitters —
    /// #233 and #235 are the first — so without this there is no way to drive a
    /// warning through a real parse and the `warnings → ParsedSchema::diagnostics`
    /// fold (the path the LSP uses) would land unguarded. Emitted immediately
    /// after each parse entry point clears its buffers, exactly where a real
    /// producer inside the parse would land.
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

    /// Emit the test-only seeded warning, if any (#237). No-op in release builds.
    #[inline]
    fn emit_seeded_warning(&mut self) {
        #[cfg(test)]
        if let Some(message) = self.seed_warning.clone() {
            self.warn(message, None, None);
        }
    }

    /// The non-fatal diagnostics collected by the most recent parse (#237).
    ///
    /// Populated by every parse entry point, including the fail-fast ones, so a
    /// caller that used [`Self::parse`] can still surface deprecations. Empty
    /// until a producer emits one — this channel ships with no emitters.
    pub fn warnings(&self) -> &[ValidationError] {
        &self.warnings
    }

    /// Take the accumulated warnings, leaving the parser's buffer empty (#237).
    pub fn take_warnings(&mut self) -> Vec<ValidationError> {
        std::mem::take(&mut self.warnings)
    }

    /// Record a non-fatal diagnostic (#237).
    ///
    /// The producer-side entry point for schema-language deprecations. Never
    /// aborts a parse and never contributes to an exit code; the diagnostic
    /// travels alongside a successful result.
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

    /// Position of the token at `idx` (used to anchor a recovered diagnostic at
    /// the start of the member/declaration it came from, not wherever the cursor
    /// happened to stop).
    fn position_at(&self, idx: usize) -> Option<Position> {
        self.tokens_with_pos.get(idx).map(|t| t.position)
    }

    /// Build a positioned diagnostic from a parse-error message.
    fn diag(message: String, position: Option<Position>) -> ValidationError {
        let err = ValidationError::new(message);
        match position {
            Some(p) => err.with_position(p),
            None => err,
        }
    }

    /// Recover from a bad member (field or model directive) by skipping to the
    /// next member boundary. A member is a single line — it never spans a newline
    /// or contains braces — so the next `Newline`/`}`/EOF is a safe resync point.
    /// The closing `}`/EOF is left unconsumed so the enclosing loop can see it.
    fn recover_to_member_boundary(&mut self) {
        while !matches!(
            self.current_token(),
            Token::Newline | Token::RBrace | Token::Eof
        ) {
            self.advance();
        }
        self.skip_newlines();
    }

    /// Is the next significant token at or after `idx` (skipping newlines) an
    /// opening brace? Used to spot a bare model header (`Name {`) — the one
    /// top-level construct with no leading keyword — during recovery.
    fn next_significant_is_lbrace(&self, idx: usize) -> bool {
        let mut i = idx;
        while matches!(self.tokens.get(i), Some(Token::Newline)) {
            i += 1;
        }
        matches!(self.tokens.get(i), Some(Token::LBrace))
    }

    /// Recover from a bad *declaration* by skipping the whole broken block. From
    /// the declaration's start token, scan forward: if this declaration has an
    /// opening `{`, consume through its balanced closing `}` (to EOF if
    /// unterminated); otherwise — a header with no brace — stop at the next clear
    /// declaration boundary (a `struct`/`enum` keyword, or a bare `Name {` model
    /// header) so a following valid declaration is not swallowed. Always advances
    /// past `start`, guaranteeing top-level progress.
    fn synchronize_from(&mut self, start: usize) {
        let n = self.tokens.len();
        let mut i = start;

        while i < n {
            match &self.tokens[i] {
                Token::LBrace => {
                    // This declaration's block: balance from here through its match.
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
                    self.position = n; // unterminated — consume to the end
                    return;
                }
                Token::Eof => {
                    self.position = i;
                    return;
                }
                // A clear next-declaration boundary (past our own start token):
                // stop here rather than scanning into it for a brace.
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

    /// Record a recovered syntax diagnostic anchored at token `at`.
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

    /// The token after the cursor.
    fn peek_token(&self) -> &Token {
        self.tokens.get(self.position + 1).unwrap_or(&Token::Eof)
    }

    /// Is the cursor sitting on the contextual `bytes` type keyword (#233)?
    ///
    /// `bytes` is deliberately **not** a reserved word. Adding one would be a
    /// larger break than the rename it serves: `bytes` is an ordinary noun for a
    /// size, and reserving it turns `bytes: i32?` — a real column in the
    /// Chinook-derived `music-store` example — into a parse error on a *minor*
    /// version bump. So it lexes as [`Token::Ident`] and only means the type in
    /// type position followed by `(`, the same lookahead trick the `tsx://` /
    /// `jsx://` / `api://` component protocols already use.
    ///
    /// (The other type keywords — `string`, `timestamp`, `decimal`, … — *are*
    /// reserved and cannot be field names. That is pre-existing and tracked
    /// separately; this rename does not add to the problem.)
    fn at_bytes_type(&self) -> bool {
        matches!(self.current_token(), Token::Ident(name) if name == "bytes")
            && matches!(self.peek_token(), Token::LParen)
    }

    /// Consume `bytes ( N )` / `char ( N )` and return [`FieldType::Bytes`].
    ///
    /// The cursor must be on the type keyword. Emits the #233 deprecation warning
    /// when the source spelling was `char`.
    fn parse_bytes_type(&mut self) -> Result<FieldType, String> {
        let deprecated = matches!(self.current_token(), Token::TypeCharDeprecated);
        // Anchor the diagnostic at the keyword, not wherever the size parse ends.
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

        // Parse constraint name.  Anchor any diagnostic at the directive name, not
        // wherever the parameter list happens to end (#235).
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
                    Token::Fractional(s) => {
                        constraint =
                            constraint.with_param(ConstraintParam::Fractional(s.clone()));
                        self.advance();
                    }
                    // `>n` / `<n` — an exclusive bound (#239).  The operator is
                    // structural here; whether this directive and this field type
                    // may carry one is a semantic question, checked in `validate`.
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
                        // `name: value` (#235).  The colon-in-directive grammar
                        // already exists for `@projection(name: a, b)`, but that is
                        // parsed by a bespoke routine; this generalizes it to the
                        // shared parameter loop so any directive can take named args.
                        // Whether a given directive *accepts* them is a per-directive
                        // question, checked after the loop.
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

        if constraint.name == "length" {
            self.check_length_constraint(&constraint, constraint_position)?;
        }

        Ok(constraint)
    }

    /// Validate `@length`'s arguments and warn on the single-arg meaning change
    /// (#235).
    ///
    /// This lives in the parser because it is where the argument *names* are known.
    /// The codegen side filters parameters it does not recognize, so an unchecked
    /// `@length(foo: 3)` would silently produce a field with no bound at all — a
    /// constraint the schema declares and the database does not enforce. Rejecting
    /// it here is the difference between a typo being caught and being ignored.
    ///
    /// The accepted surface:
    ///
    /// | spelling | meaning |
    /// |---|---|
    /// | `@length(min: n)` | at least n |
    /// | `@length(max: n)` | at most n |
    /// | `@length(min: a, max: b)` | between a and b |
    /// | `@length(a, b)` | between a and b — unchanged |
    /// | `@length(n)` | **exactly** n — changed from "at most n" |
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
            // Positional.  Only the single-arg form changed meaning; the pair is
            // kept first-class and says nothing.
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
        // `bytes(N)` is contextual and arrives as an identifier, so it has to be
        // claimed before the `Token::Ident` arm below reads it as a struct name.
        if self.at_bytes_type() {
            return self.parse_bytes_type();
        }

        // Check for relation types first
        match self.current_token() {
            // Fixed array or One-to-many: [type; count] or [Post]
            Token::LBracket => {
                self.advance();

                // Check if this is a fixed array [type; count] or one-to-many [Model]
                let first_token = self.current_token().clone();

                // `[bytes(20); 5]` — claim the contextual keyword before the
                // `Token::Ident` arm treats it as a struct name (#233).
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
                // `?bytes(3)` — claim the contextual keyword before the
                // `Token::Ident` arm reads it as an optional model ref (#233).
                if self.at_bytes_type() {
                    let inner = self.parse_bytes_type()?;
                    return Ok(FieldType::Nullable(Box::new(inner)));
                }
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
            Token::TypeCharDeprecated => return self.parse_bytes_type(),
            // The contextual `bytes(N)` spelling arrives as an identifier (#233).
            Token::Ident(name) if name == "bytes" && matches!(self.peek_token(), Token::LParen) => {
                return self.parse_bytes_type()
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
                | FieldType::Bytes(_)
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

        // Expect closing brace.
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
            let member_start = self.position;
            match self.parse_model_member(
                &mut fields,
                &mut composite_indexes,
                &mut projections,
                &mut soft_delete,
            ) {
                Ok(()) => {}
                Err(e) if self.recovering => {
                    // Record the bad member, skip to the next line, keep going —
                    // one malformed field/directive should not blank out the rest
                    // of the model for the LSP.
                    self.recover_diag(e, member_start);
                    self.recover_to_member_boundary();
                    if self.position == member_start {
                        self.advance();
                    }
                }
                Err(e) => return Err(e),
            }
        }

        // Expect closing brace.
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
                // Keep the (empty) model so its symbol still exists for the LSP.
                self.recovery_diagnostics
                    .push(Self::diag(e, model_pos));
            } else {
                return Err(e);
            }
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

    /// Parse a single member of a model body — a field, or a model-level
    /// directive (`@index(...)`, `@projection(...)`, `@soft_delete`) — appending
    /// it to the relevant accumulator. Extracted from the `parse_model` loop so a
    /// recovering parse can catch a member's error and skip to the next line
    /// without duplicating the body logic.
    fn parse_model_member(
        &mut self,
        fields: &mut Vec<Field>,
        composite_indexes: &mut Vec<CompositeIndex>,
        projections: &mut Vec<Projection>,
        soft_delete: &mut bool,
    ) -> Result<(), String> {
        if matches!(self.current_token(), Token::At) {
            // Try to parse as a composite index first.
            let start_pos = self.position;
            match self.parse_directive() {
                Ok(composite_index) => {
                    composite_indexes.push(composite_index);
                    self.skip_newlines();
                }
                Err(_) => {
                    // Reset and try to parse as a model-level directive.
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

    /// Parse the input into a [`Schema`], running the full positioned semantic
    /// validation ([`crate::validate::validate_schema`]) and failing fast on the
    /// first **error** to preserve the historical `Result<Schema, String>`
    /// contract. Naming diagnostics are gated by the parser's `use_validation`
    /// flag (see [`Self::new_with_validation`]); structural/reference diagnostics
    /// always run.
    ///
    /// Non-fatal diagnostics (#237) do **not** fail the parse. They are moved into
    /// the warning buffer, so a caller on this fail-fast path still surfaces them
    /// via [`Self::warnings`] / [`Self::take_warnings`] — which is exactly what
    /// `forgedb generate` does.
    ///
    /// This partitioning is load-bearing (#258). `Severity` shipped with no
    /// emitters, so until the first `validate_schema` warning existed nothing
    /// exercised this path — and the old `errors.first()` would have turned that
    /// warning into a hard parse failure, which is a removal rather than a
    /// deprecation. Any new advisory must stay non-fatal here.
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
        // No error: keep the advisories on the warning channel rather than
        // dropping them. `parse_unvalidated` cleared the buffer for this run.
        self.warnings.extend(errors.into_iter().filter(|d| d.is_warning()));

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
    /// mid-keystroke buffers is #173.)
    pub fn parse_unvalidated(&mut self) -> Result<Schema, String> {
        // Warnings belong to one parse run, not to the parser's lifetime (#237).
        self.warnings.clear();
        self.emit_seeded_warning();

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

    /// Resilient parse (#173): parse the input into a best-effort
    /// [`ParsedSchema`] — a partial [`Schema`] plus **all** diagnostics — instead
    /// of aborting on the first error. This is the entry point for the LSP, where
    /// a buffer is usually mid-edit and blanking every diagnostic/symbol on a
    /// single typo is unacceptable.
    ///
    /// Recovery is two-tier: a malformed field or model directive is recorded and
    /// skipped to the next line (the rest of its model still parses); a malformed
    /// *declaration* is recorded and skipped as a whole balanced block (the rest
    /// of the file still parses). After the partial AST is assembled it is run
    /// through [`crate::validate::validate_schema`], so the returned diagnostics
    /// contain both recovered syntax errors and every semantic error, positioned
    /// and sorted by source location.
    ///
    /// Unlike [`Self::parse`], this always succeeds — an empty or unparseable
    /// buffer yields an empty schema and (possibly) diagnostics rather than an
    /// error. (Lexer errors are still fatal at [`Self::new`]; the LSP surfaces
    /// those as a single diagnostic.)
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

            // Guarantee forward progress even if a parse returned without
            // consuming and recovery could not advance.
            if self.position == decl_start {
                self.advance();
            }
            self.skip_newlines();
        }

        // Resolve enum field-type references over whatever models we recovered.
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
        // Fold in any non-fatal diagnostics (#237). `ParsedSchema::diagnostics` is
        // the single channel this entry point reports through — the LSP and
        // `forgedb validate` both read it — so warnings travel here rather than in
        // the `warnings` buffer that the fail-fast paths expose. Consumers MUST
        // partition by `severity` instead of testing the list for emptiness.
        diagnostics.extend(self.take_warnings());
        // Present diagnostics in source order (unpositioned ones last).
        diagnostics.sort_by_key(|d| {
            d.position
                .map(|p| (p.line, p.column))
                .unwrap_or((usize::MAX, usize::MAX))
        });

        self.recovering = prev_recovering;
        ParsedSchema { schema, diagnostics }
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

    // ---- #235: named `@length` arguments -------------------------------------
    //
    // The accepted surface (decided on the issue 2026-08-03):
    //
    //   @length(min: 1)          min only  — inexpressible before this
    //   @length(max: 20)         max only  — the new spelling for the old @length(20)
    //   @length(min: 3, max: 5)  both
    //   @length(3, 5)            min 3, max 5 — UNCHANGED, kept first-class
    //   @length(3)               EXACT (min = max = 3) — a BREAKING meaning change
    //
    // The single-arg change is silent at every other layer — it still parses, still
    // compiles, and only shows up as a 422 at write time — so the warning is the
    // only signal a reader gets. That is why it is asserted here, not just the shape.

    /// The named form reaches the AST as `Named`, in each combination.
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
            // Order is preserved but not required — `max:` first is equally valid.
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

    /// Two-arg positional is kept first-class, not deprecated: same AST as before,
    /// and no warning.
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

    /// Single-arg `@length(N)` changed meaning from `max: N` to exact `N`. It still
    /// parses, so the warning is the only thing that tells a reader their field's
    /// validation just narrowed.
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
        // Both replacement spellings must be named: one to keep the old behavior,
        // one to adopt the new. A warning that only says "this changed" leaves the
        // reader to guess which way to go.
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

    /// The named form is validated at parse time, where the argument names are
    /// known. Each of these would otherwise be silently dropped by the codegen
    /// filter and produce a field with no bound at all.
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

    /// The colon grammar must also work on the recovering path the LSP uses — that
    /// path re-enters the same parameter loop, so a form that parses in one and not
    /// the other would show as a phantom editor diagnostic on valid source.
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

    // ---- #173: resilient parse (`parse_recover`) --------------------------

    fn recover(input: &str) -> ParsedSchema {
        Parser::new(input).unwrap().parse_recover()
    }

    /// A valid schema parses identically under recovery, with no diagnostics —
    /// parity with the fatal path.
    #[test]
    fn recover_valid_schema_matches_parse() {
        let input = "User {\n  id: +uuid\n  email: string\n}\nPost {\n  id: +uuid\n}\n";
        let recovered = recover(input);
        assert!(recovered.diagnostics.is_empty(), "no diagnostics: {:?}", recovered.diagnostics);
        assert_eq!(recovered.schema, Parser::new(input).unwrap().parse().unwrap());
    }

    /// Empty / whitespace-only input is not an error under recovery (unlike the
    /// fatal path, which rejects an empty schema).
    #[test]
    fn recover_empty_input_is_not_an_error() {
        let recovered = recover("\n  \n");
        assert!(recovered.diagnostics.is_empty());
        assert!(recovered.schema.models.is_empty());
    }

    /// Declaration-level recovery: a header with no opening brace is reported, and
    /// the valid declarations on either side of it both survive.
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

    /// Field-level recovery: one malformed field does not blank out its model —
    /// the surrounding good fields still parse, and the model is still produced.
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

    /// Recovered partial AST is still run through `validate_schema`, so semantic
    /// diagnostics (here a dangling relation) are merged with syntax diagnostics.
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

    /// Diagnostics come back ordered by source position.
    #[test]
    fn recover_diagnostics_are_sorted_by_position() {
        // Two dangling relations on different lines + a snake_case violation.
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

    /// A mid-keystroke buffer with an unterminated block terminates (no infinite
    /// loop) and yields a diagnostic rather than hanging or panicking.
    #[test]
    fn recover_handles_unterminated_block() {
        let recovered = recover("User {\n  id: +uuid\n  email:");
        assert!(!recovered.diagnostics.is_empty(), "unterminated buffer is diagnosed");
    }

    /// Multiple independent broken declarations each produce a diagnostic and the
    /// good one between them survives.
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

    // ---- #237: the deprecation-warning channel -------------------------------
    //
    // The channel ships with **no producers** — #233 (`char(n)` → `bytes(n)`) and
    // #235 (`@length(N)` becoming exact) are the first two. These tests drive
    // `Parser::warn` directly so the plumbing is guarded before either lands,
    // rather than being incidentally covered by whichever ships first.

    const VALID: &str = "User {\n  id: +uuid\n  email: string\n}\n";

    /// The channel is invisible until something emits: a schema that parsed
    /// cleanly before #237 still parses cleanly, with no warnings and no change in
    /// its `Result`. This is the regression that keeps the field additive.
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

    /// A warning emitted during a **fail-fast** parse survives to the caller. This
    /// is the case `forgedb generate` depends on: it must be able to report a
    /// deprecation without switching to `parse_recover` and thereby changing its
    /// error semantics.
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

    /// Warnings belong to one parse run. Re-parsing with the same `Parser` must not
    /// replay a previous run's deprecations, which would double-report them.
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

    /// A warning raised *during* a recovering parse reaches `ParsedSchema` on its
    /// own — this is the path the LSP and `forgedb validate` actually read, so the
    /// fold has to work without the caller merging anything by hand.
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

    /// A warning and a genuine error coexist in one `ParsedSchema` and stay
    /// separable by severity alone. This is what lets `validate` keep failing on
    /// real errors while a deprecation exits 0.
    #[test]
    fn recover_keeps_warnings_and_errors_distinguishable() {
        // `posts: [Post]` with no `Post` model is a fatal dangling-relation error.
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

    /// The same seam through the fail-fast path: a producer firing inside `parse`
    /// leaves its warning where `forgedb generate` reads it, and the parse still
    /// succeeds.
    #[test]
    fn fail_fast_parse_surfaces_a_producer_warning() {
        let mut p = Parser::new(VALID).unwrap();
        p.seed_warning = Some("@length(N) now means exactly N".into());
        assert!(p.parse().is_ok(), "a warning never fails a parse");
        assert_eq!(p.warnings().len(), 1);
        assert!(p.warnings()[0].is_warning());
    }

    /// A warning produced by `validate_schema` (not by the parser's own buffer)
    /// must also stay non-fatal on the fail-fast path (#258).
    ///
    /// `Severity` shipped with no emitters, so this path was never exercised: the
    /// old `errors.first()` took the first diagnostic of ANY severity, which would
    /// have turned the first semantic advisory into a hard parse failure — and
    /// `forgedb generate` parses through here, so every affected schema would have
    /// stopped generating. That is a removal, not a deprecation.
    #[test]
    fn fail_fast_parse_does_not_fail_on_a_validate_schema_warning() {
        // `&` on the identity is redundant → advisory (#258).
        let mut p = Parser::new("Thing {\n  id: &+uuid\n  name: string\n}\n").unwrap();
        let schema = p.parse().expect("a redundant modifier is valid, not an error");
        assert_eq!(schema.models.len(), 1);

        let warnings = p.take_warnings();
        assert_eq!(warnings.len(), 1, "surfaced, not dropped: {warnings:?}");
        assert!(warnings[0].is_warning());
        assert!(warnings[0].message.contains("has no effect"));
    }

    /// The partition keys on severity, not on position in the list: a real error
    /// still fails even when an advisory was collected first.
    #[test]
    fn fail_fast_parse_still_fails_on_an_error_beside_a_warning() {
        // `Thing` warns (redundant `&`); `Bad` is a hard error (no identity).
        let mut p =
            Parser::new("Thing {\n  id: &+uuid\n}\n\nBad {\n  name: string\n}\n").unwrap();
        let err = p.parse().expect_err("the missing identity is still fatal");
        assert!(err.contains("no identity field"), "reported the error, not the warning: {err}");
    }

    // ---- #233: `char(n)` → `bytes(n)` ---------------------------------------
    //
    // The first real producer on the #237 channel. Both spellings must reach the
    // *same* `FieldType::Bytes`, in every type position, with a warning on
    // exactly the deprecated one.

    /// Parse a single-model schema and return `(field_type, warnings)`.
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

    /// The canonical spelling parses clean — no diagnostic of any severity.
    #[test]
    fn bytes_parses_without_a_diagnostic() {
        let (ty, warnings) = parse_field("T {\n  id: +uuid\n  code: bytes(3)\n}\n", "code");
        assert_eq!(ty, FieldType::Bytes(3));
        assert!(warnings.is_empty(), "canonical spelling is silent: {warnings:?}");
    }

    /// The deprecated spelling reaches the identical AST, and warns exactly once
    /// — positioned at the `char` keyword, and naming the replacement.
    #[test]
    fn char_parses_to_bytes_and_warns_once() {
        // 1: T {   2:   id: +uuid   3:   code: char(3)
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

    /// The deprecation reaches *inside* the compound type positions, not just a
    /// bare field type. Each is a separate parse path in `parse_type`.
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

    /// `^` / `&` are parsed before the type, so a deprecation inside a modified
    /// field must not eat them.
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

    /// `bytes` is **contextual**, not reserved: it stays usable as a field name.
    /// The Chinook-derived `music-store` example has exactly this column, so
    /// reserving the word would have turned a valid schema into a parse error on
    /// a minor version bump.
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

    /// Bare `bytes` with no size is not the type — it falls through to the
    /// struct-reference path, exactly as any other unknown bare identifier does.
    /// This is what keeps the contextual keyword from swallowing real names.
    #[test]
    fn bare_bytes_without_a_size_is_not_the_type() {
        let mut p = Parser::new("T {\n  id: +uuid\n  f: bytes\n}\n").unwrap();
        // Unresolvable as a struct, so validation rejects it — the point is that
        // it was never parsed as `FieldType::Bytes`.
        assert!(p.parse().is_err(), "a sizeless `bytes` is not a type");
    }
}
