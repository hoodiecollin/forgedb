# Sprint 17: UI Component Integration - Task Breakdown

**Branch**: `sprint-17/ui-component-integration`
**Status**: In Progress
**Created**: 2025-10-14

---

## Task Overview

Sprint 17 is broken into 6 main phases with 24 discrete tasks. Each task is designed to be completable in a single session.

---

## Phase 1: Core Parser & AST (Tasks 1-4)

### ✅ Task 1: Add Component Field Type to AST
**Estimated**: 30 min | **Status**: Not Started

**Files to modify**:
- `src/ast.rs`

**Changes**:
```rust
// Add to FieldType enum
#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    // ... existing types
    Component(ComponentReference),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComponentReference {
    pub protocol: ComponentProtocol,
    pub path: String,
    pub relations: RelationInclusion,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ComponentProtocol {
    Tsx,  // tsx://
    Jsx,  // jsx://
    Api,  // api://
}

#[derive(Debug, Clone, PartialEq)]
pub enum RelationInclusion {
    None,
    All,
    Specific(Vec<String>),
}
```

**Tests**:
- Unit tests for ComponentReference creation
- Enum variant tests

---

### Task 2: Add Component Field Parsing (Basic)
**Estimated**: 45 min | **Status**: Not Started

**Files to modify**:
- `src/parser.rs`
- `src/lexer.rs` (add `://` token support)

**Changes**:
1. Add `Token::DoubleColon` to lexer
2. Parse component syntax: `field_name: protocol://path`
3. Return `FieldType::Component` for component fields

**Example parsing**:
```forge
User {
    card: tsx://components/UserCard
}
```

**Tests**:
- Parse tsx:// protocol
- Parse jsx:// protocol
- Parse api:// protocol
- Parse path with subdirectories
- Error on invalid protocol

---

### Task 3: Add @relations Directive Support
**Estimated**: 30 min | **Status**: Not Started

**Files to modify**:
- `src/parser.rs`

**Changes**:
1. Parse `@relations(*)` for all relations
2. Parse `@relations(field1, field2)` for specific relations
3. Store in ComponentReference.relations

**Example**:
```forge
User {
    card: tsx://components/UserCard @relations(posts, comments)
    profile: tsx://components/UserProfile @relations(*)
}
```

**Tests**:
- Parse @relations(*)
- Parse @relations(field1)
- Parse @relations(field1, field2, field3)
- Error on invalid relation field

---

### Task 4: Add page.tsx Naming Convention
**Estimated**: 15 min | **Status**: Not Started

**Files to modify**:
- `src/parser.rs` or new `src/component_validator.rs`

**Changes**:
- Validate that TSX/JSX component paths should use `page.tsx` convention
- Store base directory for component

**Example**:
```forge
User {
    # Maps to: components/user/card/page.tsx
    card: tsx://components/user/card
}
```

**Tests**:
- Validate page.tsx path generation

---

## Phase 2: TypeScript Props Generation (Tasks 5-8)

### Task 5: Create Component Props Type Generator Module
**Estimated**: 45 min | **Status**: Not Started

**Files to create**:
- `src/codegen/typescript_component_props.rs`

**Changes**:
1. Create `ComponentPropsGenerator` struct
2. Implement `generate_props_types(schema: &Schema) -> String`
3. Generate `{Model}{Component}Props` interfaces

**Output example**:
```typescript
export type UserCardProps = {
    data: User;
    computed?: UserComputed;
    relations?: {
        posts?: Post[];
    };
};
```

**Tests**:
- Generate props with data only
- Generate props with computed fields
- Generate props with relations

---

### Task 6: Generate Props with Relations
**Estimated**: 30 min | **Status**: Not Started

**Files to modify**:
- `src/codegen/typescript_component_props.rs`

**Changes**:
1. Handle `@relations(*)` - include all relations
2. Handle `@relations(field1, ...)` - include specific relations
3. Generate optional relations object

**Tests**:
- Relations: all (*)
- Relations: specific fields
- Relations: none (default)

---

### Task 7: Integrate Props Generator into Codegen Pipeline
**Estimated**: 20 min | **Status**: Not Started

**Files to modify**:
- `src/codegen/mod.rs`
- `src/codegen/typescript.rs`

**Changes**:
1. Call props generator after model types
2. Write props to `generated/component-props.ts`
3. Export from main types file

**Tests**:
- Integration test with full schema

---

### Task 8: Add Props Type Exports
**Estimated**: 15 min | **Status**: Not Started

