# Sprint 17: UI Component Integration - Preparation Document

**Status**: Planning & Discussion
**Created**: 2025-10-14
**Sprint Goal**: Connect database schemas to UI components with server-side rendering via Bun runtime

---

## Overview

Sprint 17 aims to bridge the gap between ForgeDB schemas and UI components by allowing developers to reference components and route handlers directly in their schema definitions. The system will generate TypeScript props types and component/handler stubs that are automatically synchronized with schema changes.

**Key Innovation**: Components and route handlers are referenced in the schema, with Bun runtime handling SSR and the Rust API server handling data operations, coordinated via a reverse proxy.

---

## Architecture Summary

```
┌─────────────┐
│   Client    │
└──────┬──────┘
       │
       ▼
┌─────────────────────────────┐
│  Axum Reverse Proxy         │
│  (Rust, Port 8080)          │
└──┬────────────────────────┬─┘
   │                        │
   │ /api/*                 │ /pages/*
   ▼                        ▼
┌────────────┐          ┌─────────────┐
│ Rust API   │          │ Bun Server  │
│ (Port 3000)│◄────────►│ (Port 3001) │
└────────────┘ IPC/Sync └─────────────┘
                           │
                           ▼ FFI (Sprint 24)
                        ┌──────────────┐
                        │ ForgeDB      │
                        │ Data Files   │
                        └──────────────┘
```

**Note**: Sprint 17 focuses on component/handler generation and Bun integration. Direct ForgeDB access via FFI from Bun is deferred to Sprint 24.

---

## Core Concept

### Schema Syntax (CONFIRMED)

Components and route handlers are referenced as fields in the schema:

```forge
User {
  id: +uuid
  email: string
  posts: [Post]

  # Component references (rendered via page.tsx)
  card: tsx://pages/user/card
  profile: tsx://pages/user/profile

  # Route handlers (HTTP method = filename)
  custom_endpoint: api://routes/user/verify
}
```

### File Naming Conventions (NEW - CONFIRMED)

#### Component Files: `page.tsx`
```
pages/
├── user/
│   ├── card/
│   │   └── page.tsx        # UserCard component
│   └── profile/
│       └── page.tsx        # UserProfile component
└── post/
    └── detail/
        └── page.tsx        # PostDetail component
```

**Route**: `GET /pages/user/card/{id}` → renders `pages/user/card/page.tsx`

#### Route Handler Files: HTTP method names
```
routes/
└── user/
    └── verify/
        ├── post.ts         # POST /api/user/verify
        ├── get.ts          # GET /api/user/verify
        └── delete.ts       # DELETE /api/user/verify
```

**Routes**:
- `POST /api/user/verify` → executes `routes/user/verify/post.ts`
- `GET /api/user/verify` → executes `routes/user/verify/get.ts`
- `DELETE /api/user/verify` → executes `routes/user/verify/delete.ts`

---

## Generated Output

### TypeScript Props Types

```typescript
// generated/types.ts
type UserCardProps = {
  data: User
  computed?: UserComputed
  relations?: {
    posts?: Post[]
  }
}
```

### Component Stub (page.tsx)

```tsx
// pages/user/card/page.tsx
import { UserCardProps } from '../../../generated/types';

export default function Page({ data, computed, relations }: UserCardProps) {
  return (
    <div className="user-card">
      <h2>{data.email}</h2>
      {/* TODO: Implement component */}
    </div>
  );
}
```

### Route Handler Stub (post.ts)

```typescript
// routes/user/verify/post.ts
import type { User } from '../../../generated/types';

export default async function handler(req: Request): Promise<Response> {
  const body = await req.json();

  // TODO: Implement verification logic
  // Note: Direct DB access via FFI will be available in Sprint 24
  // For now, call Rust API endpoints

  return new Response(JSON.stringify({ verified: true }), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  });
}
```

---

## Open Questions & Discussion Points

### 1. Relations in Props (CONFIRMED APPROACH)

**Decision**: Opt-in with support for all relations or partial relations.

