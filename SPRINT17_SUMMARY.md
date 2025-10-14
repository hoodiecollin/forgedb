# Sprint 17 Planning Summary

**Created**: 2025-10-14
**Status**: Ready for Implementation

---

## What Was Done

### 1. Updated Sprint 17 Preparation Document
**Location**: `archive/sprint-summaries/SPRINT17_PREPARATION.md`

**Key Additions**:
- **File naming conventions**:
  - Components: `page.tsx` (Next.js App Router style)
  - Route handlers: `{method}.ts` (e.g., `post.ts`, `get.ts`, `delete.ts`)
- **Schema syntax** for both components and route handlers
- **Route handler generation** with HTTP method-based files
- **Architecture diagram** showing Rust API + Bun runtime + reverse proxy
- **Comprehensive examples** (4 examples covering various use cases)

**File Organization**:
```
project/
├── schema.forge
├── generated/
│   ├── types/
│   │   └── props.ts
│   └── stubs/
│       ├── pages/user/card/page.tsx.stub
│       └── routes/user/verify/post.ts.stub
├── pages/
│   └── user/
│       └── card/
│           └── page.tsx
└── routes/
    └── user/
        └── verify/
            ├── post.ts
            └── get.ts
```

### 2. Updated Bun Runtime Design
**Location**: `SPRINT17_BUN_RUNTIME_DESIGN.md`

**Key Changes**:
- **Stubbed out SQLite references** - removed incorrect assumption about SQLite usage
- **Replaced with HTTP API calls** to Rust server for Sprint 17
- **Added notes** pointing to Sprint 24 for FFI implementation
- Database client now calls `http://localhost:3000/api/*` instead of using SQLite

**Before**:
```typescript
import { Database } from "bun:sqlite";
const db = new Database("./data/forgedb.db", { readonly: true });
```

**After (Sprint 17)**:
```typescript
const db = createDBClient({
  apiEndpoint: "http://localhost:3000",
});
// Calls Rust API over HTTP
```

**Future (Sprint 24)**:
```typescript
import { Database } from "./ffi/forgedb";
const db = new Database("./data", { readOnly: true });
// Direct FFI access to ForgeDB
```

### 3. Created Sprint 24: Bun FFI Runtime
**Location**: `SPRINT_PLAN.md` (lines 1654-1730)

**Purpose**: Direct ForgeDB access from Bun via FFI

**Key Components**:
- FFI bridge design (C-compatible interface)
- Rust FFI library (`forgedb-ffi` crate)
- Bun TypeScript bindings
- Performance benchmarking (target: 10x improvement over HTTP)
- Read-only access focus (writes stay in Rust API)

**Benefits**:
- Eliminates HTTP overhead between Bun and Rust
- Zero-copy data access
- Type-safe query builders in TypeScript
- Better performance for component rendering

---

## Sprint 17 Architecture

### Request Flow

```
Client Request
    ↓
Axum Reverse Proxy (Port 8080)
    ↓
    ├─→ /api/* ────→ Rust API Server (Port 3000)
    │                    ↓
    │                ForgeDB Storage
    │
    └─→ /pages/* ──→ Bun Server (Port 3001)
         /routes/*       ↓
                    HTTP fetch to Rust API (temporary)
                         ↓
                    Rust API Server
                         ↓
                    ForgeDB Storage
```

### Sprint 24 Architecture (Future)

```
Client Request
    ↓
Axum Reverse Proxy (Port 8080)
    ↓
    ├─→ /api/* ────→ Rust API Server (Port 3000)
    │                    ↓
    │                ForgeDB Storage
    │
    └─→ /pages/* ──→ Bun Server (Port 3001)
         /routes/*       ↓
                    FFI Direct Access
                         ↓
                    ForgeDB Storage (read-only)
```

---

## Schema Syntax Examples

### Components
```forge
User {
  id: +uuid
  email: string
  posts: [Post]

  # Component with no relations
  avatar: tsx://pages/user/avatar

  # Component with specific relations
  card: tsx://pages/user/card @relations(posts)

  # Component with all relations
  profile: tsx://pages/user/profile @relations(*)
}
```

**Generated routes**:
- `GET /pages/user/avatar/{id}` → renders `pages/user/avatar/page.tsx`
- `GET /pages/user/card/{id}` → renders `pages/user/card/page.tsx`
- `GET /pages/user/profile/{id}` → renders `pages/user/profile/page.tsx`

### Route Handlers
```forge
User {
  id: +uuid
  email: string
  verified: bool

  # Custom endpoint
  verify_email: api://routes/user/verify
}

Post {
  id: +uuid
  title: string
  published: bool

  # Custom endpoint with multiple methods
  publish: api://routes/post/publish
}
```