**Files to modify**:
- `src/codegen/typescript.rs`

**Changes**:
- Export all component props types
- Add to main `index.ts`

---

## Phase 3: Component Stub Generation (Tasks 9-12)

### Task 9: Create Component Stub Generator Module
**Estimated**: 45 min | **Status**: Not Started

**Files to create**:
- `src/codegen/component_stubs.rs`

**Changes**:
1. Create `ComponentStubGenerator` struct
2. Generate React/TSX component stubs
3. Use `page.tsx` naming convention

**Output example**:
```tsx
// components/user/card/page.tsx
import { UserCardProps } from '../../../generated/component-props';

export default function UserCard({ data, computed, relations }: UserCardProps) {
    return (
        <div className="user-card">
            <h2>{data.email}</h2>
            {/* TODO: Implement component */}
        </div>
    );
}
```

**Tests**:
- Generate basic component stub
- Generate stub with relations
- Check file path

---

### Task 10: Add Detailed Stub Template Option
**Estimated**: 30 min | **Status**: Not Started

**Files to modify**:
- `src/codegen/component_stubs.rs`

**Changes**:
1. Add template option (minimal vs. detailed)
2. Detailed: render all fields
3. Detailed: render relations with map

**Tests**:
- Generate minimal stub
- Generate detailed stub

---

### Task 11: HTTP Route Handler Stub Generation
**Estimated**: 45 min | **Status**: Not Started

**Files to create/modify**:
- `src/codegen/route_handlers.rs`

**Changes**:
1. Generate route handler stubs for api:// fields
2. Filename = HTTP method (get.ts, post.ts, etc.)
3. Include TypeScript types

**Example**:
```forge
User {
    verify_email: api://routes/user/verify
}
```

Generates: `routes/user/verify/post.ts`
```typescript
import { NextRequest, NextResponse } from 'next/server';

export async function POST(req: NextRequest) {
    // TODO: Implement email verification
    return NextResponse.json({ message: "Not implemented" });
}
```

**Tests**:
- Generate GET handler
- Generate POST handler
- Generate PUT/DELETE handlers

---

### Task 12: Integrate Stub Generation into CLI
**Estimated**: 20 min | **Status**: Not Started

**Files to modify**:
- `crates/cli/src/main.rs`

**Changes**:
1. Add component stub generation to `generate` command
2. Option: `--skip-stubs` to skip stub generation
3. Don't overwrite existing files

**Tests**:
- CLI generates stubs
- CLI skips existing files

---

## Phase 4: Bun Runtime (Tasks 13-16)

### Task 13: Create Bun Runtime Directory Structure
**Estimated**: 15 min | **Status**: Not Started

**Files to create**:
- `runtime/bun/package.json`
- `runtime/bun/tsconfig.json`
- `runtime/bun/server.ts`

**Changes**:
1. Initialize Bun project
2. Add necessary dependencies
3. Basic TypeScript configuration

---

### Task 14: Create Database Client (HTTP Fetch)
**Estimated**: 45 min | **Status**: Not Started

**Files to create**:
- `runtime/bun/db-client.ts`

**Changes**:
1. Create DB client using fetch to Rust API
2. CRUD operations (get, list, create, update, delete)
3. TypeScript types from generated types

**Example**:
```typescript
export class DBClient {
    constructor(private apiEndpoint: string) {}

    async get<T>(model: string, id: string): Promise<T> {
        const response = await fetch(
            `${this.apiEndpoint}/api/${model}/${id}`
        );
        return response.json();
    }
}
```

**Tests**:
- Mock fetch requests
- Test CRUD operations

---

### Task 15: Create Component Renderer
**Estimated**: 60 min | **Status**: Not Started

**Files to create**:
- `runtime/bun/renderer.ts`

**Changes**:
1. Dynamic import components
2. Render components with renderToReadableStream
3. Return HTML responses

**Example**:
```typescript
export async function renderComponent(
    componentPath: string,
    props: any
): Promise<string> {
    const Component = await import(componentPath);
    return await renderToString(<Component {...props} />);
}
```

**Tests**:
- Render basic component
- Render with props

---

### Task 16: Create Bun HTTP Server
**Estimated**: 45 min | **Status**: Not Started

**Files to modify**:
- `runtime/bun/server.ts`

**Changes**:
1. Create HTTP server on port 3001
2. Route `/components/:model/:id/:component` to renderer
3. Route `/api/*` handlers to dynamic imports