**Implementation Ideas**:

#### Option A: Directive-based relation inclusion
```forge
User {
  id: +uuid
  email: string
  posts: [Post]
  liked_posts: [Post]

  # Include specific relations
  card: tsx://pages/user/card @relations(posts)

  # Include all relations
  profile: tsx://pages/user/profile @relations(*)

  # No relations (default)
  avatar: tsx://pages/user/avatar
}
```

#### Option B: Explicit relation blocks
```forge
User {
  id: +uuid
  email: string
  posts: [Post]
  liked_posts: [Post]

  card: tsx://pages/user/card {
    relations: [posts]
  }

  profile: tsx://pages/user/profile {
    relations: *
  }
}
```

#### Option C: Separate configuration
```forge
User {
  id: +uuid
  email: string
  posts: [Post]
  liked_posts: [Post]

  card: tsx://pages/user/card
  profile: tsx://pages/user/profile
}

@component(card) {
  relations: [posts]
}

@component(profile) {
  relations: *
}
```

**TODO**: Decide on syntax for relation inclusion.

---

### 2. Component Stub Generation (CONFIRMED)

**Decision**: Generate stub components with proper TypeScript types following `page.tsx` convention.

**Questions**:
- Should stubs include basic structure (e.g., display all fields)?
- Should stubs include example relation rendering?
- Should we support templates (minimal, detailed, custom)?

**Example - Minimal Stub**:
```tsx
// pages/user/card/page.tsx
export default function Page({ data }: UserCardProps) {
  return (
    <div>
      {/* TODO: Implement UserCard */}
    </div>
  );
}
```

**Example - Detailed Stub**:
```tsx
// pages/user/card/page.tsx
export default function Page({ data, relations }: UserCardProps) {
  return (
    <div className="user-card">
      <div className="user-email">{data.email}</div>
      <div className="user-id">{data.id}</div>

      {relations?.posts && (
        <div className="user-posts">
          <h3>Posts ({relations.posts.length})</h3>
          {relations.posts.map(post => (
            <div key={post.id}>{post.title}</div>
          ))}
        </div>
      )}
    </div>
  );
}
```

---

### 3. Route Handler Generation (NEW)

**Purpose**: Custom API endpoints beyond generated CRUD operations.

**Schema Reference**:
```forge
User {
  id: +uuid
  email: string

  # Custom endpoints
  verify_email: api://routes/user/verify
  reset_password: api://routes/user/reset
}
```

**Generated Structure**:
```
routes/
├── user/
│   ├── verify/
│   │   ├── post.ts         # POST handler
│   │   └── get.ts          # GET handler (optional)
│   └── reset/
│       └── post.ts         # POST handler
```

**Handler Signature**:
```typescript
// routes/user/verify/post.ts
export default async function handler(req: Request): Promise<Response> {
  // Access to model context via request
  const { userId } = await req.json();

  // TODO: Implement logic
  // Can call Rust API endpoints until Sprint 24 FFI is ready

  return new Response(JSON.stringify({ success: true }), {
    headers: { 'Content-Type': 'application/json' },
  });
}
```

**Questions**:
- Should handlers have automatic DB access (via Sprint 24 FFI)?
- Should handlers receive model-specific context (e.g., `userId` from route)?
- Should we generate OpenAPI docs for custom endpoints?
- Should handlers support middleware/auth hooks?

---

### 4. Server-Side Rendering (CONFIRMED - BUN RUNTIME)

**Decision**: Use Bun runtime with React SSR for component rendering.

See `SPRINT17_BUN_RUNTIME_DESIGN.md` for complete technical design.

**Key Points**:
- Bun server runs on port 3001
- Rust API server runs on port 3000
- Axum reverse proxy routes requests
- Components are rendered server-side via `renderToReadableStream`
- **Database access**: Via Rust API calls (FFI integration in Sprint 24)

**Component Routing**:
```
GET /pages/user/card/123 → Bun renders pages/user/card/page.tsx
```

**Handler Routing**:
```
POST /api/user/verify → Bun executes routes/user/verify/post.ts
```

