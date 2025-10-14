# Sprint 17: UI Component Integration - Preparation Document

**Status**: Planning & Discussion
**Created**: 2025-10-14
**Sprint Goal**: Connect database schemas to UI components with server-side rendering support

---

## Overview

Sprint 17 aims to bridge the gap between ForgeDB schemas and UI components by allowing developers to reference components directly in their schema definitions. The system will generate TypeScript props types and component stubs that are automatically synchronized with schema changes.

---

## Core Concept

### Schema Syntax (CONFIRMED)

Components are referenced as fields in the schema:

```forge
User {
  id: +uuid
  email: string
  posts: [Post]

  card: jsx://components/UserCard.tsx
  profile: jsx://views/UserProfile.tsx
}
```

**Decision**: Use field syntax (as shown above) rather than directives like `@component(...)`.

---

## Generated Output

### TypeScript Props Types

```typescript
type UserCardProps = {
  data: User
  computed?: UserComputed
  relations?: {
    posts?: Post[]
  }
}
```

### Component Stub

```tsx
// components/UserCard.tsx
import { UserCardProps } from '../generated/types';

export default function UserCard({ data, computed, relations }: UserCardProps) {
  return (
    <div className="user-card">
      <h2>{data.email}</h2>
      {/* TODO: Implement component */}
    </div>
  );
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
  card: jsx://components/UserCard.tsx @relations(posts)

  # Include all relations
  profile: jsx://views/UserProfile.tsx @relations(*)

  # No relations (default)
  avatar: jsx://components/UserAvatar.tsx
}
```

