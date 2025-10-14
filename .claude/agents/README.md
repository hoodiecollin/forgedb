# SinkDB Claude Agents

This directory contains specialized Claude Code sub-agents for the SinkDB project. Each agent is an expert in a specific aspect of SinkDB development.

## Available Agents

### 1. Schema Specialist (`schema-specialist.md`)
**Expertise**: SinkDB schema language design and optimization

**Use when:**
- Designing new schema models
- Validating schema syntax and semantics
- Optimizing schema for columnar storage
- Planning schema migrations
- Understanding type system and constraints

**Example prompts:**
- "Help me design a schema for a social media platform"
- "Is this inline struct declaration valid?"
- "What's the best type for storing email addresses?"
- "How should I model this many-to-many relationship?"

---

### 2. Storage Architect (`storage-architect.md`)
**Expertise**: Columnar storage engine design and performance

**Use when:**
- Optimizing storage layout
- Debugging performance issues
- Designing indexing strategies
- Planning compaction and garbage collection
- Understanding query execution

**Example prompts:**
- "Why is my query slow on 1M rows?"
- "What index should I add for this query pattern?"
- "How does columnar storage work for this schema?"
- "Help me optimize memory usage"

---

### 3. Code Generation Specialist (`codegen-specialist.md`)
**Expertise**: Schema transpilation and code generation

**Use when:**
- Understanding the transpiler pipeline
- Debugging generated code
- Optimizing code generation
- Adding new codegen features
- Troubleshooting AST parsing

**Example prompts:**
- "How does the parser handle inline structs?"
- "What Rust code is generated for this schema?"
- "How do I add validation to the transpiler?"
- "Why isn't my schema compiling?"

---

### 4. API Developer (`api-developer.md`)
**Expertise**: REST API generation and optimization

**Use when:**
- Designing API endpoints
- Understanding query parameters
- Debugging API issues
- Optimizing API performance
- Generating OpenAPI specs

**Example prompts:**
- "What endpoints are generated for this model?"
- "How do I filter by multiple fields?"
- "What's the best way to paginate large results?"
- "How do I add authentication to the API?"

---

### 5. Documentation Specialist (`documentation-specialist.md`)
**Expertise**: Technical documentation and tutorials

**Use when:**
- Writing schema documentation
- Creating API documentation
- Writing tutorials and guides
- Documenting code
- Creating examples

**Example prompts:**
- "Help me document this schema model"
- "Create API documentation for the User endpoints"
- "Write a tutorial for schema design"
- "Document this Rust function"

---

### 6. Test Engineer (`test-engineer.md`)
**Expertise**: Testing and performance benchmarking

**Use when:**
- Writing unit tests
- Creating integration tests
- Designing benchmarks
- Debugging test failures
- Performance analysis

**Example prompts:**
- "Write tests for this storage function"
- "Create a benchmark for columnar scanning"
- "How do I test schema validation?"
- "What should I test for this API endpoint?"

---

## How to Use Agents

### Option 1: Direct Invocation (Recommended)
Ask Claude Code to use a specific agent:

```
"Use the schema specialist agent to help me design a blog schema"
"Have the storage architect review my indexing strategy"
"Ask the test engineer to write benchmarks for this function"
```

### Option 2: Context Switching
If Claude Code detects you need specialized help, it may suggest switching to an appropriate agent.

### Option 3: Manual Reference
Read the agent files directly to understand their expertise and use that knowledge when asking questions.

---

## Agent Capabilities

All agents have deep knowledge of:
- SinkDB documentation (README, INDEX, ROADMAP, etc.)
- Domain-specific expertise
- Best practices and patterns
- Common pitfalls and solutions

Each agent can:
- Answer questions in their domain
- Provide code examples (inline, not as separate files)
- Suggest optimizations
- Debug issues
- Create documentation

## Important Guidelines

**Examples and Demos:**
- Agents should NOT automatically create example files or demo applications
- Examples should only be created when explicitly requested by the user
- Focus on tests, implementation, and documentation instead
- If examples are needed, ask the user first

---

## Project Structure Reference

```
kitchen-sink/
├── README.md                  # Project overview
├── INDEX.md                   # Documentation index
├── ROADMAP.md                 # Development roadmap
├── DSL_SPECIFICATION.md       # Schema language spec
├── STORAGE_ARCHITECTURE.md    # Storage engine design
├── API_GENERATION.md          # API generation spec
├── CLI_SPECIFICATION.md       # CLI tool spec
├── EXAMPLES.md                # Example applications
├── ADVANCED_FEATURES.md       # Future features
├── COMPLETE_INTEGRATION.md    # Full-stack integration
└── .claude/
    └── agents/                # This directory
        ├── schema-specialist.md
        ├── storage-architect.md
        ├── codegen-specialist.md
        ├── api-developer.md
        ├── documentation-specialist.md
        └── test-engineer.md
```

---

## Tips for Best Results

1. **Be specific**: The more context you provide, the better the agent can help
2. **Reference docs**: Agents know all project documentation
3. **Ask follow-ups**: Agents can dive deeper into topics
4. **Request examples**: Agents can provide concrete code examples
5. **Combine agents**: Different agents can work together on complex tasks

---

## Agent Maintenance

These agents are based on the SinkDB documentation as of **October 11, 2025**.

If project documentation changes significantly, agents may need updates to reflect:
- New schema syntax
- Changed storage architecture
- Updated API patterns
- New performance targets

---

## Questions?

If you're unsure which agent to use:
1. Start with a general question to Claude Code
2. Claude may suggest an appropriate agent
3. Or consult this README to find the right specialist

For questions about the agents themselves, ask the **Documentation Specialist**.

---

**Last Updated**: October 11, 2025
**Project Status**: Design Phase