**Data Fetching (Temporary - Sprint 17)**:
```typescript
// Components call Rust API for data
const response = await fetch(`http://localhost:3000/api/users/${id}`);
const user = await response.json();
```

**Data Fetching (Future - Sprint 24)**:
```typescript
// Direct DB access via FFI
const user = await db.users.get(id);
```

---

### 5. File Organization

**Confirmed Structure**:
```
project/
├── schema.forge
├── generated/
│   ├── types/
│   │   ├── user.ts            # User type
│   │   └── props.ts           # UserCardProps, etc.
│   └── stubs/
│       ├── pages/
│       │   └── user/
│       │       ├── card/
│       │       │   └── page.tsx.stub
│       │       └── profile/
│       │           └── page.tsx.stub
│       └── routes/
│           └── user/
│               └── verify/
│                   └── post.ts.stub
├── pages/                     # User implementations
│   └── user/
│       ├── card/
│       │   └── page.tsx
│       └── profile/
│           └── page.tsx
└── routes/                    # User implementations
    └── user/
        └── verify/
            ├── post.ts
            └── get.ts
```

**Stub Generation Strategy**:
- Generate `.stub` files in `generated/` directory
- User copies/modifies stubs into `pages/` or `routes/`
- Never overwrite user implementations
- Regenerate stubs on schema changes

---

### 6. Integration Points (DEFERRED - NEEDS DISCUSSION)

**High-level question**: Beyond types and stubs, what else gets generated?

#### Potential Levels:

**Level 1: Types + Stubs Only (Sprint 17 Target)**
- Generate `UserCardProps` types
- Generate `page.tsx` stubs
- Generate `post.ts` handler stubs
- Developer implements everything else

**Level 2: Types + Stubs + Routing (Future)**
- Generate props types
- Generate component/handler stubs
- Generate Bun server routing configuration
- Automatic route registration

**Level 3: Full Integration (Future)**
- Generate props types
- Generate component/handler stubs
- Generate server config with routing
- Generate data-fetching utilities
- Everything wired together automatically

**Sprint 17 Focus**: Level 1 (Types + Stubs)

**Sub-questions for future sprints**:
- Automatic data fetching in components?
- Cache integration for rendered HTML?
- Error boundary generation?
- Loading state components?
- Auth/middleware for handlers?

---

## Implementation Phases

### Phase 1: Schema Parsing & Validation
- Parse component field syntax: `card: tsx://pages/user/card`
- Parse route handler syntax: `verify: api://routes/user/verify`
- Support protocol prefixes: `tsx://`, `api://`
- Validate paths follow naming conventions
- Parse relation inclusion directives (TBD syntax)

### Phase 2: Type Generation
- Generate TypeScript props types for components
- Include model data (`data: User`)
- Include optional computed fields (`computed?: UserComputed`)
- Include optional relations based on directives (`relations?: { ... }`)
- Generate handler request/response types