**Example routes**:
- `GET /components/users/123/card` → Renders UserCard
- `POST /api/users/verify-email` → Calls verify handler

**Tests**:
- Test component routing
- Test API handler routing

---

## Phase 5: Code Generator Updates (Tasks 17-20)

### Task 17: Update Storage Generator to Skip Components
**Estimated**: 30 min | **Status**: Not Started

**Files to modify**:
- `src/codegen/storage.rs`

**Changes**:
1. Filter out component fields from storage schema
2. Don't generate storage for component fields
3. Add comment in generated code

**Tests**:
- Component fields not in storage
- Storage tests still pass

---

### Task 18: Update CRUD API to Skip Components
**Estimated**: 20 min | **Status**: Not Started

**Files to modify**:
- `crates/crud-api/src/lib.rs`

**Changes**:
1. Skip component fields in API responses (or include metadata)
2. Don't accept component fields in create/update

**Tests**:
- Component fields excluded from API

---

### Task 19: Update TypeScript SDK with Components
**Estimated**: 30 min | **Status**: Not Started

**Files to modify**:
- `src/codegen/typescript.rs`

**Changes**:
1. Include component fields in model type (as metadata)
2. Mark as optional/null
3. Add JSDoc comments

**Example**:
```typescript
export type User = {
    id: string;
    email: string;
    // Component references (not stored)
    card?: null;
    profile?: null;
};
```

**Tests**:
- TypeScript types include components

---

### Task 20: Update OpenAPI Schema
**Estimated**: 20 min | **Status**: Not Started

**Files to modify**:
- `src/codegen/openapi.rs`

**Changes**:
1. Document component fields in schema
2. Mark as readOnly
3. Add component metadata

**Tests**:
- OpenAPI includes component docs

---

## Phase 6: Testing & Documentation (Tasks 21-24)

### Task 21: Add Parser Tests
**Estimated**: 45 min | **Status**: Not Started

**Files to create**:
- `tests/component_parsing_test.rs`

**Tests**:
- Parse basic component field
- Parse with @relations(*)
- Parse with @relations(field1, field2)
- Parse tsx://, jsx://, api://
- Error handling

---

### Task 22: Add Integration Test
**Estimated**: 60 min | **Status**: Not Started

**Files to create**:
- `tests/sprint17_integration_test.rs`

**Tests**:
1. Parse schema with components
2. Generate TypeScript props
3. Generate component stubs
4. Generate route handlers
5. Verify file structure

**Example schema**:
```forge
User {
    id: +uuid
    email: string
    posts: [Post]

    card: tsx://components/user/card @relations(posts)
    profile: tsx://components/user/profile @relations(*)
    verify_email: api://routes/user/verify
}

Post {
    id: +uuid
    title: string
    author: *User
}
```

---

### Task 23: Create Example Project
**Estimated**: 45 min | **Status**: Not Started

**Files to create**:
- `examples/component-integration/schema.forge`
- `examples/component-integration/README.md`

**Changes**:
1. Complete working example
2. Show component rendering
3. Show API handlers
4. Include Bun runtime setup

---

### Task 24: Update Documentation
**Estimated**: 30 min | **Status**: Not Started

**Files to modify**:
- `README.md`
- `docs/COMPONENT_INTEGRATION.md` (new)
- `SPRINT_PLAN.md`

**Changes**:
1. Document component field syntax
2. Document @relations directive
3. Document Bun runtime setup
4. Add examples

---

## Task Summary

**Total Tasks**: 24
**Estimated Total Time**: 13-15 hours
**Phases**: 6

### By Phase:
- Phase 1 (Parser/AST): 4 tasks, ~2 hours
- Phase 2 (Props Generation): 4 tasks, ~2 hours
- Phase 3 (Component Stubs): 4 tasks, ~2.5 hours
- Phase 4 (Bun Runtime): 4 tasks, ~3 hours
- Phase 5 (Codegen Updates): 4 tasks, ~2 hours
- Phase 6 (Testing/Docs): 4 tasks, ~3.5 hours

---

## Current Status

**Branch**: `sprint-17/ui-component-integration`
**Tasks Complete**: 0/24
**Current Task**: Task 1 - Add Component Field Type to AST

---

## Next Session

Start with **Phase 1: Tasks 1-4** (Parser & AST changes). These are foundational and required for all subsequent work.

**Command to continue**:
```bash
git checkout sprint-17/ui-component-integration
# Start with Task 1
```