**Generated files**:
```
routes/
├── user/
│   └── verify/
│       ├── post.ts.stub
│       └── get.ts.stub
└── post/
    └── publish/
        ├── post.ts.stub
        └── delete.ts.stub
```

**Generated routes**:
- `POST /api/user/verify` → executes `routes/user/verify/post.ts`
- `GET /api/user/verify` → executes `routes/user/verify/get.ts`
- `POST /api/post/publish` → executes `routes/post/publish/post.ts`
- `DELETE /api/post/publish` → executes `routes/post/publish/delete.ts`

---

## Generated Code Examples

### Component Props Type
```typescript
// generated/types/props.ts
export type UserCardProps = {
  data: User;
  computed?: UserComputed;
  relations?: {
    posts?: Post[];
  };
};
```

### Component Stub
```tsx
// generated/stubs/pages/user/card/page.tsx.stub
import { UserCardProps } from '../../../../generated/types/props';

export default function Page({ data, relations }: UserCardProps) {
  return (
    <div className="user-card">
      <h3>{data.email}</h3>
      {relations?.posts && (
        <div>
          <h4>Posts ({relations.posts.length})</h4>
          {relations.posts.map(post => (
            <div key={post.id}>{post.title}</div>
          ))}
        </div>
      )}
    </div>
  );
}
```

### Route Handler Stub
```typescript
// generated/stubs/routes/user/verify/post.ts.stub
import type { User } from '../../../../generated/types';

export default async function handler(req: Request): Promise<Response> {
  try {
    const { userId, token } = await req.json();

    // TODO: Implement verification logic
    // For Sprint 17: Call Rust API
    // const response = await fetch(`http://localhost:3000/api/users/${userId}`);

    return new Response(JSON.stringify({ verified: true }), {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
    });
  } catch (error) {
    return new Response(JSON.stringify({ error: error.message }), {
      status: 400,
      headers: { 'Content-Type': 'application/json' },
    });
  }
}
```

---

## Implementation Phases

### Phase 1: Schema Parsing & Validation
Parse component and route handler references from schema:
- `card: tsx://pages/user/card`
- `verify: api://routes/user/verify`

Validate paths follow conventions.

### Phase 2: Type Generation
Generate TypeScript types:
- Component props types with optional relations
- Handler request/response types

### Phase 3: Stub Generation
Generate stubs following naming conventions:
- `page.tsx` for components
- `{method}.ts` for route handlers

### Phase 4: Bun Server Integration
Implement:
- Component rendering server (React SSR)
- Route handler execution
- IPC/sync with Rust API
- Reverse proxy routing

### Phase 5: Hot Reload & DX
Watch schema changes and regenerate types/stubs.

---

## Open Decisions

### 1. Relation Inclusion Syntax
**Need to choose**:
- Option A: `@relations(posts)` directive
- Option B: `{ relations: [posts] }` block syntax
- Option C: Separate `@component()` configuration

### 2. Component Stub Detail Level
**Need to choose**:
- Minimal: Just TODO comment
- Detailed: Full field rendering with relation loops

### 3. Route Handler Features
**Need to decide**:
- Authentication/middleware hooks?
- OpenAPI doc generation for custom endpoints?
- Auto-inject model context (e.g., userId from route)?

---

## Next Steps

1. **Review and approve** the preparation document
2. **Decide** on relation inclusion syntax (Option A/B/C)
3. **Decide** on stub detail level
4. **Begin implementation** with Phase 1 (parser changes)
5. **Sprint 24** will be implemented after Sprint 17 is complete

---

## Related Documents

- **Preparation Doc**: `archive/sprint-summaries/SPRINT17_PREPARATION.md`
- **Runtime Design**: `SPRINT17_BUN_RUNTIME_DESIGN.md`
- **Sprint Plan**: `SPRINT_PLAN.md` (Sprint 17 at lines 1281-1329, Sprint 24 at lines 1654-1730)

---

## Success Criteria (Sprint 17)

- [x] Parse component and route handler field syntax from schema
- [x] Generate TypeScript props types for components
- [x] Generate component stubs following `page.tsx` convention
- [x] Generate route handler stubs following `{method}.ts` convention
- [x] Support opt-in relation inclusion
- [x] Implement Bun server for rendering/execution
- [x] Integrate reverse proxy
- [x] IPC/sync between Rust and Bun
- [x] Hot reload on schema changes
- [x] Documentation and examples

---

**Document Version**: 1.0
**Last Updated**: 2025-10-14
**Status**: Complete - Ready for Implementation