### Phase 3: Component & Handler Stub Generation
- Generate `page.tsx` stubs with proper imports
- Generate HTTP method handler stubs (`post.ts`, `get.ts`, etc.)
- Include TypeScript prop/handler types
- Choose stub detail level (minimal vs. detailed)
- Handle file conflicts (don't overwrite existing)

### Phase 4: Bun Server Integration
- Implement component rendering server
- Implement route handler execution
- Implement IPC/sync with Rust API server
- Integrate reverse proxy routing
- **Database access**: Via Rust API (FFI in Sprint 24)

### Phase 5: Hot Reload & DX
- Watch schema changes
- Regenerate types on component/handler field changes
- Regenerate stubs when models change
- Preserve developer customizations
- Clear terminal display

---

## Technical Considerations

### Parser Changes
- New field types: `ComponentReference`, `RouteHandlerReference`
- Parse URI scheme: `protocol://path`
- Support `@relations` directive (or chosen syntax)
- Validate paths follow conventions (`page.tsx`, `{method}.ts`)

### Code Generator Changes
- New module: `typescript_props_generator`
- New module: `component_stub_generator`
- New module: `handler_stub_generator`
- New module: `bun_server_generator`

### CLI Integration
- `forgedb generate` generates types and stubs
- `forgedb dev` watches schema and starts Bun server
- Possibly: `forgedb pages scaffold <model>/<name>` for manual creation
- Possibly: `forgedb routes scaffold <model>/<name> <method>` for handlers

### Type System Integration
- Component/handler fields are metadata only (not stored)
- Don't generate storage for these fields
- Don't include in CRUD operations
- Include in schema documentation

---

## Success Criteria

**Minimum Viable Sprint 17**:
- [x] Parse component and route handler field syntax from schema
- [x] Generate TypeScript props types for all component fields
- [x] Generate component stubs following `page.tsx` convention
- [x] Generate route handler stubs following `{method}.ts` convention
- [x] Support opt-in relation inclusion in component props
- [x] Implement Bun server for component rendering
- [x] Implement Bun server for route handler execution
- [x] Integrate reverse proxy (Axum)
- [x] IPC/sync between Rust and Bun servers
- [x] Hot reload updates types on schema change
- [x] Documentation and examples

**Extended Goals** (may move to future sprints):
- [ ] Sprint 24: Direct ForgeDB access from Bun via FFI
- [ ] Automatic routing configuration generation
- [ ] Data-fetching helpers
- [ ] Cache layer for rendered components
- [ ] Middleware/auth system for handlers

---

## Examples

### Example 1: Basic Card Component

**Schema**:
```forge
User {
  id: +uuid
  email: ^&string
  name: string

  card: tsx://pages/user/card
}
```

**Generated Type**:
```typescript
// generated/types/props.ts
export type UserCardProps = {
  data: User;
};
```

**Generated Stub**:
```tsx
// generated/stubs/pages/user/card/page.tsx.stub
import { UserCardProps } from '../../../../generated/types/props';

export default function Page({ data }: UserCardProps) {
  return (
    <div className="user-card">
      <h3>{data.name}</h3>
      <p>{data.email}</p>
    </div>
  );
}
```

**User Implementation** (copied from stub):
```tsx
// pages/user/card/page.tsx
import { UserCardProps } from '../../../generated/types/props';

export default function Page({ data }: UserCardProps) {
  return (
    <div className="user-card">
      <div className="avatar">
        <img src={`/avatars/${data.id}`} alt={data.name} />
      </div>
      <h3>{data.name}</h3>
      <p>{data.email}</p>
    </div>
  );
}
```

**Route**: `GET /pages/user/card/{id}` renders the component with user data

---

### Example 2: Profile with Relations

**Schema**:
```forge
User {
  id: +uuid
  email: string
  name: string
  posts: [Post]
  comments: [Comment]

  profile: tsx://pages/user/profile @relations(posts)
}

Post {
  id: +uuid
  title: string
  author: *User
}

Comment {
  id: +uuid
  text: string
  author: *User
}
```

**Generated Type**:
```typescript
// generated/types/props.ts
export type UserProfileProps = {
  data: User;
  relations?: {
    posts?: Post[];
  };
};
```

**Generated Stub**:
```tsx
// generated/stubs/pages/user/profile/page.tsx.stub
import { UserProfileProps } from '../../../../generated/types/props';

export default function Page({ data, relations }: UserProfileProps) {
  return (
    <div className="user-profile">
      <h1>{data.name}</h1>
      <p>{data.email}</p>

      {relations?.posts && (
        <section className="user-posts">
          <h2>Posts</h2>
          {relations.posts.map(post => (
            <article key={post.id}>
              <h3>{post.title}</h3>
            </article>
          ))}
        </section>
      )}
    </div>
  );
}
```

---

### Example 3: Custom Route Handler

**Schema**:
```forge
User {
  id: +uuid
  email: ^&string
  verified: bool

  verify_email: api://routes/user/verify
}
```

**Generated Stub**:
```typescript
// generated/stubs/routes/user/verify/post.ts.stub
import type { User } from '../../../../generated/types';

export default async function handler(req: Request): Promise<Response> {
  try {
    const { userId, token } = await req.json();

    // TODO: Implement email verification logic
    // Note: Direct DB access via FFI available in Sprint 24
    // For now, call Rust API:
    // const response = await fetch(`http://localhost:3000/api/users/${userId}`);
    // const user = await response.json();

    return new Response(JSON.stringify({ verified: true }), {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
    });
  } catch (error) {
    return new Response(JSON.stringify({ error: 'Verification failed' }), {
      status: 400,
      headers: { 'Content-Type': 'application/json' },
    });
  }
}
```

**User Implementation**:
```typescript
// routes/user/verify/post.ts
import type { User } from '../../../generated/types';