#### Option B: Explicit relation fields
```forge
User {
  id: +uuid
  email: string
  posts: [Post]
  liked_posts: [Post]

  card: jsx://components/UserCard.tsx {
    relations: [posts]
  }

  profile: jsx://views/UserProfile.tsx {
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

  card: jsx://components/UserCard.tsx
  profile: jsx://views/UserProfile.tsx
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

**Decision**: Generate stub components with proper TypeScript types.

**Questions**:
- Should stubs include basic structure (e.g., display all fields)?
- Should stubs include example relation rendering?
- Should we support templates (minimal, detailed, custom)?

**Example - Minimal Stub**:
```tsx
export default function UserCard({ data }: UserCardProps) {
  return (
    <div>
      {/* TODO: Implement UserCard */}
    </div>
  );
}
```

**Example - Detailed Stub**:
```tsx
export default function UserCard({ data, relations }: UserCardProps) {
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

### 3. Server-Side Rendering (NEEDS DEEP DIVE)

**Focus**: HTML templates and React (JSX/TSX) with server-side rendering.

#### Rendering Approaches

##### Option A: Pure HTML Templates (Tera/Handlebars)
```forge
User {
  id: +uuid
  email: string
  card: html://templates/user-card.html
}
```

Generated template:
```html
<div class="user-card">
  <h2>{{data.email}}</h2>
  <span>ID: {{data.id}}</span>
</div>
```

**Pros**: Simple, no JS runtime needed, fast
**Cons**: Limited interactivity, no type safety in templates

##### Option B: React Server Components (renderToString)
```forge
User {
  id: +uuid
  email: string
  card: tsx://components/UserCard.tsx
}
```

Backend renders using `renderToString`:
```rust
// Pseudo-code
let html = render_react_component("UserCard", props)?;
```

**Pros**: Familiar React syntax, component reusability
**Cons**: Requires JS runtime integration (Node/Deno), more complex

##### Option C: Rust-based JSX (Yew SSR)
Use Yew's server-side rendering capabilities:
```rust
// Generated Rust component
fn user_card(props: UserCardProps) -> Html {
    html! {
        <div class="user-card">
            <h2>{ &props.data.email }</h2>
        </div>
    }
}
```

**Pros**: Pure Rust, no JS runtime, type-safe
**Cons**: Not standard React, different syntax, learning curve

##### Option D: Maud (Rust HTML Templates)
```rust
fn user_card(props: UserCardProps) -> Markup {
    html! {
        div.user-card {
            h2 { (props.data.email) }
            span { "ID: " (props.data.id) }
        }
    }
}
```

**Pros**: Pure Rust, clean syntax, fast
**Cons**: Not JSX, custom syntax

##### Option E: Hybrid - Support Multiple Engines
Allow developers to choose per-component:
- `html://` → Tera templates
- `tsx://` → React SSR (via Node)
- `rs://` → Rust components (Maud/Yew)

**Pros**: Flexibility, best tool for each use case
**Cons**: Complexity, multiple code generation paths

#### Open Questions for SSR

1. **Rendering Engine Selection**: Which approach(es) to implement?
   - Start with one and expand?
   - Support multiple from day one?
   - Let user configure preferred engine?

2. **Component Routes**: Should components automatically become API endpoints?
   ```
   GET /api/users/{id}/card → Returns rendered HTML
   GET /api/users/{id}/profile → Returns rendered HTML
   ```

   Or should they be called programmatically only?

3. **Data Fetching Inside Components**: Should components have access to the database?
   ```tsx
   // Option A: Props-only (explicit data passing)
   export default function UserCard({ data }: UserCardProps) {
     return <div>{data.email}</div>;
   }

   // Option B: DB-aware (implicit data fetching)
   export default async function UserCard({ userId }: { userId: string }) {
     const user = await db.users.get(userId); // Auto-injected?
     return <div>{user.email}</div>;
   }
   ```

4. **Component Composition**: How do components reference each other?
   ```tsx
   // Can UserProfile reference UserCard?
   import UserCard from './UserCard';

   export default function UserProfile({ data }: UserProfileProps) {
     return (
       <div>
         <UserCard data={data} />
         <div>Additional profile info...</div>
       </div>
     );
   }
   ```

5. **Hydration Strategy**: For components that need client-side interactivity:
   - Pure SSR (no hydration)?
   - SSR + hydration with islands architecture?
   - Full SSR + React hydration?

6. **Template Engine Features**:
   - Layouts/partials support?
   - Helper functions?
   - Conditional rendering syntax?
   - Loop syntax for collections?

---

### 4. File Organization

**Questions**:
- Where should generated components live?
  ```
  project/
  ├── schema.forge
  ├── generated/
  │   ├── types.ts       # Props types
  │   └── api.rs         # API code
  └── components/        # User-written or generated stubs?
      ├── UserCard.tsx
      └── UserProfile.tsx
  ```

- Should component files be:
  - Generated once (scaffold)?
  - Regenerated on schema change (overwrite)?
  - Generated with merge support (preserve custom code)?

**Possible Approach**:
```
project/
├── schema.forge
├── generated/
│   ├── types/
│   │   ├── user.ts            # User type
│   │   └── props.ts           # UserCardProps, UserProfileProps
│   └── components/
│       ├── UserCard.stub.tsx  # Generated stub (reference only)
│       └── UserProfile.stub.tsx
└── components/                # User implementations
    ├── UserCard.tsx           # Developer writes here
    └── UserProfile.tsx
```

---

### 5. Integration Points (DEFERRED - NEEDS DISCUSSION)

**High-level question**: Beyond types and stubs, what else gets generated?

#### Potential Levels:

**Level 1: Types Only**
- Generate `UserCardProps` types
- Developer handles everything else

**Level 2: Types + Helpers**
- Generate props types
- Generate data-fetching helpers
- Developer wires them together

**Level 3: Full Integration**
- Generate props types
- Generate component stubs
- Generate route handlers
- Generate data fetchers
- Everything wired together automatically

**TODO**: Deep dive required to determine scope and implementation approach.

**Sub-questions**:
- API endpoint generation for component rendering?
- Data-fetching layer (automatic relation loading)?
- Cache integration?
- Error boundary generation?
- Loading states?

---

## Implementation Phases

### Phase 1: Schema Parsing & Validation
- Parse component field syntax: `card: jsx://path/to/Component.tsx`
- Support protocol prefixes: `jsx://`, `tsx://`, `html://`
- Validate component paths
- Parse relation inclusion directives (TBD syntax)

### Phase 2: Type Generation
- Generate TypeScript props types
- Include model data (`data: User`)
- Include optional computed fields (`computed?: UserComputed`)
- Include optional relations based on directives (`relations?: { ... }`)
- Support all models with component fields

### Phase 3: Component Stub Generation
- Generate component stubs with proper imports
- Include TypeScript prop types
- Choose stub detail level (minimal vs. detailed)
- Handle file conflicts (don't overwrite existing)

### Phase 4: SSR Integration (TBD - depends on rendering approach)
- Integrate chosen rendering engine
- Generate rendering code
- Possibly generate route handlers
- Handle data fetching (if applicable)

### Phase 5: Hot Reload & DX
- Watch schema changes
- Regenerate types on component field changes
- Update stubs when models change
- Preserve developer customizations

---

## Technical Considerations

### Parser Changes
- New field type: `ComponentReference`
- Parse URI scheme: `protocol://path`
- Support `@relations` directive (or chosen syntax)
- Validate component paths exist (optional)

### Code Generator Changes
- New module: `typescript_props_generator`
- New module: `component_stub_generator`
- Possibly: `ssr_integration_generator`

### CLI Integration
- `forgedb generate` should generate types and stubs
- Possibly: `forgedb components scaffold` for manual stub creation
- Possibly: `forgedb components validate` to check implementations match types

### Type System Integration
- Component fields are metadata only (not stored)
- Don't generate storage for component fields
- Don't include in CRUD operations
- Include in schema documentation

---

## Success Criteria

**Minimum Viable Sprint 17**:
- [x] Parse component field syntax from schema
- [x] Generate TypeScript props types for all component fields
- [x] Generate component stubs (basic)
- [x] Support opt-in relation inclusion
- [x] Hot reload updates types on schema change
- [x] Documentation and examples

**Extended Goals** (may move to future sprints):
- [ ] SSR integration with chosen rendering engine
- [ ] Component route generation
- [ ] Data-fetching helpers
- [ ] Multiple rendering engine support

---

## Examples

### Example 1: Basic Card Component

**Schema**:
```forge
User {
  id: +uuid
  email: ^&string
  name: string

  card: tsx://components/UserCard.tsx
}
```

**Generated Type**:
```typescript
// generated/types.ts
export type User = {
  id: string;
  email: string;
  name: string;
};

export type UserCardProps = {
  data: User;
};
```

**Generated Stub**:
```tsx
// components/UserCard.tsx (generated once)
import { UserCardProps } from '../generated/types';

export default function UserCard({ data }: UserCardProps) {
  return (
    <div className="user-card">
      <h3>{data.name}</h3>
      <p>{data.email}</p>
    </div>
  );
}
```

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

  profile: tsx://views/UserProfile.tsx @relations(posts)
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
// generated/types.ts
export type UserProfileProps = {
  data: User;
  relations?: {
    posts?: Post[];
  };
};
```

**Generated Stub**:
```tsx
// views/UserProfile.tsx
import { UserProfileProps } from '../generated/types';

export default function UserProfile({ data, relations }: UserProfileProps) {
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

### Example 3: Multiple Components with Computed Fields

**Schema**:
```forge
User {
  id: +uuid
  first_name: string
  last_name: string
  full_name: string @computed
  email: string
  posts: [Post]
  post_count: u32 @computed

  avatar: tsx://components/UserAvatar.tsx
  card: tsx://components/UserCard.tsx @relations(*)
  profile: tsx://views/UserProfile.tsx @relations(posts)
}

Post {
  id: +uuid
  title: string
  author: *User
}
```

**Generated Types**:
```typescript
// generated/types.ts
export type UserComputed = {
  full_name?: string;
  post_count?: number;
};

export type UserAvatarProps = {
  data: User;
  computed?: UserComputed;
};

export type UserCardProps = {
  data: User;
  computed?: UserComputed;
  relations?: {
    posts?: Post[];
  };
};

export type UserProfileProps = {
  data: User;
  computed?: UserComputed;
  relations?: {
    posts?: Post[];
  };
};
```

---

## Next Steps

1. **Decision Required**: Choose syntax for relation inclusion (Option A/B/C)
2. **Decision Required**: Choose component stub detail level
3. **Deep Dive Required**: SSR rendering approach (#3)
   - Which rendering engine(s)?
   - Component routes?
   - Data fetching strategy?
   - Hydration approach?
4. **Deep Dive Required**: Integration points (#5)
   - Scope of code generation beyond types
   - Data-fetching layer design
   - Route generation strategy
5. **Create detailed design doc** after decisions are made
6. **Begin implementation** with Phase 1 (parsing)

---

## Related Sprints

- **Sprint 10**: TypeScript SDK generation (props follow similar pattern)
- **Sprint 12**: Computed fields (included in component props)
- **Sprint 13**: OpenAPI documentation (may document component endpoints)

---

## References

**Potential Libraries/Tools**:
- Tera: Rust templating engine (Jinja-like)
- Maud: Rust HTML templating with compile-time checking
- Yew: Rust framework with SSR support
- tsx-rs: Rust bindings for TypeScript/TSX
- Node.js integration: For React SSR (renderToString)

**Inspiration**:
- Next.js: React SSR framework
- Astro: Islands architecture
- Remix: Nested routes + data loading
- SvelteKit: Compiler + SSR
- HTMX: Hypermedia-driven components

---

**Document Version**: 1.0
**Last Updated**: 2025-10-14
**Status**: Active Discussion