export default async function handler(req: Request): Promise<Response> {
  try {
    const { userId, token } = await req.json();

    // Call Rust API to verify token and update user
    const response = await fetch(`http://localhost:3000/api/users/${userId}`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ verified: true }),
    });

    if (!response.ok) {
      throw new Error('Failed to update user');
    }

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

**Routes**:
- `POST /api/user/verify` executes the handler
- Request: `{ "userId": "...", "token": "..." }`
- Response: `{ "verified": true }`

---

### Example 4: Multiple HTTP Methods

**Schema**:
```forge
Post {
  id: +uuid
  title: string
  content: string
  published: bool

  publish: api://routes/post/publish
}
```

**Generated Stubs**:

```typescript
// generated/stubs/routes/post/publish/post.ts.stub
export default async function handler(req: Request): Promise<Response> {
  // TODO: Implement POST /api/post/publish
  return new Response(JSON.stringify({ published: true }), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  });
}
```

```typescript
// generated/stubs/routes/post/publish/delete.ts.stub
export default async function handler(req: Request): Promise<Response> {
  // TODO: Implement DELETE /api/post/publish (unpublish)
  return new Response(JSON.stringify({ published: false }), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  });
}
```

**User Implementations**:

```typescript
// routes/post/publish/post.ts
export default async function handler(req: Request): Promise<Response> {
  const { postId } = await req.json();

  const response = await fetch(`http://localhost:3000/api/posts/${postId}`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ published: true }),
  });

  return response;
}
```

```typescript
// routes/post/publish/delete.ts
export default async function handler(req: Request): Promise<Response> {
  const { postId } = await req.json();

  const response = await fetch(`http://localhost:3000/api/posts/${postId}`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ published: false }),
  });

  return response;
}
```

**Routes**:
- `POST /api/post/publish` → publish post
- `DELETE /api/post/publish` → unpublish post

---

## Next Steps

1. **Decision Required**: Choose syntax for relation inclusion (Option A/B/C)
2. **Decision Required**: Choose component stub detail level
3. **Decision Required**: Handler authentication/middleware strategy
4. **Review**: Bun runtime design doc (see `SPRINT17_BUN_RUNTIME_DESIGN.md`)
5. **Create**: Sprint 24 placeholder for FFI integration
6. **Begin implementation** with Phase 1 (parsing)

---

## Related Sprints

- **Sprint 10**: TypeScript SDK generation (props follow similar pattern)
- **Sprint 12**: Computed fields (included in component props)
- **Sprint 13**: OpenAPI documentation (should document custom endpoints)
- **Sprint 24**: Bun FFI Runtime (direct ForgeDB access from Bun)

---

## References

**Bun Runtime Design**:
- See: `SPRINT17_BUN_RUNTIME_DESIGN.md`

**Technologies**:
- Bun: TypeScript runtime with SSR support
- React: `renderToReadableStream` for SSR
- Axum: Reverse proxy and routing
- Unix sockets: IPC between Rust and Bun

**Inspiration**:
- Next.js: `page.tsx` convention, app router
- Remix: File-based routing, data loading
- SvelteKit: File-based routing conventions
- Astro: Islands architecture

---

**Document Version**: 2.0
**Last Updated**: 2025-10-14
**Status**: Active Discussion
